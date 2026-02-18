use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use dialoguer::theme::ColorfulTheme;
use dialoguer::FuzzySelect;

type AnyResult<T> = Result<T, String>;

struct Args {
    list: bool,
    requested: Option<String>,
}

struct Config {
    root_dir: PathBuf,
    examples_dir: PathBuf,
    last_file: PathBuf,
    peeps_listen: String,
    peeps_http: String,
    peeps_dashboard: String,
    no_open: bool,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> AnyResult<()> {
    let args = parse_args()?;
    let cfg = config_from_env()?;

    let examples = discover_examples(&cfg.examples_dir)?;
    let last = read_last_example(&cfg.last_file);
    let ordered = ordered_examples(&examples, last.as_deref());

    if args.list {
        for example in ordered {
            println!("{example}");
        }
        return Ok(());
    }

    let selected = if let Some(requested) = args.requested {
        resolve_requested(&examples, &requested)?
    } else {
        pick_interactively(&ordered, last.as_deref())?
    };

    let _ = fs::write(&cfg.last_file, format!("{selected}\n"));

    let manifest_path = cfg.examples_dir.join(&selected).join("Cargo.toml");
    if !manifest_path.is_file() {
        return Err(format!(
            "Resolved example '{selected}' has no Cargo.toml at '{}'.",
            manifest_path.display()
        ));
    }

    ensure_backend_not_running(&cfg.peeps_http)?;

    let interrupted = Arc::new(AtomicBool::new(false));
    {
        let interrupted = Arc::clone(&interrupted);
        ctrlc::set_handler(move || {
            interrupted.store(true, Ordering::SeqCst);
        })
        .map_err(|e| format!("failed to install Ctrl+C handler: {e}"))?;
    }

    println!(
        "Starting peeps-web on {} (ingest: {})",
        cfg.peeps_http, cfg.peeps_listen
    );
    let mut backend = spawn_backend(&cfg)?;
    wait_for_backend_health(&cfg.peeps_http, &mut backend)?;

    if !cfg.no_open {
        open_browser(&format!("http://{}", cfg.peeps_http));
    }

    println!(
        "Running example '{}' (PEEPS_DASHBOARD={})",
        selected, cfg.peeps_dashboard
    );
    let mut example = spawn_example(&cfg, &manifest_path)?;

    let exit = monitor_loop(&mut backend, &mut example, &interrupted)?;

    terminate_child_group(&mut example);
    terminate_child_group(&mut backend);

    match exit {
        MonitorExit::Interrupted => Ok(()),
        MonitorExit::Example(status) => exit_from_status(status),
        MonitorExit::Backend(status) => Err(format!(
            "peeps-web backend exited unexpectedly: {}",
            format_status(status)
        )),
    }
}

fn parse_args() -> AnyResult<Args> {
    let mut args = env::args().skip(1);
    let mut list = false;
    let mut requested: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--list" => {
                list = true;
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => {
                return Err(format!("Unknown option '{arg}'"));
            }
            _ => {
                if requested.is_some() {
                    return Err("Too many arguments".to_owned());
                }
                requested = Some(arg);
            }
        }
    }

    Ok(Args { list, requested })
}

fn print_help() {
    eprintln!("Usage: peeps-examples [--list] [example-name]");
}

fn config_from_env() -> AnyResult<Config> {
    let root_dir = workspace_root();
    let examples_dir = root_dir.join("examples");
    if !examples_dir.is_dir() {
        return Err(format!(
            "expected examples dir at '{}'",
            examples_dir.display()
        ));
    }

    let peeps_listen = env::var("PEEPS_LISTEN").unwrap_or_else(|_| "127.0.0.1:9119".to_owned());
    let peeps_http = env::var("PEEPS_HTTP").unwrap_or_else(|_| "127.0.0.1:9130".to_owned());
    let peeps_dashboard = env::var("PEEPS_DASHBOARD").unwrap_or_else(|_| peeps_listen.clone());
    let no_open = env::var("PEEPS_NO_OPEN").map(|v| v == "1").unwrap_or(false);

    let last_file = env::var("PEEPS_LAST_EXAMPLE_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root_dir.join(".run-example-last"));

    Ok(Config {
        root_dir,
        examples_dir,
        last_file,
        peeps_listen,
        peeps_http,
        peeps_dashboard,
        no_open,
    })
}

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("crate must live under <root>/crates/<name>")
        .to_path_buf()
}

