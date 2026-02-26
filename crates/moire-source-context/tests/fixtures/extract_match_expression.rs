fn process() {
    let x = 1;
👉  match self.rx.recv().await {
        Ok(Some(msg)) => {
            let payload = msg.map(|m| m.payload);
            handle(payload);
        }
        Ok(None) => {}
        Err(e) => return Err(e),
    }
    let y = 2;
}
