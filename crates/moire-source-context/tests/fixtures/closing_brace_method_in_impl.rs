struct Foo;

impl Foo {
    fn method_a(&self) {
        do_a();
        do_b();
        do_c();
    }

    fn getter(&self) -> u32 {
        self.value
👉  }

    fn method_b(&self) {
        do_x();
        do_y();
        do_z();
    }
}
