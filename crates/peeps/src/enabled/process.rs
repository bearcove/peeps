use compact_str::CompactString;
use peeps_types::{CommandEntity, EntityBody};
use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
use std::future::Future;
use std::io;
use std::process::{ExitStatus, Output, Stdio};
use std::time::Duration;

use super::futures::{instrument_future_on, instrument_future_on_with_source};
use super::handles::EntityHandle;
use super::{register_current_task_scope, CrateContext, UnqualSource, FUTURE_CAUSAL_STACK};

pub struct Command {
    inner: tokio::process::Command,
    program: CompactString,
    args: Vec<CompactString>,
    env: Vec<CompactString>,
}

#[derive(Clone, Debug)]
pub struct CommandDiagnostics {
    pub program: CompactString,
    pub args: Vec<CompactString>,
    pub env: Vec<CompactString>,
}

pub struct Child {
    inner: Option<tokio::process::Child>,
    handle: EntityHandle,
}

pub struct JoinSet<T> {
    inner: tokio::task::JoinSet<T>,
    handle: EntityHandle,
}

pub struct DiagnosticInterval {
    inner: tokio::time::Interval,
    handle: EntityHandle,
}

pub type Interval = DiagnosticInterval;

impl Command {
    #[track_caller]
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        let program = CompactString::from(program.as_ref().to_string_lossy().as_ref());
        Self {
            inner: tokio::process::Command::new(program.as_str()),
            program,
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    #[track_caller]
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        let arg = arg.as_ref().to_owned();
        self.args
            .push(CompactString::from(arg.to_string_lossy().as_ref()));
        self.inner.arg(&arg);
        self
    }

    #[track_caller]
    pub fn args(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        let args: Vec<OsString> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        for arg in &args {
            self.args
                .push(CompactString::from(arg.to_string_lossy().as_ref()));
        }
        self.inner.args(args);
        self
    }

    #[track_caller]
    pub fn env(&mut self, key: impl AsRef<OsStr>, val: impl AsRef<OsStr>) -> &mut Self {
        let key = key.as_ref().to_owned();
        let val = val.as_ref().to_owned();
        self.env.push(CompactString::from(format!(
            "{}={}",
            key.to_string_lossy(),
            val.to_string_lossy()
        )));
        self.inner.env(&key, &val);
        self
    }

    #[track_caller]
    pub fn envs(
        &mut self,
        vars: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    ) -> &mut Self {
        let vars: Vec<(OsString, OsString)> = vars
            .into_iter()
            .map(|(k, v)| (k.as_ref().to_owned(), v.as_ref().to_owned()))
            .collect();
        for (k, v) in &vars {
            self.env.push(CompactString::from(format!(
                "{}={}",
                k.to_string_lossy(),
                v.to_string_lossy()
            )));
        }
        self.inner.envs(vars);
        self
    }

    #[track_caller]
    pub fn env_clear(&mut self) -> &mut Self {
        self.env.clear();
        self.inner.env_clear();
        self
    }

    #[track_caller]
    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        let key = key.as_ref().to_owned();
        let key_prefix = format!("{}=", key.to_string_lossy());
        self.env
            .retain(|entry| !entry.as_str().starts_with(&key_prefix));
        self.inner.env_remove(&key);
        self
    }

    #[track_caller]
    pub fn current_dir(&mut self, dir: impl AsRef<std::path::Path>) -> &mut Self {
        self.inner.current_dir(dir);
        self
    }

    #[track_caller]
    pub fn stdin(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stdin(cfg);
        self
    }

    #[track_caller]
    pub fn stdout(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stdout(cfg);
        self
    }

    #[track_caller]
    pub fn stderr(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stderr(cfg);
        self
    }

    #[track_caller]
    pub fn kill_on_drop(&mut self, kill_on_drop: bool) -> &mut Self {
        self.inner.kill_on_drop(kill_on_drop);
        self
    }

    #[track_caller]
    pub fn spawn_with_cx(&mut self, cx: CrateContext) -> io::Result<Child> {
        self.spawn_with_source(UnqualSource::caller(), cx)
    }

    pub fn spawn_with_source(
        &mut self,
        source: UnqualSource,
        _cx: CrateContext,
    ) -> io::Result<Child> {
        let child = self.inner.spawn()?;
        let handle = EntityHandle::new(self.entity_name(), self.entity_body(), source);
        Ok(Child {
            inner: Some(child),
            handle,
        })
    }

    #[track_caller]
    pub fn status_with_cx(
        &mut self,
        cx: CrateContext,
    ) -> impl Future<Output = io::Result<ExitStatus>> + '_ {
        self.status_with_source(UnqualSource::caller(), cx)
    }

    pub fn status_with_source(
        &mut self,
        source: UnqualSource,
        _cx: CrateContext,
    ) -> impl Future<Output = io::Result<ExitStatus>> + '_ {
        let handle = EntityHandle::new(self.entity_name(), self.entity_body(), source);
        instrument_future_on_with_source("command.status", &handle, self.inner.status(), source)
    }

    #[track_caller]
    pub fn output_with_cx(
        &mut self,
        cx: CrateContext,
    ) -> impl Future<Output = io::Result<Output>> + '_ {
        self.output_with_source(UnqualSource::caller(), cx)
    }

    pub fn output_with_source(
        &mut self,
        source: UnqualSource,
        _cx: CrateContext,
    ) -> impl Future<Output = io::Result<Output>> + '_ {
        let handle = EntityHandle::new(self.entity_name(), self.entity_body(), source);
        instrument_future_on_with_source("command.output", &handle, self.inner.output(), source)
    }

    #[track_caller]
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

    #[track_caller]
    pub fn into_inner(self) -> tokio::process::Command {
        self.inner
    }

    #[track_caller]
    pub fn into_inner_with_diagnostics(self) -> (tokio::process::Command, CommandDiagnostics) {
        let diag = CommandDiagnostics {
            program: self.program.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
        };
        (self.inner, diag)
    }

    fn entity_name(&self) -> CompactString {
        CompactString::from(format!("command.{}", self.program))
    }

    fn entity_body(&self) -> EntityBody {
        EntityBody::Command(CommandEntity {
            program: self.program.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
        })
    }
}

