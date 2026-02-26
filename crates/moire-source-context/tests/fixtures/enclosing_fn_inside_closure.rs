fn run() {
    let v: Vec<u32> = vec![1, 2, 3];
    v.iter().for_each(|x| {
👉      spawn(async move { *x });
    });
}
