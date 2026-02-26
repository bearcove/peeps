struct Foo;
impl Foo {
    pub async fn run(&self) {
        let a = 1;
👉      spawn(async move { a });
    }
}
