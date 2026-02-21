use facet::Facet;
use figue as args;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::Duration;

mod scenarios;

type AnyResult<T> = Result<T, String>;
pub(crate) const EXAMPLE_CHILD_MODE_ENV: &str = "MOIRE_EXAMPLES_CHILD_MODE";

#[derive(Facet, Debug)]
struct Cli {
    #[facet(flatten)]
    builtins: args::FigueBuiltins,
    #[facet(args::named, default)]
    no_web: bool,
    #[facet(args::named, default)]
    no_open: bool,
    #[facet(args::named, default)]
    moire_listen: Option<String>,
    #[facet(args::named, default)]
    moire_http: Option<String>,
    #[facet(args::subcommand)]
    command: CommandKind,
}

#[derive(Facet, Debug)]
#[repr(u8)]
enum CommandKind {
    ChannelFullStall,
    MutexLockOrderInversion,
    OneshotSenderLostInMap,
    #[cfg(feature = "roam")]
    RoamRpcStuckRequest,
    #[cfg(feature = "roam")]
    RoamRpcStuckRequestClient {
        #[facet(args::named)]
        peer_addr: String,
    },
    #[cfg(feature = "roam")]
    RoamRustSwiftStuckRequest,
    SemaphoreStarvation,
}

struct Config {
    root_dir: PathBuf,
    moire_listen: String,
    moire_http: String,
    no_open: bool,
    no_web: bool,
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run() -> AnyResult<()> {
    let cli = parse_cli()?;
    let cfg = config_from_cli(&cli);
    let child_mode = std::env::var_os(EXAMPLE_CHILD_MODE_ENV).is_some();

    if child_mode {
        ur_taking_me_with_you::die_with_parent();
        return dispatch_command(&cfg.root_dir, cli.command).await;
    }

    let mut backend = if cfg.no_web {
        None
    } else {
        ensure_backend_not_running(&cfg.moire_http)?;
        println!(
            "Starting moire-web on {} (ingest: {})",
            cfg.moire_http, cfg.moire_listen
        );
        let mut child = spawn_backend(&cfg)?;
        wait_for_backend_health(&cfg.moire_http, &mut child)?;
        if !cfg.no_open {
            open_browser(&format!("http://{}", cfg.moire_http));
        }
        // SAFETY: single-threaded at this point, before spawning scenario subprocess
        unsafe { std::env::set_var("MOIRE_DASHBOARD", &cfg.moire_listen) };
        Some(child)
    };

    let scenario_status = run_scenario_in_subprocess(&cfg, &cli.command)?;
    let scenario_result = scenario_status_to_result(&scenario_status);

    if let Some(child) = backend.as_mut() {
        if should_wait_for_ctrl_c_after_scenario(&scenario_status) {
            println!(
                "Scenario exited; moire-web is still running at http://{}. Press Ctrl+C to stop moire-web.",
                cfg.moire_http
            );
            tokio::signal::ctrl_c()
                .await
                .map_err(|e| format!("failed waiting for Ctrl+C: {e}"))?;
        }
        terminate_child_group(child);
    }

    scenario_result
}

fn parse_cli() -> AnyResult<Cli> {
    let figue_config = args::builder::<Cli>()
        .map_err(|e| format!("failed to build CLI schema: {e}"))?
        .cli(|cli| cli.strict())
        .help(|h| {
            h.program_name("moire-examples")
                .description("Run moire scenarios as subcommands")
                .version(option_env!("CARGO_PKG_VERSION").unwrap_or("dev"))
        })
        .build();

    args::Driver::new(figue_config)
        .run()
        .into_result()
        .map(|v| v.value)
        .map_err(|e| e.to_string())
}

fn config_from_cli(cli: &Cli) -> Config {
    let moire_listen = cli
        .moire_listen
        .as_ref()
        .map(|v| v.to_string())
        .or_else(|| std::env::var("MOIRE_LISTEN").ok())
        .unwrap_or_else(|| "127.0.0.1:9119".to_owned());

    let moire_http = cli
        .moire_http
        .as_ref()
        .map(|v| v.to_string())
        .or_else(|| std::env::var("MOIRE_HTTP").ok())
        .unwrap_or_else(|| "127.0.0.1:9130".to_owned());

    Config {
        root_dir: workspace_root(),
        moire_listen,
        moire_http,
        no_open: cli.no_open,
        no_web: cli.no_web,
    }
}

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("crate must live under <root>/crates/<name>")
        .to_path_buf()
}

