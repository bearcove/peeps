fn process(y: Option<u32>) {
👉  let x = match y {
        Some(1) => 1,
        Some(2) => 2,
        Some(3) => 3,
        Some(4) => 4,
        Some(5) => 5,
        Some(6) => 6,
        Some(7) => 7,
        Some(8) => 8,
        _ => 0,
    };
    println!("{x}");
}
