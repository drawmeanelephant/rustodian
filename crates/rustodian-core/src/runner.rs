use std::collections::HashMap;
use std::path::PathBuf;

/// Structured command specification for the command runner.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
    pub env: HashMap<String, String>,
    pub use_shell: bool,
}

use std::process::{Child, Command, Stdio};
use std::os::unix::process::CommandExt;
use std::io::Read;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use crate::error::CoreError;
use crate::traits::{CommandRunner, RunningProcess};

pub struct DefaultCommandRunner;

impl CommandRunner for DefaultCommandRunner {
    fn spawn(&self, spec: CommandSpec) -> Result<Box<dyn RunningProcess>, CoreError> {
        let mut cmd = if spec.use_shell {
            let mut c = Command::new("sh");
            c.arg("-c").arg(&spec.program);
            c
        } else {
            // If the user specifies `use_shell=false`, but `spec.program` is actually a full command string,
            // we should parse it with shlex.
            let mut args_iter = shlex::split(&spec.program).unwrap_or_else(|| vec![spec.program.clone()]);

            let program = if args_iter.is_empty() {
                spec.program.clone()
            } else {
                args_iter.remove(0)
            };

            let mut c = Command::new(program);
            c.args(args_iter);
            c.args(&spec.args);
            c
        };

        cmd.current_dir(&spec.working_dir)
            .envs(&spec.env)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .process_group(0); // Create a new process group

        let child = cmd.spawn().map_err(|e| CoreError::Storage(format!("Failed to spawn process: {e}")))?;

        Ok(Box::new(DefaultRunningProcess { child }))
    }
}

pub struct DefaultRunningProcess {
    child: Child,
}

impl RunningProcess for DefaultRunningProcess {
    fn id(&self) -> u32 {
        self.child.id()
    }

    fn wait(&mut self) -> Result<(), CoreError> {
        self.child.wait().map_err(|e| CoreError::Storage(format!("Failed to wait for process: {e}")))?;
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<()>, CoreError> {
        match self.child.try_wait() {
            Ok(Some(_)) => Ok(Some(())),
            Ok(None) => Ok(None),
            Err(e) => Err(CoreError::Storage(format!("Failed to try_wait for process: {e}"))),
        }
    }

    fn kill(&mut self) -> Result<(), CoreError> {
        let pid = Pid::from_raw(self.child.id().cast_signed());
        // Kill the entire process group
        let _ = kill(Pid::from_raw(-pid.as_raw()), Signal::SIGKILL);

        // Reap the zombie
        let _ = self.child.wait();
        Ok(())
    }

    fn stdout(&mut self) -> Option<Box<dyn Read + Send + Sync>> {
        self.child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send + Sync>)
    }

    fn stderr(&mut self) -> Option<Box<dyn Read + Send + Sync>> {
        self.child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send + Sync>)
    }
}
