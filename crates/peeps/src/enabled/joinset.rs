use std::cell::RefCell;
use std::future::Future;

use super::process::JoinSet;
use super::Source;
use peeps_runtime::{
    instrument_future, register_current_task_scope, EntityHandle, FUTURE_CAUSAL_STACK,
};

impl<T> JoinSet<T>
where
    T: Send + 'static,
{
    pub fn named(name: impl Into<String>, source: Source) -> Self {
        let name = name.into();
        let handle = EntityHandle::new(
            format!("joinset.{name}"),
            peeps_types::EntityBody::Future(peeps_types::FutureEntity {}),
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

    #[doc(hidden)]
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

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn abort_all(&mut self) {
        self.inner.abort_all();
    }

    #[doc(hidden)]
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
