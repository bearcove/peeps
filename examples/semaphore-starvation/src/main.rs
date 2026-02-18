use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    peeps::init("example-semaphore-starvation");

    let gate = Arc::new(peeps::semaphore!("demo.api_gate", 1));

    let holder_gate = Arc::clone(&gate);
    peeps::spawn_tracked!("permit_holder", async move {
        let _permit = holder_gate
            .acquire_owned()
            .await
            .expect("holder should acquire initial permit");

        println!("permit_holder acquired the only permit and will hold it forever");
        tokio::time::sleep(Duration::from_secs(3600)).await;
    });

    let waiter_gate = Arc::clone(&gate);
    peeps::spawn_tracked!("permit_waiter", async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        println!("permit_waiter requesting permit; this should block forever");

        let _permit = peeps::peep!(waiter_gate.acquire_owned(), "gate.acquire.blocked")
            .await
            .expect("permit waiter unexpectedly acquired permit");
    });

    println!("example running. open peeps-web and inspect demo.api_gate");
    println!("press Ctrl+C to exit");
    let _ = tokio::signal::ctrl_c().await;
}
