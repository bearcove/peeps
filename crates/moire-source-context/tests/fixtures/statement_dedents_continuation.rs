fn run() {
👉  spawn(async move {
        println!("hello");
        let _ = idle_rx.recv().await;
    })
    .named("blocked_receiver");
}
