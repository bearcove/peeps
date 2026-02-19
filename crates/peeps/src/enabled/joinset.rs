use std::cell::RefCell;
use std::future::Future;

use super::futures::instrument_future;
use super::process::JoinSet;
use super::{register_current_task_scope, CrateContext, Source, UnqualSource, FUTURE_CAUSAL_STACK};

impl<T> JoinSet<T>
where
    T: Send + 'static,
{
    pub fn named(name: impl Into<String>, source: Source) -> Self {
        let name = name.into();
        let handle = super::handles::EntityHandle::new(
            format!("joinset.{name}"),
            peeps_types::EntityBody::Future,
            source,
        );
        Self {
            inner: tokio::task::JoinSet::new(),
            handle,
        }
    }

    pub fn with_name(name: impl Into<String>, source: Source) -> Self {
        #[allow(deprecated)]
        Self::named(name, source)
    }

    #[track_caller]
    pub fn spawn_with_cx<F>(&mut self, label: &'static str, future: F, cx: CrateContext)
    where
        F: Future<Output = T> + Send + 'static,
    {
        self.spawn_with_source(label, future, cx.join(UnqualSource::caller()));
    }

    pub fn spawn_with_source<F>(&mut self, label: &'static str, future: F, source: Source)
    where
        F: Future<Output = T> + Send + 'static,
    {
        let joinset_handle = self.handle.clone();
        self.inner.spawn(
            FUTURE_CAUSAL_STACK.scope(RefCell::new(Vec::new()), async move {
                let _task_scope = register_current_task_scope(label, source.clone());
                instrument_future(
                    label,
                    future,
                    source,
                    Some(joinset_handle.entity_ref()),
                    None,
                )
                .await
            }),
        );
    }

    #[track_caller]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[track_caller]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[track_caller]
    pub fn abort_all(&mut self) {
        self.inner.abort_all();
    }

    #[track_caller]
    pub fn join_next_with_cx(
        &mut self,
        cx: CrateContext,
    ) -> impl Future<Output = Option<Result<T, tokio::task::JoinError>>> + '_ {
        self.join_next_with_source(cx.join(UnqualSource::caller()))
    }

    pub fn join_next_with_source(
        &mut self,
        source: Source,
    ) -> impl Future<Output = Option<Result<T, tokio::task::JoinError>>> + '_ {
        let handle = self.handle.clone();
        let fut = self.inner.join_next();
        instrument_future(
            "joinset.join_next",
            fut,
            source,
            Some(handle.entity_ref()),
            None,
        )
    }
}
