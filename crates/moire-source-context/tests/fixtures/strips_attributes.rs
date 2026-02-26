impl Session {
    #[moire::instrument]
👉  pub async fn establish(self) -> Result<(Session<C>, ConnectionHandle), SessionError> {
        let (mut server_session, server_handle) = acceptor(server_conduit)
            .establish()
            .await
            .expect("server");
    }
}
