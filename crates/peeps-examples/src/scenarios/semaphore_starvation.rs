use std::sync::Arc;
use std::time::Duration;

use crate::peeps::prelude::*;

pub async fn run() -> Result<(), String> {
    peeps::__init_from_macro();

    let gate = Arc::new(crate::peeps::semaphore("demo.api_gate", 1));

    let holder_gate = Arc::clone(&gate);
    crate::peeps::spawn_tracked("permit_holder", async move {
        let _permit = holder_gate
            .acquire_owned()
            .await
            .expect("holder should acquire initial permit");

        println!("permit_holder acquired the only permit and will hold it forever");
        tokio::time::sleep(Duration::from_secs(3600)).await;
    });

    let waiter_gate = Arc::clone(&gate);
    crate::peeps::spawn_tracked("permit_waiter", async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        println!("permit_waiter requesting permit; this should block forever");

        let _permit = waiter_gate
            .acquire_owned()
            .tracked("gate.acquire.blocked")
            .await
            .expect("permit waiter unexpectedly acquired permit");
    });

    println!("example running. open peeps-web and inspect demo.api_gate");
    println!("press Ctrl+C to exit");
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("failed waiting for Ctrl+C: {e}"))?;
    Ok(())
}
