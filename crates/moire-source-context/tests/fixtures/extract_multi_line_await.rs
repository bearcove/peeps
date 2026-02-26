async fn do_work() {
    let handle = session
👉      .establish_as_acceptor(self.root_settings, self.metadata)
        .await?;
}
