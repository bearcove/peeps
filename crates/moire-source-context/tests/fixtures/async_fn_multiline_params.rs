👉 async fn establish_as_acceptor(
    &mut self,
    settings: ConnectionSettings,
    metadata: Metadata<'_>,
) -> Result<Handle> {
    let handle = session
        .establish(self.root_settings, self.metadata)
        .await?;
    Ok(handle)
}
