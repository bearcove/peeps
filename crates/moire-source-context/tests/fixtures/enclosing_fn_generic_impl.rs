struct Queue<T>;
impl<T: Send> Queue<T> {
    fn push(&mut self, value: T) {
👉      spawn(async move { value });
    }
}