async fn dispatch_command(_root_dir: &std::path::Path, command: CommandKind) -> AnyResult<()> {
    match command {
        CommandKind::ChannelFullStall => scenarios::channel_full_stall::run().await,
        CommandKind::MutexLockOrderInversion => scenarios::mutex_lock_order_inversion::run().await,
        CommandKind::OneshotSenderLostInMap => scenarios::oneshot_sender_lost_in_map::run().await,
        #[cfg(feature = "roam")]
        CommandKind::RoamRpcStuckRequest => scenarios::roam_rpc_stuck_request::run().await,
        #[cfg(feature = "roam")]
        CommandKind::RoamRpcStuckRequestClient { peer_addr } => {
            scenarios::roam_rpc_stuck_request::run_client_process(peer_addr.to_string()).await
        }
        #[cfg(feature = "roam")]
        CommandKind::RoamRustSwiftStuckRequest => {
            scenarios::roam_rust_swift_stuck_request::run(_root_dir).await
        }
        CommandKind::SemaphoreStarvation => scenarios::semaphore_starvation::run().await,
    }
}

fn ensure_backend_not_running(moire_http: &str) -> AnyResult<()> {
    let health_url = format!("http://{moire_http}/health");
    if http_get_ok(&health_url) {
        return Err(format!(
            "A moire-web backend is already running at http://{moire_http}. Stop it first, or set MOIRE_HTTP/MOIRE_LISTEN to alternate ports."
        ));
    }
    Ok(())
}

fn wait_for_backend_health(moire_http: &str, backend: &mut Child) -> AnyResult<()> {
    let health_url = format!("http://{moire_http}/health");
    for _ in 0..100 {
        if http_get_ok(&health_url) {
            return Ok(());
        }
        if let Some(status) = backend.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "moire-web backend exited before becoming healthy: {}",
                format_status(status)
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err(format!(
        "Timed out waiting for moire-web backend health at {health_url}"
    ))
}

fn http_get_ok(url: &str) -> bool {
    ureq::get(url)
        .call()
        .map(|r| r.status() < 400)
        .unwrap_or(false)
}

fn spawn_backend(cfg: &Config) -> AnyResult<Child> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&cfg.root_dir)
        .args(["run", "--bin", "moire-web", "--", "--dev"])
        .env("MOIRE_LISTEN", &cfg.moire_listen)
        .env("MOIRE_HTTP", &cfg.moire_http)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    configure_process_group(&mut cmd);
    ur_taking_me_with_you::spawn_dying_with_parent(cmd)
        .map_err(|e| format!("failed to spawn moire-web: {e}"))
}

fn run_scenario_in_subprocess(cfg: &Config, command: &CommandKind) -> AnyResult<ExitStatus> {
    let exe = std::env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.current_dir(&cfg.root_dir)
        .arg("--no-web")
        .env(EXAMPLE_CHILD_MODE_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    for arg in command_cli_args(command) {
        cmd.arg(arg);
    }

    let mut child = ur_taking_me_with_you::spawn_dying_with_parent(cmd)
        .map_err(|e| format!("failed to run scenario subprocess: {e}"))?;
    child
        .wait()
        .map_err(|e| format!("failed waiting for scenario subprocess: {e}"))
}

fn command_cli_args(command: &CommandKind) -> Vec<String> {
    match command {
        CommandKind::ChannelFullStall => vec!["channel-full-stall".to_string()],
        CommandKind::MutexLockOrderInversion => vec!["mutex-lock-order-inversion".to_string()],
        CommandKind::OneshotSenderLostInMap => vec!["oneshot-sender-lost-in-map".to_string()],
        #[cfg(feature = "roam")]
        CommandKind::RoamRpcStuckRequest => vec!["roam-rpc-stuck-request".to_string()],
        #[cfg(feature = "roam")]
        CommandKind::RoamRpcStuckRequestClient { peer_addr } => vec![
            "roam-rpc-stuck-request-client".to_string(),
            "--peer-addr".to_string(),
            peer_addr.to_string(),
        ],
        #[cfg(feature = "roam")]
        CommandKind::RoamRustSwiftStuckRequest => vec!["roam-rust-swift-stuck-request".to_string()],
        CommandKind::SemaphoreStarvation => vec!["semaphore-starvation".to_string()],
    }
}

fn scenario_status_to_result(status: &ExitStatus) -> AnyResult<()> {
    if status.success() {
        return Ok(());
    }
    Err(format!(
        "scenario subprocess failed: {}",
        format_status(*status)
    ))
}

fn should_wait_for_ctrl_c_after_scenario(status: &ExitStatus) -> bool {
    !was_ctrl_c_exit(status)
}

fn was_ctrl_c_exit(status: &ExitStatus) -> bool {
    if status.code() == Some(130) {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal() == Some(libc::SIGINT)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

#[cfg(unix)]
fn configure_process_group(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_cmd: &mut Command) {}

#[cfg(unix)]
fn terminate_child_group(child: &mut Child) {
    let pid = child.id() as i32;

    if child.try_wait().ok().flatten().is_none() {
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }

        for _ in 0..10 {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }

        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }

    let _ = child.wait();
}

#[cfg(not(unix))]
fn terminate_child_group(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn format_status(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => "signal".to_owned(),
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = Command::new("xdg-open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}
