// r[impl api.command]
//! Instrumented process spawning, mirroring [`tokio::process`].
//!
//! This module mirrors the structure of `tokio::process` and can be used as a
//! drop-in replacement. Every spawned child process is registered as a named
//! entity in the Moiré runtime graph so the dashboard can show which tasks are
//! waiting on which subprocesses.
//!
//! # Example
//!
//! ```rust,no_run
//! use moire::process::Command;
//!
//! let status = Command::new("git")
//!     .args(["fetch", "--all"])
//!     .status()
//!     .await?;
//! ```
use moire_types::CommandEntity;
use std::ffi::{OsStr, OsString};
use std::future::Future;
use std::io;
use std::process::{ExitStatus, Output, Stdio};

use moire_runtime::{EntityHandle, instrument_future};

/// Instrumented version of [`tokio::process::Command`], used to collect task and process diagnostics.
pub struct Command {
    inner: tokio::process::Command,
    program: String,
    args: Vec<String>,
    env: Vec<String>,
}

#[derive(Clone, Debug)]
/// Snapshot of command construction state used for diagnostics.
pub struct CommandDiagnostics {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<String>,
}

/// Instrumented equivalent of [`tokio::process::Child`] for diagnostics metadata.
pub struct Child {
    inner: Option<tokio::process::Child>,
    handle: EntityHandle<CommandEntity>,
}

impl Command {
    /// Creates a new instrumented command builder, mirroring [`tokio::process::Command::new`].
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        let program = String::from(program.as_ref().to_string_lossy().as_ref());
        Self {
            inner: tokio::process::Command::new(program.as_str()),
            program,
            args: Vec::new(),
            env: Vec::new(),
        }
    }
    /// Adds one argument, matching [`tokio::process::Command::arg`].
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        let arg = arg.as_ref().to_owned();
        self.args.push(String::from(arg.to_string_lossy().as_ref()));
        self.inner.arg(&arg);
        self
    }
    /// Adds multiple arguments, matching [`tokio::process::Command::args`].
    pub fn args(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        let args: Vec<OsString> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        for arg in &args {
            self.args.push(String::from(arg.to_string_lossy().as_ref()));
        }
        self.inner.args(args);
        self
    }
    /// Sets one environment variable, matching [`tokio::process::Command::env`].
    pub fn env(&mut self, key: impl AsRef<OsStr>, val: impl AsRef<OsStr>) -> &mut Self {
        let key = key.as_ref().to_owned();
        let val = val.as_ref().to_owned();
        self.env.push(String::from(format!(
            "{}={}",
            key.to_string_lossy(),
            val.to_string_lossy()
        )));
        self.inner.env(&key, &val);
        self
    }
    /// Sets multiple environment variables, matching [`tokio::process::Command::envs`].
    pub fn envs(
        &mut self,
        vars: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    ) -> &mut Self {
        let vars: Vec<(OsString, OsString)> = vars
            .into_iter()
            .map(|(k, v)| (k.as_ref().to_owned(), v.as_ref().to_owned()))
            .collect();
        for (k, v) in &vars {
            self.env.push(String::from(format!(
                "{}={}",
                k.to_string_lossy(),
                v.to_string_lossy()
            )));
        }
        self.inner.envs(vars);
        self
    }
    /// Clears inherited environment variables, matching [`tokio::process::Command::env_clear`].
    pub fn env_clear(&mut self) -> &mut Self {
        self.env.clear();
        self.inner.env_clear();
        self
    }
    /// Removes an environment variable, matching [`tokio::process::Command::env_remove`].
    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        let key = key.as_ref().to_owned();
        let key_prefix = format!("{}=", key.to_string_lossy());
        self.env
            .retain(|entry| !entry.as_str().starts_with(&key_prefix));
        self.inner.env_remove(&key);
        self
    }
    /// Sets the current working directory, matching [`tokio::process::Command::current_dir`].
    pub fn current_dir(&mut self, dir: impl AsRef<std::path::Path>) -> &mut Self {
        self.inner.current_dir(dir);
        self
    }
    /// Configures stdin, matching [`tokio::process::Command::stdin`].
    pub fn stdin(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stdin(cfg);
        self
    }
    /// Configures stdout, matching [`tokio::process::Command::stdout`].
    pub fn stdout(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stdout(cfg);
        self
    }
    /// Configures stderr, matching [`tokio::process::Command::stderr`].
    pub fn stderr(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stderr(cfg);
        self
    }
    /// Toggles whether to kill child processes on drop, matching [`tokio::process::Command::kill_on_drop`].
    pub fn kill_on_drop(&mut self, kill_on_drop: bool) -> &mut Self {
        self.inner.kill_on_drop(kill_on_drop);
        self
    }
    /// Spawns the configured process, equivalent to [`tokio::process::Command::spawn`].
    pub fn spawn(&mut self) -> io::Result<Child> {
        let child = self.inner.spawn()?;
        let handle = EntityHandle::new(self.entity_name(), self.entity_body());
        Ok(Child {
            inner: Some(child),
            handle,
        })
    }
    /// Gets process status asynchronously, matching [`tokio::process::Command::status`].
    pub fn status(&mut self) -> impl Future<Output = io::Result<ExitStatus>> + '_ {
        let handle = EntityHandle::new(self.entity_name(), self.entity_body());
        instrument_future(
            "command.status",
            self.inner.status(),
            Some(handle.entity_ref()),
            None,
        )
    }
    /// Captures process output asynchronously, matching [`tokio::process::Command::output`].
    pub fn output(&mut self) -> impl Future<Output = io::Result<Output>> + '_ {
        let handle = EntityHandle::new(self.entity_name(), self.entity_body());
        instrument_future(
            "command.output",
            self.inner.output(),
            Some(handle.entity_ref()),
            None,
        )
    }
    /// Returns the inner `std::process::Command` reference.
    pub fn as_std(&self) -> &std::process::Command {
        self.inner.as_std()
    }

    #[cfg(unix)]
    /// Sets a pre-exec hook, matching [`tokio::process::Command::pre_exec`].
    pub unsafe fn pre_exec<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut() -> io::Result<()> + Send + Sync + 'static,
    {
        // SAFETY: caller guarantees the pre-exec closure is safe to run
        unsafe { self.inner.pre_exec(f) };
        self
    }
    /// Extracts the inner Tokio command.
    pub fn into_inner(self) -> tokio::process::Command {
        self.inner
    }
    /// Extracts the inner Tokio command and collected diagnostics metadata.
    pub fn into_inner_with_diagnostics(self) -> (tokio::process::Command, CommandDiagnostics) {
        let diag = CommandDiagnostics {
            program: self.program.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
        };
        (self.inner, diag)
    }

    fn entity_name(&self) -> String {
        String::from(format!("command.{}", self.program))
    }

    fn entity_body(&self) -> CommandEntity {
        CommandEntity {
            program: self.program.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
        }
    }
}

