use peeps_types::{CommandEntity, EntityBody};
use std::ffi::{OsStr, OsString};
use std::future::Future;
use std::io;
use std::process::{ExitStatus, Output, Stdio};

use super::{local_source, Source, SourceRight};
use peeps_runtime::{instrument_future, EntityHandle};

pub struct Command {
    inner: tokio::process::Command,
    program: String,
    args: Vec<String>,
    env: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CommandDiagnostics {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<String>,
}

pub struct Child {
    inner: Option<tokio::process::Child>,
    handle: EntityHandle,
}

pub struct JoinSet<T> {
    pub(super) inner: tokio::task::JoinSet<T>,
    pub(super) handle: EntityHandle,
}

pub struct DiagnosticInterval {
    _inner: tokio::time::Interval,
    _handle: EntityHandle,
}

pub type Interval = DiagnosticInterval;

impl Command {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        let program = String::from(program.as_ref().to_string_lossy().as_ref());
        Self {
            inner: tokio::process::Command::new(program.as_str()),
            program,
            args: Vec::new(),
            env: Vec::new(),
        }
    }
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        let arg = arg.as_ref().to_owned();
        self.args.push(String::from(arg.to_string_lossy().as_ref()));
        self.inner.arg(&arg);
        self
    }
    pub fn args(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        let args: Vec<OsString> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        for arg in &args {
            self.args.push(String::from(arg.to_string_lossy().as_ref()));
        }
        self.inner.args(args);
        self
    }
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
    pub fn env_clear(&mut self) -> &mut Self {
        self.env.clear();
        self.inner.env_clear();
        self
    }
    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        let key = key.as_ref().to_owned();
        let key_prefix = format!("{}=", key.to_string_lossy());
        self.env
            .retain(|entry| !entry.as_str().starts_with(&key_prefix));
        self.inner.env_remove(&key);
        self
    }
    pub fn current_dir(&mut self, dir: impl AsRef<std::path::Path>) -> &mut Self {
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
        self.inner.kill_on_drop(kill_on_drop);
        self
    }

    #[doc(hidden)]
    pub fn spawn_with_source(&mut self, source: Source) -> io::Result<Child> {
        let child = self.inner.spawn()?;
        let handle = EntityHandle::new(self.entity_name(), self.entity_body(), source);
        Ok(Child {
            inner: Some(child),
            handle,
        })
    }

    #[doc(hidden)]
    pub fn status_with_source(
        &mut self,
        source: Source,
    ) -> impl Future<Output = io::Result<ExitStatus>> + '_ {
        let handle = EntityHandle::new(self.entity_name(), self.entity_body(), source.clone());
        instrument_future(
            "command.status",
            self.inner.status(),
            source,
            Some(handle.entity_ref()),
            None,
        )
    }

    #[doc(hidden)]
    pub fn output_with_source(
        &mut self,
        source: Source,
    ) -> impl Future<Output = io::Result<Output>> + '_ {
        let handle = EntityHandle::new(self.entity_name(), self.entity_body(), source.clone());
        instrument_future(
            "command.output",
            self.inner.output(),
            source,
            Some(handle.entity_ref()),
            None,
        )
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
            program: self.program.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
        };
        (self.inner, diag)
    }

    fn entity_name(&self) -> String {
        String::from(format!("command.{}", self.program))
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
    pub fn from_tokio_with_diagnostics(
        child: tokio::process::Child,
        diag: CommandDiagnostics,
    ) -> Self {
        let body = EntityBody::Command(CommandEntity {
            program: diag.program.clone(),
            args: diag.args.clone(),
            env: diag.env.clone(),
        });
        let name = String::from(format!("command.{}", diag.program));
        let handle = EntityHandle::new(
            name,
            body,
            local_source(SourceRight::caller()),
        );
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
    pub fn id(&self) -> Option<u32> {
        self.inner().id()
    }

    #[doc(hidden)]
    pub fn wait_with_source(
        &mut self,
        source: Source,
    ) -> impl Future<Output = io::Result<ExitStatus>> + '_ {
        let handle = self.handle.clone();
        let wait_fut = self.inner_mut().wait();
        instrument_future(
            "command.wait",
            wait_fut,
            source,
            Some(handle.entity_ref()),
            None,
        )
    }

    #[doc(hidden)]
    pub fn wait_with_output_with_source(
        mut self,
        source: Source,
    ) -> impl Future<Output = io::Result<Output>> {
        let child = self.inner.take().expect("child already consumed");
        instrument_future(
            "command.wait_with_output",
            child.wait_with_output(),
            source,
            Some(self.handle.entity_ref()),
            None,
        )
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
}