fn discover_examples(examples_dir: &Path) -> AnyResult<Vec<String>> {
    let mut names = Vec::new();

    let entries = fs::read_dir(examples_dir).map_err(|e| {
        format!(
            "Failed to read examples dir '{}': {e}",
            examples_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {e}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if path.join("Cargo.toml").is_file() {
            let name = entry.file_name();
            names.push(name.to_string_lossy().to_string());
        }
    }

    names.sort();

    if names.is_empty() {
        return Err(format!("No examples found in {}", examples_dir.display()));
    }

    Ok(names)
}

fn read_last_example(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn ordered_examples(examples: &[String], last: Option<&str>) -> Vec<String> {
    let mut ordered = Vec::with_capacity(examples.len());

    if let Some(last_name) = last {
        if examples.iter().any(|e| e == last_name) {
            ordered.push(last_name.to_owned());
        }
    }

    for example in examples {
        if Some(example.as_str()) == last {
            continue;
        }
        ordered.push(example.clone());
    }

    ordered
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn resolve_requested(examples: &[String], requested: &str) -> AnyResult<String> {
    if let Some(exact) = examples.iter().find(|name| *name == requested) {
        return Ok(exact.clone());
    }

    if let Some(substring_match) = examples
        .iter()
        .find(|name| contains_case_insensitive(name, requested))
    {
        eprintln!(
            "Using closest example match '{}' for '{}'.",
            substring_match, requested
        );
        return Ok(substring_match.clone());
    }

    eprintln!("Unknown example '{requested}'. Available examples:");
    for example in examples {
        eprintln!("{example}");
    }
    Err("No matching example found".to_owned())
}

fn pick_interactively(ordered: &[String], last: Option<&str>) -> AnyResult<String> {
    if !io::stdin().is_terminal() {
        return Err(
            "No example name provided and interactive picker is unavailable (no TTY).".to_owned(),
        );
    }

    let mut labels = Vec::with_capacity(ordered.len());
    for example in ordered {
        if Some(example.as_str()) == last {
            labels.push(format!("◉ {example}  [last]"));
        } else {
            labels.push(format!("◇ {example}"));
        }
    }

    let theme = ColorfulTheme::default();
    let selection = FuzzySelect::with_theme(&theme)
        .with_prompt("Choose an example  ✨  (type to filter)")
        .items(&labels)
        .default(0)
        .interact_opt()
        .map_err(|e| format!("Interactive picker failed: {e}"))?;

    let idx = selection.ok_or_else(|| "Selection cancelled".to_owned())?;
    Ok(ordered[idx].clone())
}

fn ensure_backend_not_running(peeps_http: &str) -> AnyResult<()> {
    let health_url = format!("http://{peeps_http}/health");
    if http_get_ok(&health_url) {
        return Err(format!(
            "A peeps-web backend is already running at http://{peeps_http}. Stop it first, or set PEEPS_HTTP/PEEPS_LISTEN to alternate ports."
        ));
    }
    Ok(())
}

fn wait_for_backend_health(peeps_http: &str, backend: &mut Child) -> AnyResult<()> {
    let health_url = format!("http://{peeps_http}/health");
    for _ in 0..100 {
        if http_get_ok(&health_url) {
            return Ok(());
        }
        if let Some(status) = backend.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "peeps-web backend exited before becoming healthy: {}",
                format_status(status)
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err(format!(
        "Timed out waiting for peeps-web backend health at {health_url}"
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
        .args(["run", "-p", "peeps-web", "--", "--dev"])
        .env("PEEPS_LISTEN", &cfg.peeps_listen)
        .env("PEEPS_HTTP", &cfg.peeps_http)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    configure_process_group(&mut cmd);
    cmd.spawn()
        .map_err(|e| format!("failed to spawn peeps-web: {e}"))
}

fn spawn_example(cfg: &Config, manifest_path: &Path) -> AnyResult<Child> {
    let manifest = manifest_path
        .to_str()
        .ok_or_else(|| format!("non-utf8 manifest path: '{}'", manifest_path.display()))?;

    let mut cmd = Command::new("cargo");
    cmd.current_dir(&cfg.root_dir)
        .args(["run", "--manifest-path", manifest])
        .env("PEEPS_DASHBOARD", &cfg.peeps_dashboard)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    configure_process_group(&mut cmd);
    cmd.spawn()
        .map_err(|e| format!("failed to spawn example process: {e}"))
}

enum MonitorExit {
    Interrupted,
    Example(ExitStatus),
    Backend(ExitStatus),
}

fn monitor_loop(
    backend: &mut Child,
    example: &mut Child,
    interrupted: &AtomicBool,
) -> AnyResult<MonitorExit> {
    loop {
        if interrupted.load(Ordering::SeqCst) {
            return Ok(MonitorExit::Interrupted);
        }

        if let Some(status) = example.try_wait().map_err(|e| e.to_string())? {
            return Ok(MonitorExit::Example(status));
        }

        if let Some(status) = backend.try_wait().map_err(|e| e.to_string())? {
            return Ok(MonitorExit::Backend(status));
        }

        thread::sleep(Duration::from_millis(200));
    }
}

fn exit_from_status(status: ExitStatus) -> AnyResult<()> {
    if status.success() {
        Ok(())
    } else {
        Err(format!("example exited with {}", format_status(status)))
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
        // Send TERM to the whole process group.
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
