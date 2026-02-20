use std::sync::Arc;
use std::time::Duration;

use moire::spawn_tracked;
use moire::sync::Semaphore;

pub async fn run() -> Result<(), String> {
    let gate = Arc::new(Semaphore::new("demo.api_gate", 1));

    let holder_gate = Arc::clone(&gate);
    spawn_tracked("permit_holder", async move {
        let _permit = holder_gate
            .acquire_owned()
            .await
            .expect("holder should acquire initial permit");

        println!("permit_holder acquired the only permit and will hold it forever");
        tokio::time::sleep(Duration::from_secs(3600)).await;
    });

    let waiter_gate = Arc::clone(&gate);
    spawn_tracked("permit_waiter", async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        println!("permit_waiter requesting permit; this should block forever");

        let _permit = waiter_gate
            .acquire_owned()
            .await
            .expect("permit waiter unexpectedly acquired permit");
    });

    println!("example running. open moire-web and inspect demo.api_gate");
    println!("press Ctrl+C to exit");
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("failed waiting for Ctrl+C: {e}"))?;
    Ok(())
}