impl Child {
    #[track_caller]
    pub fn from_tokio_with_diagnostics(
        child: tokio::process::Child,
        diag: CommandDiagnostics,
    ) -> Self {
        let body = EntityBody::Command(CommandEntity {
            program: diag.program.clone(),
            args: diag.args.clone(),
            env: diag.env.clone(),
        });
        let name = CompactString::from(format!("command.{}", diag.program));
        let handle = EntityHandle::new(name, body, UnqualSource::caller());
        Self {
            inner: Some(child),
            handle,
        }
    }

    fn inner(&self) -> &tokio::process::Child {
        self.inner.as_ref().expect("child already consumed")
    }

    fn inner_mut(&mut self) -> &mut tokio::process::Child {
        self.inner.as_mut().expect("child already consumed")
    }

    #[track_caller]
    pub fn id(&self) -> Option<u32> {
        self.inner().id()
    }

    #[track_caller]
    pub fn wait_with_cx(
        &mut self,
        cx: CrateContext,
    ) -> impl Future<Output = io::Result<ExitStatus>> + '_ {
        self.wait_with_source(UnqualSource::caller(), cx)
    }

    pub fn wait_with_source(
        &mut self,
        source: UnqualSource,
        _cx: CrateContext,
    ) -> impl Future<Output = io::Result<ExitStatus>> + '_ {
        let handle = self.handle.clone();
        let wait_fut = self.inner_mut().wait();
        instrument_future_on_with_source("command.wait", &handle, wait_fut, source)
    }

    #[track_caller]
    pub fn wait_with_output_with_cx(
        self,
        cx: CrateContext,
    ) -> impl Future<Output = io::Result<Output>> {
        self.wait_with_output_with_source(UnqualSource::caller(), cx)
    }

    pub fn wait_with_output_with_source(
        mut self,
        source: UnqualSource,
        _cx: CrateContext,
    ) -> impl Future<Output = io::Result<Output>> {
        let child = self.inner.take().expect("child already consumed");
        instrument_future_on_with_source(
            "command.wait_with_output",
            &self.handle,
            child.wait_with_output(),
            source,
        )
    }

    #[track_caller]
    pub fn start_kill(&mut self) -> io::Result<()> {
        self.inner_mut().start_kill()
    }

    #[track_caller]
    pub fn kill(&mut self) -> io::Result<()> {
        self.start_kill()
    }

    #[track_caller]
    pub fn stdin(&mut self) -> &mut Option<tokio::process::ChildStdin> {
        &mut self.inner_mut().stdin
    }

    #[track_caller]
    pub fn stdout(&mut self) -> &mut Option<tokio::process::ChildStdout> {
        &mut self.inner_mut().stdout
    }

    #[track_caller]
    pub fn stderr(&mut self) -> &mut Option<tokio::process::ChildStderr> {
        &mut self.inner_mut().stderr
    }

    #[track_caller]
    pub fn take_stdin(&mut self) -> Option<tokio::process::ChildStdin> {
        self.inner_mut().stdin.take()
    }

    #[track_caller]
    pub fn take_stdout(&mut self) -> Option<tokio::process::ChildStdout> {
        self.inner_mut().stdout.take()
    }

    #[track_caller]
    pub fn take_stderr(&mut self) -> Option<tokio::process::ChildStderr> {
        self.inner_mut().stderr.take()
    }
}
