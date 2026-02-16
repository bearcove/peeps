use std::ffi::{OsStr, OsString};
use std::io;
use std::process::{ExitStatus, Output, Stdio};
use std::time::Instant;

use facet::Facet;
use peeps_types::{Node, NodeKind};

/// Maximum length for the args preview string.
const ARGS_PREVIEW_MAX: usize = 200;

/// Truncate a string to a maximum byte length, appending "..." if truncated.
fn truncate_preview(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Find the last char boundary at or before max-3
        let end = s
            .char_indices()
            .take_while(|&(i, _)| i <= max.saturating_sub(3))
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        let mut out = s[..end].to_string();
        out.push_str("...");
        out
    }
}

// ── Attrs struct ─────────────────────────────────────────

#[derive(Facet)]
struct CommandAttrs {
    #[facet(rename = "cmd.program")]
    cmd_program: String,
    #[facet(rename = "cmd.args_preview")]
    cmd_args_preview: String,
    #[facet(rename = "cmd.cwd")]
    #[facet(skip_unless_truthy)]
    cmd_cwd: Option<String>,
    #[facet(rename = "cmd.env_count")]
    cmd_env_count: u64,
    #[facet(rename = "process.pid")]
    #[facet(skip_unless_truthy)]
    process_pid: Option<u64>,
    #[facet(rename = "exit.code")]
    #[facet(skip_unless_truthy)]
    exit_code: Option<i64>,
    #[facet(rename = "exit.signal")]
    #[facet(skip_unless_truthy)]
    exit_signal: Option<String>,
    #[facet(skip_unless_truthy)]
    elapsed_ns: Option<u64>,
    #[facet(skip_unless_truthy)]
    error: Option<String>,
}

fn build_attrs_json(
    program: &str,
    args_preview: &str,
    cwd: Option<&str>,
    env_count: usize,
    pid: Option<u32>,
    exit_code: Option<i32>,
    exit_signal: Option<&str>,
    elapsed_ns: Option<u64>,
    error: Option<&str>,
) -> String {
    let attrs = CommandAttrs {
        cmd_program: program.to_owned(),
        cmd_args_preview: args_preview.to_owned(),
        cmd_cwd: cwd.map(|s| s.to_owned()),
        cmd_env_count: env_count as u64,
        process_pid: pid.map(|p| p as u64),
        exit_code: exit_code.map(|c| c as i64),
        exit_signal: exit_signal.map(|s| s.to_owned()),
        elapsed_ns,
        error: error.map(|s| s.to_owned()),
    };
    facet_json::to_string(&attrs).unwrap()
}

/// Diagnostic wrapper around `tokio::process::Command`.
///
/// Registers a graph node on execution with program, args, timing, and exit info.
pub struct Command {
    inner: tokio::process::Command,
    program: String,
    args: Vec<String>,
    cwd: Option<String>,
    env_count: usize,
    kill_on_drop: bool,
}

/// Captured command metadata used when spawning through external helpers.
#[derive(Clone, Debug)]
pub struct CommandDiagnostics {
    pub program: String,
    pub args_preview: String,
    pub cwd: Option<String>,
    pub env_count: usize,
}

