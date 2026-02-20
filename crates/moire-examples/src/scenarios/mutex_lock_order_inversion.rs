use crate::scenarios::spawn_tracked;
use moire::sync::Mutex;
use std::sync::Arc;
use std::sync::Barrier;
use std::time::Duration;
use tokio::sync::oneshot;

fn spawn_lock_order_worker(
    task_name: &'static str,
    first_name: &'static str,
    first: Arc<Mutex<()>>,
    second_name: &'static str,
    second: Arc<Mutex<()>>,
    ready_barrier: Arc<Barrier>,
    completed_tx: oneshot::Sender<()>,
) {
    spawn_tracked(task_name, async move {
        let _first_guard = first.lock();
        println!("{task_name} locked {first_name}; waiting for peer");

        ready_barrier.wait();

        println!(
            "{task_name} attempting {second_name}; this should deadlock due to lock-order inversion"
        );
        let _second_guard = second.lock();

        println!("{task_name} unexpectedly acquired {second_name}; deadlock did not occur");
        let _ = completed_tx.send(());
    });
}

pub async fn run() -> Result<(), String> {
    let left = Arc::new(Mutex::new("demo.shared.left", ()));
    let right = Arc::new(Mutex::new("demo.shared.right", ()));
    let ready_barrier = Arc::new(Barrier::new(2));

    let (alpha_done_tx, alpha_done_rx) = oneshot::channel::<()>();
    let (beta_done_tx, beta_done_rx) = oneshot::channel::<()>();

    spawn_lock_order_worker(
        "deadlock.worker.alpha",
        "demo.shared.left",
        Arc::clone(&left),
        "demo.shared.right",
        Arc::clone(&right),
        Arc::clone(&ready_barrier),
        alpha_done_tx,
    );

    spawn_lock_order_worker(
        "deadlock.worker.beta",
        "demo.shared.right",
        Arc::clone(&right),
        "demo.shared.left",
        Arc::clone(&left),
        Arc::clone(&ready_barrier),
        beta_done_tx,
    );

    spawn_tracked("observer.alpha_completion", async move {
        let _ = alpha_done_rx.await;
        println!("observer.alpha_completion unexpectedly unblocked");
    });

    spawn_tracked("observer.beta_completion", async move {
        let _ = beta_done_rx.await;
        println!("observer.beta_completion unexpectedly unblocked");
    });

    spawn_tracked("observer.async_heartbeat", async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            println!("async heartbeat: runtime is alive while worker threads are deadlocked");
        }
    });

    println!(
        "example running. two tracked tokio tasks should deadlock on demo.shared.left/demo.shared.right"
    );
    println!(
        "inspect deadlock.alpha.completion.await and deadlock.beta.completion.await in moire-web"
    );
    println!("press Ctrl+C to exit");

    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("failed waiting for Ctrl+C: {e}"))?;
    Ok(())
}
