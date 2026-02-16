use std::ffi::OsStr;
use std::io;
use std::process::{ExitStatus, Output, Stdio};

/// Zero-cost wrapper around `tokio::process::Command` (diagnostics disabled).
pub struct Command(tokio::process::Command);

#[derive(Clone, Debug)]
pub struct CommandDiagnostics;

impl Command {
    #[inline]
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self(tokio::process::Command::new(program))
    }

    #[inline]
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.0.arg(arg);
        self
    }

    #[inline]
    pub fn args(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        self.0.args(args);
        self
    }

    #[inline]
    pub fn env(&mut self, key: impl AsRef<OsStr>, val: impl AsRef<OsStr>) -> &mut Self {
        self.0.env(key, val);
        self
    }

    #[inline]
    pub fn envs(
        &mut self,
        vars: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    ) -> &mut Self {
        self.0.envs(vars);
        self
    }

    #[inline]
    pub fn env_clear(&mut self) -> &mut Self {
        self.0.env_clear();
        self
    }

    #[inline]
    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        self.0.env_remove(key);
        self
    }

    #[inline]
    pub fn current_dir(&mut self, dir: impl AsRef<std::path::Path>) -> &mut Self {
        self.0.current_dir(dir);
        self
    }

    #[inline]
    pub fn stdin(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.0.stdin(cfg);
        self
    }

    #[inline]
    pub fn stdout(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.0.stdout(cfg);
        self
    }

    #[inline]
    pub fn stderr(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.0.stderr(cfg);
        self
    }

    #[inline]
    pub fn kill_on_drop(&mut self, kill_on_drop: bool) -> &mut Self {
        self.0.kill_on_drop(kill_on_drop);
        self
    }

    #[inline]
    pub fn spawn(&mut self) -> io::Result<Child> {
        self.0.spawn().map(Child)
    }

    #[inline]
    pub async fn status(&mut self) -> io::Result<ExitStatus> {
        self.0.status().await
    }

    #[inline]
    pub async fn output(&mut self) -> io::Result<Output> {
        self.0.output().await
    }

    #[inline]
    pub fn as_std(&self) -> &std::process::Command {
        self.0.as_std()
    }

    #[cfg(unix)]
    #[inline]
    pub unsafe fn pre_exec<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut() -> io::Result<()> + Send + Sync + 'static,
    {
        self.0.pre_exec(f);
        self
    }

    #[inline]
    pub fn into_inner(self) -> tokio::process::Command {
        self.0
    }

    #[inline]
    pub fn into_inner_with_diagnostics(self) -> (tokio::process::Command, CommandDiagnostics) {
        (self.0, CommandDiagnostics)
    }
}

/// Zero-cost wrapper around `tokio::process::Child` (diagnostics disabled).
pub struct Child(tokio::process::Child);

impl Child {
    #[inline]
    pub fn from_tokio_with_diagnostics(
        child: tokio::process::Child,
        _diag: CommandDiagnostics,
    ) -> Self {
        Self(child)
    }

    #[inline]
    pub fn id(&self) -> Option<u32> {
        self.0.id()
    }

    #[inline]
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        self.0.wait().await
    }

    #[inline]
    pub async fn wait_with_output(self) -> io::Result<Output> {
        self.0.wait_with_output().await
    }

    #[inline]
    pub fn start_kill(&mut self) -> io::Result<()> {
        self.0.start_kill()
    }

    #[inline]
    pub fn kill(&mut self) -> io::Result<()> {
        self.start_kill()
    }

    #[inline]
    pub fn stdin(&mut self) -> &mut Option<tokio::process::ChildStdin> {
        &mut self.0.stdin
    }

    #[inline]
    pub fn stdout(&mut self) -> &mut Option<tokio::process::ChildStdout> {
        &mut self.0.stdout
    }

    #[inline]
    pub fn stderr(&mut self) -> &mut Option<tokio::process::ChildStderr> {
        &mut self.0.stderr
    }

    #[inline]
    pub fn take_stdin(&mut self) -> Option<tokio::process::ChildStdin> {
        self.0.stdin.take()
    }

    #[inline]
    pub fn take_stdout(&mut self) -> Option<tokio::process::ChildStdout> {
        self.0.stdout.take()
    }

    #[inline]
    pub fn take_stderr(&mut self) -> Option<tokio::process::ChildStderr> {
        self.0.stderr.take()
    }
}
