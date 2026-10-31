use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

#[path = "process_tree.rs"]
pub mod process_tree;

#[derive(Clone, Debug)]
pub struct ProcessRequest {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub stdin: Vec<u8>,
    pub env: BTreeMap<OsString, OsString>,
    pub env_remove: Vec<OsString>,
}

impl ProcessRequest {
    pub fn new(program: impl Into<OsString>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            timeout: Duration::from_secs(30),
            stdin: Vec::new(),
            env: BTreeMap::new(),
            env_remove: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessOutput {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
}

pub trait ProcessRunner {
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput>;
    fn factory_transaction(
        &self,
        _root: &std::path::Path,
        _writes: BTreeMap<PathBuf, Vec<u8>>,
        _release_inputs: BTreeMap<PathBuf, Option<Vec<u8>>>,
        _executor: &dyn ProcessRunner,
    ) -> Result<crate::factory_bridge::FactoryTransaction, String> {
        Err("factory transaction adapter is unavailable; use the factory entry point".into())
    }
    /// A verified cache load, after byte, receipt, self-test and environment checks.
    fn cache_hit(&self) {}
    fn cache_miss(&self, _reason: &'static str) {}
    fn build_group_finished(&self, _group: &'static str, _elapsed_ms: u128) {}
    /// Runners must explicitly support compiler probes and executing cached bytes.
    fn artifact_cache_enabled(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProcessRunner;

impl ProcessRunner for SystemProcessRunner {
    fn artifact_cache_enabled(&self) -> bool {
        true
    }
    fn run(&self, request: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        let program = std::path::Path::new(&request.program)
            .file_stem()
            .and_then(|p| p.to_str())
            .unwrap_or("other");
        let verb = request.args.iter().filter_map(|v| v.to_str()).find(|v| {
            matches!(
                *v,
                "show"
                    | "ls-tree"
                    | "cat-file"
                    | "rev-list"
                    | "log"
                    | "diff"
                    | "ls-files"
                    | "status"
                    | "fetch"
                    | "pull"
                    | "push"
                    | "commit"
                    | "build"
            )
        });
        let _profile =
            crate::release_metrics::span(format!("process.{program}.{}", verb.unwrap_or("other")));
        // The deadline includes setup, stdin delivery and draining both output pipes.
        let started = Instant::now();
        let mut command = std::process::Command::new(&request.program);
        command
            .args(&request.args)
            .current_dir(&request.cwd)
            .stdin(if request.stdin.is_empty() {
                Stdio::null()
            } else {
                Stdio::piped()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for key in &request.env_remove {
            command.env_remove(key);
        }
        command.envs(&request.env);
        let mut tree = process_tree::ProcessTree::spawn(&mut command)?;
        let mut stdout = tree.child.stdout.take().expect("piped stdout");
        let stdout_reader = thread::spawn(move || {
            let mut payload = Vec::new();
            stdout.read_to_end(&mut payload).map(|_| payload)
        });
        let mut stderr = tree.child.stderr.take().expect("piped stderr");
        let stderr_reader = thread::spawn(move || {
            let mut payload = Vec::new();
            stderr.read_to_end(&mut payload).map(|_| payload)
        });
        let stdin_writer = tree.child.stdin.take().map(|mut stdin| {
            let payload = request.stdin.clone();
            thread::spawn(move || stdin.write_all(&payload))
        });
        let mut status = None;
        let mut timed_out = false;
        let mut cleanup_started = None;
        loop {
            if status.is_none() {
                status = tree.child.try_wait()?;
            }
            let io_done = stdout_reader.is_finished()
                && stderr_reader.is_finished()
                && stdin_writer
                    .as_ref()
                    .is_none_or(|writer| writer.is_finished());
            if status.is_some() && io_done {
                break;
            }
            if !timed_out && started.elapsed() >= request.timeout {
                timed_out = true;
                tree.terminate()?;
                cleanup_started = Some(Instant::now());
            }
            if cleanup_started.is_some_and(|start| start.elapsed() >= Duration::from_secs(2)) {
                // Never join a blocked pipe thread or perform an unbounded child.wait().
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "process tree or standard streams did not close after timeout",
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
        let stdout = stdout_reader
            .join()
            .map_err(|_| std::io::Error::other("stdout reader thread panicked"))??;
        let stderr = stderr_reader
            .join()
            .map_err(|_| std::io::Error::other("stderr reader thread panicked"))??;
        if let Some(writer) = stdin_writer {
            let result = writer
                .join()
                .map_err(|_| std::io::Error::other("stdin writer thread panicked"))?;
            // Broken stdin after terminating the tree must not hide the timeout.
            if !timed_out {
                result?;
            }
        }
        Ok(ProcessOutput {
            code: if timed_out {
                -1
            } else {
                status.and_then(|s| s.code()).unwrap_or(-1)
            },
            stdout,
            stderr,
            timed_out,
        })
    }
}
