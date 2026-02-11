//! peeps CLI tool
//!
//! Commands:
//! - `peeps` - Collect and serve dashboard (like `vx debug`)
//! - `peeps clean` - Clean stale dumps

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "clean" {
        peeps::clean_dumps();
        eprintln!("[peeps] Cleaned dump directory");
        return;
    }

    // Default: serve dashboard
    eprintln!("[peeps] Reading dumps from {}", peeps::DUMP_DIR);
    let dumps = peeps::read_all_dumps();

    if dumps.is_empty() {
        eprintln!("[peeps] No dumps found. Trigger with: kill -SIGUSR1 <pid>");
        std::process::exit(1);
    }

    eprintln!("[peeps] Found {} dumps:", dumps.len());
    for dump in &dumps {
        eprintln!(
            "  {} (pid {}): {} tasks, {} threads",
            dump.process_name,
            dump.pid,
            dump.tasks.len(),
            dump.threads.len()
        );
    }

    serve_dashboard(dumps).await.unwrap();
}

async fn serve_dashboard(dumps: Vec<peeps::ProcessDump>) -> std::io::Result<()> {
    let dumps_json = facet_json::to_string(&dumps)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let html = build_dashboard_html(&dumps_json);

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let url = format!("http://{addr}");

    eprintln!("[peeps] Dashboard: {url}");
    eprintln!("[peeps] Press Ctrl-C to stop.");

    // Open browser
    let _ = std::process::Command::new("open").arg(&url).status();

    // Simple HTTP server
    loop {
        let (mut stream, _) = listener.accept().await?;
        let html_clone = html.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await;

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                html_clone.len(),
                html_clone
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
    }
}

fn build_dashboard_html(dumps_json: &str) -> String {
    const TEMPLATE: &str = include_str!("debug_dashboard.html");
    TEMPLATE.replace("__DUMPS_JSON__", dumps_json)
}
