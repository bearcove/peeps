use crate::peeps::prelude::*;
use std::sync::Arc;
use std::sync::Barrier;
use std::time::Duration;
use tokio::sync::oneshot;

fn spawn_lock_order_worker(
    task_name: &'static str,
    first_name: &'static str,
    first: Arc<peeps::Mutex<()>>,
    second_name: &'static str,
    second: Arc<peeps::Mutex<()>>,
    ready_barrier: Arc<Barrier>,
    completed_tx: oneshot::Sender<()>,
) {
    crate::peeps::spawn_tracked(task_name, async move {
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
    peeps::__init_from_macro();

    let left = Arc::new(crate::peeps::mutex("demo.shared.left", ()));
    let right = Arc::new(crate::peeps::mutex("demo.shared.right", ()));
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

    crate::peeps::spawn_tracked("observer.alpha_completion", async move {
        let _ = crate::peeps::instrument_future(
            "deadlock.alpha.completion.await",
            alpha_done_rx,
            None,
            None,
        )
        .await;
        println!("observer.alpha_completion unexpectedly unblocked");
    });

    crate::peeps::spawn_tracked("observer.beta_completion", async move {
        let _ = crate::peeps::instrument_future(
            "deadlock.beta.completion.await",
            beta_done_rx,
            None,
            None,
        )
        .await;
        println!("observer.beta_completion unexpectedly unblocked");
    });

    crate::peeps::spawn_tracked("observer.async_heartbeat", async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            println!("async heartbeat: runtime is alive while worker threads are deadlocked");
        }
    });

    println!(
        "example running. two tracked tokio tasks should deadlock on demo.shared.left/demo.shared.right"
    );
    println!(
        "inspect deadlock.alpha.completion.await and deadlock.beta.completion.await in peeps-web"
    );
    println!("press Ctrl+C to exit");

    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("failed waiting for Ctrl+C: {e}"))?;
    Ok(())
}
