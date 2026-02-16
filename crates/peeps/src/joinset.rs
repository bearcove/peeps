use std::future::Future;

/// Diagnostic wrapper around `tokio::task::JoinSet`.
///
/// JoinSet is a first-class resource node so spawned tasks can be attached as
/// descendants through canonical edges:
/// - parent `touches` joinset (on creation)
/// - joinset `spawned` child (on spawn, via peepable inside scope)
/// - joinset `touches` child (on poll, via auto-emit in stack::push)
pub struct JoinSet<T> {
    node_id: String,
    inner: tokio::task::JoinSet<T>,
}

impl<T> JoinSet<T>
where
    T: Send + 'static,
{
    pub fn new() -> Self {
        let node_id = peeps_types::new_node_id("joinset");
        #[cfg(feature = "diagnostics")]
        {
            crate::registry::register_node(peeps_types::Node {
                id: node_id.clone(),
                kind: peeps_types::NodeKind::JoinSet,
                label: Some("joinset".to_string()),
                attrs_json: "{}".to_string(),
            });
            // Parent touches joinset (historical interaction, not a blocking dependency).
            crate::stack::with_top(|src| crate::registry::touch_edge(src, &node_id));
        }
        Self {
            node_id,
            inner: tokio::task::JoinSet::new(),
        }
    }

    pub fn spawn<F>(&mut self, label: &'static str, future: F)
    where
        F: Future<Output = T> + Send + 'static,
    {
        let joinset_node_id = self.node_id.clone();
        self.inner.spawn(async move {
            // Scope under the joinset node FIRST, then create the peepable inside
            // the scope. This ensures peepable() sees joinset_node_id on the stack
            // and emits a spawned edge (joinset → child).
            let scoped = crate::stack::scope(&joinset_node_id, async move {
                crate::peepable(future, label).await
            });
            crate::stack::ensure(scoped).await
        });
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

    pub async fn join_next(&mut self) -> Option<Result<T, tokio::task::JoinError>> {
        self.inner.join_next().await
    }
}

impl<T> Default for JoinSet<T>
where
    T: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for JoinSet<T> {
    fn drop(&mut self) {
        #[cfg(feature = "diagnostics")]
        crate::registry::remove_node(&self.node_id);
    }
}
