pub async fn run() -> Result<(), String> {
    let setup = 1;
👉  spawn(async move {
        println!("line 1");
        println!("line 2");
        println!("line 3");
        println!("line 4");
        println!("line 5");
    })
    .named("bounded_sender");
    let teardown = 2;
}
