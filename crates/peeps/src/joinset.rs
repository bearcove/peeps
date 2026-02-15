use std::future::Future;

/// Extension trait to make `tokio::task::JoinSet::spawn` preserve the peeps stack.
///
/// `JoinSet` is commonly used for task groups; if those tasks are spawned while
/// handling a request, we almost always want them to remain descendants of that
/// request/response node.
pub trait JoinSetExt<T> {
    fn spawn_peeps<F>(&mut self, label: &'static str, future: F)
    where
        F: Future<Output = T> + Send + 'static;
}

impl<T> JoinSetExt<T> for tokio::task::JoinSet<T>
where
    T: Send + 'static,
{
    fn spawn_peeps<F>(&mut self, label: &'static str, future: F)
    where
        F: Future<Output = T> + Send + 'static,
    {
        let parent = crate::stack::capture_top();
        self.spawn(async move {
            // Provide a stable root future node for the joinset task body.
            let fut = crate::peepable(future, label);

            // If we have a parent frame, seed it so we get parent->child edges.
            if let Some(parent) = parent {
                let fut = crate::stack::scope(&parent, fut);
                crate::stack::with_stack(fut).await
            } else {
                crate::stack::with_stack(fut).await
            }
        });
    }
}

