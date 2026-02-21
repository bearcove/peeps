use std::sync::Arc;

use facet::Facet;
use figue as args;
use moire_web::app::{AppState, DevProxyState, build_router};
use moire_web::db::{Db, init_sqlite, load_next_connection_id};
use moire_web::proxy::{DEFAULT_VITE_ADDR, start_vite_dev_server};
use moire_web::tcp::run_tcp_acceptor;
use tokio::net::TcpListener;
use tracing::{error, info};

#[derive(Facet, Debug)]
struct Cli {
    #[facet(flatten)]
    builtins: args::FigueBuiltins,
    #[facet(args::named, default)]
    dev: bool,
}

const REAPER_PIPE_FD_ENV: &str = "MOIRE_REAPER_PIPE_FD";
const REAPER_PGID_ENV: &str = "MOIRE_REAPER_PGID";

fn main() {
    // Reaper mode: watch the pipe, kill the process group when it closes.
    // Must NOT call die_with_parent() — we need to outlive the parent briefly.
    #[cfg(unix)]
    if let (Ok(fd_str), Ok(pgid_str)) = (
        std::env::var(REAPER_PIPE_FD_ENV),
        std::env::var(REAPER_PGID_ENV),
    ) && let (Ok(fd), Ok(pgid)) = (
        fd_str.parse::<libc::c_int>(),
        pgid_str.parse::<libc::pid_t>(),
    ) {
        reaper_main(fd, pgid);
        return;
    }

    ur_taking_me_with_you::die_with_parent();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(async {
            if let Err(err) = run().await {
                eprintln!("{err}");
                std::process::exit(1);
            }
        });
}

#[cfg(unix)]
fn reaper_main(pipe_fd: libc::c_int, pgid: libc::pid_t) {
    // Block until the parent closes the write end of the pipe (i.e. parent died).
    let mut buf = [0u8; 1];
    loop {
        let n = unsafe { libc::read(pipe_fd, buf.as_mut_ptr() as *mut _, 1) };
        if n <= 0 {
            break; // EOF or error — parent is gone
        }
    }
    // Kill the entire process group.
    unsafe {
        libc::kill(-pgid, libc::SIGTERM);
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
}

async fn run() -> Result<(), String> {
    let cli = parse_cli()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // r[impl config.web.tcp-listen]
    let tcp_addr = std::env::var("MOIRE_LISTEN").unwrap_or_else(|_| "127.0.0.1:9119".into());
    // r[impl config.web.http-listen]
    let http_addr = std::env::var("MOIRE_HTTP").unwrap_or_else(|_| "127.0.0.1:9130".into());
    // r[impl config.web.vite-addr]
    let vite_addr = std::env::var("MOIRE_VITE_ADDR").unwrap_or_else(|_| DEFAULT_VITE_ADDR.into());
    // r[impl config.web.db-path]
    let db_path = std::path::PathBuf::from(
        std::env::var("MOIRE_DB").unwrap_or_else(|_| "moire-web.sqlite".into()),
    );
    let db = Db::new(db_path);
    init_sqlite(&db).map_err(|e| format!("failed to init sqlite at {:?}: {e}", db.path()))?;
    let next_conn_id = load_next_connection_id(&db)
        .map_err(|e| format!("failed to load next connection id at {:?}: {e}", db.path()))?;

    let mut dev_vite_child = None;
    let dev_proxy = if cli.dev {
        let child = start_vite_dev_server(&vite_addr).await?;
        info!(vite_addr = %vite_addr, "moire-web --dev launched Vite");
        dev_vite_child = Some(child);
        Some(DevProxyState {
            base_url: Arc::new(format!("http://{vite_addr}")),
        })
    } else {
        None
    };

    let state = AppState::new(db, next_conn_id, dev_proxy);

    let tcp_listener = TcpListener::bind(&tcp_addr)
        .await
        .map_err(|e| format!("failed to bind TCP on {tcp_addr}: {e}"))?;
    info!(%tcp_addr, %next_conn_id, "moire-web TCP ingest listener ready");

    let http_listener = TcpListener::bind(&http_addr)
        .await
        .map_err(|e| format!("failed to bind HTTP on {http_addr}: {e}"))?;
    if cli.dev {
        info!(%http_addr, vite_addr = %vite_addr, "moire-web HTTP API + Vite proxy ready");
    } else {
        info!(%http_addr, "moire-web HTTP API ready");
    }
    print_startup_hints(
        &http_addr,
        &tcp_addr,
        if cli.dev { Some(&vite_addr) } else { None },
    );

    let app = build_router(state.clone());

    let _dev_vite_child = dev_vite_child;
    tokio::select! {
        _ = run_tcp_acceptor(tcp_listener, state.clone()) => {}
        result = axum::serve(http_listener, app) => {
            if let Err(e) = result {
                error!(%e, "HTTP server error");
            }
        }
    }
    Ok(())
}

fn parse_cli() -> Result<Cli, String> {
    let figue_config = args::builder::<Cli>()
        .map_err(|e| format!("failed to build CLI schema: {e}"))?
        .cli(|cli| cli.strict())
        .help(|h| {
            h.program_name("moire-web")
                .description("SQLite-backed moire ingest + API server")
                .version(option_env!("CARGO_PKG_VERSION").unwrap_or("dev"))
        })
        .build();
    let cli = args::Driver::new(figue_config)
        .run()
        .into_result()
        .map_err(|e| e.to_string())?;
    Ok(cli.value)
}

fn print_startup_hints(http_addr: &str, tcp_addr: &str, vite_addr: Option<&str>) {
    let mode = if vite_addr.is_some() {
        "dev proxy"
    } else {
        "api only"
    };
    println!();
    println!();

    if let Some(vite_addr) = vite_addr {
        println!("  Vite dev server (managed): http://{vite_addr}");
        println!();
    }

    println!("  moire-web ready ({mode})");
    println!();
    println!("  \x1b[32mOpen in browser: http://{http_addr}\x1b[0m");
    println!();
    println!("  Connect apps with:");
    println!("    \x1b[32mMOIRE_DASHBOARD={tcp_addr}\x1b[0m <your-binary>");
    println!();
    println!();
}