impl Command {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        let program_str = program.as_ref().to_string_lossy().to_string();
        Self {
            inner: tokio::process::Command::new(program),
            program: program_str,
            args: Vec::new(),
            cwd: None,
            env_count: 0,
            kill_on_drop: false,
        }
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args
            .push(arg.as_ref().to_string_lossy().to_string());
        self.inner.arg(arg);
        self
    }

    pub fn args(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        let args: Vec<OsString> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        for a in &args {
            self.args.push(a.to_string_lossy().to_string());
        }
        self.inner.args(args);
        self
    }

    pub fn env(&mut self, key: impl AsRef<OsStr>, val: impl AsRef<OsStr>) -> &mut Self {
        self.env_count += 1;
        self.inner.env(key, val);
        self
    }

    pub fn envs(
        &mut self,
        vars: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    ) -> &mut Self {
        let vars: Vec<(OsString, OsString)> = vars
            .into_iter()
            .map(|(k, v)| (k.as_ref().to_owned(), v.as_ref().to_owned()))
            .collect();
        self.env_count += vars.len();
        self.inner.envs(vars);
        self
    }

    pub fn env_clear(&mut self) -> &mut Self {
        self.env_count = 0;
        self.inner.env_clear();
        self
    }

    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        // env_count is an approximation; env_remove may not reduce it below 0
        self.env_count = self.env_count.saturating_sub(1);
        self.inner.env_remove(key);
        self
    }

    pub fn current_dir(&mut self, dir: impl AsRef<std::path::Path>) -> &mut Self {
        self.cwd = Some(dir.as_ref().to_string_lossy().to_string());
        self.inner.current_dir(dir);
        self
    }

    pub fn stdin(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stdin(cfg);
        self
    }

    pub fn stdout(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stdout(cfg);
        self
    }

    pub fn stderr(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stderr(cfg);
        self
    }

    pub fn kill_on_drop(&mut self, kill_on_drop: bool) -> &mut Self {
        self.kill_on_drop = kill_on_drop;
        self.inner.kill_on_drop(kill_on_drop);
        self
    }

    fn args_preview(&self) -> String {
        let joined = self.args.join(" ");
        truncate_preview(&joined, ARGS_PREVIEW_MAX)
    }

    fn register_node(&self, node_id: &str, pid: Option<u32>) {
        let attrs = build_attrs_json(
            &self.program,
            &self.args_preview(),
            self.cwd.as_deref(),
            self.env_count,
            pid,
            None,
            None,
            None,
            None,
        );

        crate::registry::register_node(Node {
            id: node_id.to_string(),
            kind: NodeKind::Command,
            label: Some(self.program.clone()),
            attrs_json: attrs,
        });

        // Emit touch edge: parent stack top → command node
        let nid = node_id.to_string();
        crate::stack::with_top(|src| {
            crate::registry::touch_edge(src, &nid);
        });
    }

    pub fn spawn(&mut self) -> io::Result<Child> {
        let node_id = peeps_types::new_node_id("command");
        let start = Instant::now();

        match self.inner.spawn() {
            Ok(child) => {
                let pid = child.id();
                self.register_node(&node_id, pid);

                Ok(Child {
                    inner: Some(child),
                    node_id,
                    start,
                    program: self.program.clone(),
                    args_preview: self.args_preview(),
                    cwd: self.cwd.clone(),
                    env_count: self.env_count,
                })
            }
            Err(e) => {
                // Register a short-lived node for the failed spawn
                self.register_node(&node_id, None);
                let attrs = build_attrs_json(
                    &self.program,
                    &self.args_preview(),
                    self.cwd.as_deref(),
                    self.env_count,
                    None,
                    None,
                    None,
                    Some(start.elapsed().as_nanos() as u64),
                    Some(&e.to_string()),
                );
                crate::registry::register_node(Node {
                    id: node_id.clone(),
                    kind: NodeKind::Command,
                    label: Some(self.program.clone()),
                    attrs_json: attrs,
                });
                crate::registry::remove_node(&node_id);
                Err(e)
            }
        }
    }

    pub async fn status(&mut self) -> io::Result<ExitStatus> {
        let node_id = peeps_types::new_node_id("command");
        let start = Instant::now();
        self.register_node(&node_id, None);
        crate::stack::with_top(|src| {
            crate::registry::touch_edge(src, &node_id);
            crate::registry::edge(src, &node_id);
        });

        let result = self.inner.status().await;
        let elapsed_ns = start.elapsed().as_nanos() as u64;

        let (exit_code, exit_signal, error) = match &result {
            Ok(status) => (status.code(), exit_signal_str(status), None),
            Err(e) => (None, None, Some(e.to_string())),
        };

        let attrs = build_attrs_json(
            &self.program,
            &self.args_preview(),
            self.cwd.as_deref(),
            self.env_count,
            None,
            exit_code,
            exit_signal.as_deref(),
            Some(elapsed_ns),
            error.as_deref(),
        );
        crate::registry::register_node(Node {
            id: node_id.clone(),
            kind: NodeKind::Command,
            label: Some(self.program.clone()),
            attrs_json: attrs,
        });
        crate::registry::remove_node(&node_id);

        result
    }

    pub async fn output(&mut self) -> io::Result<Output> {
        let node_id = peeps_types::new_node_id("command");
        let start = Instant::now();
        self.register_node(&node_id, None);
        crate::stack::with_top(|src| {
            crate::registry::touch_edge(src, &node_id);
            crate::registry::edge(src, &node_id);
        });

        let result = self.inner.output().await;
        let elapsed_ns = start.elapsed().as_nanos() as u64;

        let (exit_code, exit_signal, error) = match &result {
            Ok(output) => (
                output.status.code(),
                exit_signal_str(&output.status),
                None,
            ),
            Err(e) => (None, None, Some(e.to_string())),
        };

        let attrs = build_attrs_json(
            &self.program,
            &self.args_preview(),
            self.cwd.as_deref(),
            self.env_count,
            None,
            exit_code,
            exit_signal.as_deref(),
            Some(elapsed_ns),
            error.as_deref(),
        );
        crate::registry::register_node(Node {
            id: node_id.clone(),
            kind: NodeKind::Command,
            label: Some(self.program.clone()),
            attrs_json: attrs,
        });
        crate::registry::remove_node(&node_id);

        result
    }

    pub fn as_std(&self) -> &std::process::Command {
        self.inner.as_std()
    }

    #[cfg(unix)]
    pub unsafe fn pre_exec<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut() -> io::Result<()> + Send + Sync + 'static,
    {
        self.inner.pre_exec(f);
        self
    }

    pub fn into_inner(self) -> tokio::process::Command {
        self.inner
    }

    pub fn into_inner_with_diagnostics(self) -> (tokio::process::Command, CommandDiagnostics) {
        let diag = CommandDiagnostics {
            program: self.program,
            args_preview: self.args_preview(),
            cwd: self.cwd,
            env_count: self.env_count,
        };
        (self.inner, diag)
    }
}

