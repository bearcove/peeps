use std::future::Future;

use facet::Facet;

/// Diagnostic wrapper around `tokio::task::JoinSet`.
///
/// JoinSet is a first-class resource node so spawned tasks can be attached as
/// descendants through canonical edges:
/// - parent `touches` joinset (on creation)
/// - joinset `spawned` child (on spawn, via peepable inside scope)
/// - joinset `touches` child (on poll, via auto-emit in stack::push)
pub struct JoinSet<T> {
    node_id: String,
    name: String,
    inner: tokio::task::JoinSet<T>,
}

#[derive(Facet)]
struct JoinSetNodeAttrs {
    #[facet(skip_unless_truthy)]
    name: Option<String>,
    #[facet(skip_unless_truthy)]
    cancelled: Option<bool>,
    #[facet(skip_unless_truthy)]
    close_cause: Option<String>,
}

fn joinset_attrs_json(name: Option<&str>, cancelled: bool, close_cause: Option<&str>) -> String {
    facet_json::to_string(&JoinSetNodeAttrs {
        name: name.map(str::to_string),
        cancelled: cancelled.then_some(true),
        close_cause: close_cause.map(str::to_string),
    })
    .expect("failed to build JoinSet attrs json")
}

impl<T> JoinSet<T>
where
    T: Send + 'static,
{
    /// Create a named joinset for friendlier frontend display.
    pub fn named(name: impl Into<String>) -> Self {
        let node_id = peeps_types::new_node_id("joinset");
        let name = name.into();
        #[cfg(feature = "diagnostics")]
        {
            crate::registry::register_node(peeps_types::Node {
                id: node_id.clone(),
                kind: peeps_types::NodeKind::JoinSet,
                label: Some(name.clone()),
                attrs_json: joinset_attrs_json(Some(&name), false, None),
            });
            // Parent touches joinset (historical interaction, not a blocking dependency).
            crate::stack::with_top(|src| crate::registry::touch_edge(src, &node_id));
        }
        Self {
            node_id,
            name,
            inner: tokio::task::JoinSet::new(),
        }
    }

    /// Compatibility alias for named constructors.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self::named(name)
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
        #[cfg(feature = "diagnostics")]
        {
            crate::registry::register_node(peeps_types::Node {
                id: self.node_id.clone(),
                kind: peeps_types::NodeKind::JoinSet,
                label: Some(self.name.clone()),
                attrs_json: joinset_attrs_json(Some(&self.name), true, Some("abort_all")),
            });
        }
    }

    pub async fn join_next(&mut self) -> Option<Result<T, tokio::task::JoinError>> {
        self.inner.join_next().await
    }
}

impl<T> Drop for JoinSet<T> {
    fn drop(&mut self) {
        #[cfg(feature = "diagnostics")]
        crate::registry::remove_node(&self.node_id);
    }
}
