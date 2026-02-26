pub async fn run(
    session: &mut Session,
    permit: Permit,
) -> Result<(), String> {
    let setup = 1;
    let before = 2;
    moire::task::spawn(
        async move {
            work().await;
        },
    )
👉  .named("permit_waiter");
    let after = 3;
    Ok(())
}
