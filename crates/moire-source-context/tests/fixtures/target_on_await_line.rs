async fn do_work() {
    let a = setup().await;
    let b = step_one().await;
    let c = step_two().await;
    let result = main_work()
👉      .await;
    let d = cleanup().await;
    let e = finalize().await;
}
