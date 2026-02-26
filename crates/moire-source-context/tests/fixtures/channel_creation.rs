fn setup() {
👉  let (tx_a, rx_b) = mpsc::channel("memory_link.a→b", buffer);
    let (a, b) = memory_link_pair(64);
}