impl Child {
    #[doc(hidden)]
    pub fn from_tokio_with_diagnostics(
        child: tokio::process::Child,
        diag: CommandDiagnostics,
    ) -> Self {
        let body = CommandEntity {
            program: diag.program.clone(),
            args: diag.args.clone(),
            env: diag.env.clone(),
        };
        let name = String::from(format!("command.{}", diag.program));
        let handle = EntityHandle::new(name, body);
        Self {
            inner: Some(child),
            handle,
        }
    }

    /// Returns the inner Tokio child handle.
    fn inner(&self) -> &tokio::process::Child {
        self.inner.as_ref().expect("child already consumed")
    }

    /// Returns a mutable Tokio child handle.
    fn inner_mut(&mut self) -> &mut tokio::process::Child {
        self.inner.as_mut().expect("child already consumed")
    }
    /// Returns the OS process ID.
    pub fn id(&self) -> Option<u32> {
        self.inner().id()
    }
    /// Waits for the process to exit, matching [`tokio::process::Child::wait`].
    pub fn wait(&mut self) -> impl Future<Output = io::Result<ExitStatus>> + '_ {
        let handle = self.handle.clone();
        let wait_fut = self.inner_mut().wait();
        instrument_future("command.wait", wait_fut, Some(handle.entity_ref()), None)
    }
    /// Waits for output from the process, matching [`tokio::process::Child::wait_with_output`].
    pub fn wait_with_output(mut self) -> impl Future<Output = io::Result<Output>> {
        let child = self.inner.take().expect("child already consumed");
        instrument_future(
            "command.wait_with_output",
            child.wait_with_output(),
            Some(self.handle.entity_ref()),
            None,
        )
    }
    /// Requests immediate process termination, matching [`tokio::process::Child::start_kill`].
    pub fn start_kill(&mut self) -> io::Result<()> {
        self.inner_mut().start_kill()
    }
    /// Kills the child process (alias of `start_kill`).
    pub fn kill(&mut self) -> io::Result<()> {
        self.start_kill()
    }
    /// Returns mutable stdin handle, matching [`tokio::process::Child::stdin`].
    pub fn stdin(&mut self) -> &mut Option<tokio::process::ChildStdin> {
        &mut self.inner_mut().stdin
    }
    /// Returns mutable stdout handle, matching [`tokio::process::Child::stdout`].
    pub fn stdout(&mut self) -> &mut Option<tokio::process::ChildStdout> {
        &mut self.inner_mut().stdout
    }
    /// Returns mutable stderr handle, matching [`tokio::process::Child::stderr`].
    pub fn stderr(&mut self) -> &mut Option<tokio::process::ChildStderr> {
        &mut self.inner_mut().stderr
    }
    /// Takes the child's stdin handle.
    pub fn take_stdin(&mut self) -> Option<tokio::process::ChildStdin> {
        self.inner_mut().stdin.take()
    }
    /// Takes the child's stdout handle.
    pub fn take_stdout(&mut self) -> Option<tokio::process::ChildStdout> {
        self.inner_mut().stdout.take()
    }
    /// Takes the child's stderr handle.
    pub fn take_stderr(&mut self) -> Option<tokio::process::ChildStderr> {
        self.inner_mut().stderr.take()
    }
}