/// Diagnostic wrapper around `tokio::process::Child`.
///
/// Registered as a graph node for the lifetime of the child process.
pub struct Child {
    // Option so wait_with_output can .take() without conflicting with Drop.
    inner: Option<tokio::process::Child>,
    node_id: String,
    start: Instant,
    program: String,
    args_preview: String,
    cwd: Option<String>,
    env_count: usize,
}

impl Child {
    pub fn from_tokio_with_diagnostics(
        child: tokio::process::Child,
        diag: CommandDiagnostics,
    ) -> Self {
        let node_id = peeps_types::new_node_id("command");
        let pid = child.id();
        let attrs = build_attrs_json(
            &diag.program,
            &diag.args_preview,
            diag.cwd.as_deref(),
            diag.env_count,
            pid,
            None,
            None,
            None,
            None,
        );
        crate::registry::register_node(Node {
            id: node_id.clone(),
            kind: NodeKind::Command,
            label: Some(diag.program.clone()),
            attrs_json: attrs,
        });
        crate::stack::with_top(|src| {
            crate::registry::touch_edge(src, &node_id);
            crate::registry::edge(src, &node_id);
        });

        Self {
            inner: Some(child),
            node_id,
            start: Instant::now(),
            program: diag.program,
            args_preview: diag.args_preview,
            cwd: diag.cwd,
            env_count: diag.env_count,
        }
    }
}

impl Child {
    fn inner(&self) -> &tokio::process::Child {
        self.inner.as_ref().expect("Child already consumed")
    }

    fn inner_mut(&mut self) -> &mut tokio::process::Child {
        self.inner.as_mut().expect("Child already consumed")
    }

    pub fn id(&self) -> Option<u32> {
        self.inner().id()
    }

    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        crate::stack::with_top(|src| {
            crate::registry::touch_edge(src, &self.node_id);
            crate::registry::edge(src, &self.node_id);
        });
        let result = self.inner_mut().wait().await;
        self.update_node_on_exit(&result);
        result
    }

    pub async fn wait_with_output(mut self) -> io::Result<Output> {
        crate::stack::with_top(|src| {
            crate::registry::touch_edge(src, &self.node_id);
            crate::registry::edge(src, &self.node_id);
        });
        let inner = self.inner.take().expect("Child already consumed");
        let pid = inner.id();

        let result = inner.wait_with_output().await;
        let elapsed_ns = self.start.elapsed().as_nanos() as u64;

        let (exit_code, exit_signal, error) = match &result {
            Ok(output) => (
                output.status.code(),
                exit_signal_str(&output.status),
                None,
            ),
            Err(e) => (None, None, Some(e.to_string())),
        };

        let attrs = build_attrs_json(
            &self.program,
            &self.args_preview,
            self.cwd.as_deref(),
            self.env_count,
            pid,
            exit_code,
            exit_signal.as_deref(),
            Some(elapsed_ns),
            error.as_deref(),
        );
        crate::registry::register_node(Node {
            id: self.node_id.clone(),
            kind: NodeKind::Command,
            label: Some(self.program.clone()),
            attrs_json: attrs,
        });
        // Drop will handle remove_node

        result
    }

    pub fn start_kill(&mut self) -> io::Result<()> {
        self.inner_mut().start_kill()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.start_kill()
    }

    pub fn stdin(&mut self) -> &mut Option<tokio::process::ChildStdin> {
        &mut self.inner_mut().stdin
    }

    pub fn stdout(&mut self) -> &mut Option<tokio::process::ChildStdout> {
        &mut self.inner_mut().stdout
    }

    pub fn stderr(&mut self) -> &mut Option<tokio::process::ChildStderr> {
        &mut self.inner_mut().stderr
    }

    pub fn take_stdin(&mut self) -> Option<tokio::process::ChildStdin> {
        self.inner_mut().stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<tokio::process::ChildStdout> {
        self.inner_mut().stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<tokio::process::ChildStderr> {
        self.inner_mut().stderr.take()
    }

    fn update_node_on_exit(&self, result: &io::Result<ExitStatus>) {
        let elapsed_ns = self.start.elapsed().as_nanos() as u64;

        let (exit_code, exit_signal, error) = match result {
            Ok(status) => (status.code(), exit_signal_str(status), None),
            Err(e) => (None, None, Some(e.to_string())),
        };

        let attrs = build_attrs_json(
            &self.program,
            &self.args_preview,
            self.cwd.as_deref(),
            self.env_count,
            self.inner().id(),
            exit_code,
            exit_signal.as_deref(),
            Some(elapsed_ns),
            error.as_deref(),
        );
        crate::registry::register_node(Node {
            id: self.node_id.clone(),
            kind: NodeKind::Command,
            label: Some(self.program.clone()),
            attrs_json: attrs,
        });
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        crate::registry::remove_node(&self.node_id);
    }
}

// ── Helpers ──────────────────────────────────────────────

#[cfg(unix)]
fn exit_signal_str(status: &ExitStatus) -> Option<String> {
    use std::os::unix::process::ExitStatusExt;
    status.signal().map(|s| s.to_string())
}

#[cfg(not(unix))]
fn exit_signal_str(_status: &ExitStatus) -> Option<String> {
    None
}
