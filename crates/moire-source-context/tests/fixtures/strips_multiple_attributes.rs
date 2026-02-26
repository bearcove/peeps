#[allow(dead_code)]
#[moire::instrument]
👉 async fn recv(&mut self) -> Result<Option<Msg>> {
    self.rx.recv().await
}
