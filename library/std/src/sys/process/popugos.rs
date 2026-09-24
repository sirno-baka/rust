use super::CommandEnvs;
use super::env::{CommandEnv, CommandResolvedEnvs};
use crate::ffi::{OsStr, OsString};
pub use crate::ffi::OsString as EnvKey;
use crate::num::NonZeroI32;
use crate::path::{Path, PathBuf};
use crate::process::StdioPipes;
use crate::sys::fs::File;
use crate::sys::pal::abi;
use crate::sys::pipe::{self, Pipe};
use crate::sys::{AsInner, cvt};
use crate::{fmt, io};

const SIGKILL: i32 = 9;
const O_RDWR: u32 = 2;

#[derive(Debug)]
pub enum Stdio {
    Inherit,
    Null,
    MakePipe,
    ParentStdout,
    ParentStderr,
    File(File),
    Pipe(Pipe),
}

pub struct Command {
    program: OsString,
    args: Vec<OsString>,
    env: CommandEnv,
    cwd: Option<OsString>,
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
}

impl Command {
    pub fn new(program: &OsStr) -> Command {
        Command {
            program: program.to_owned(),
            args: Vec::new(),
            env: CommandEnv::default(),
            cwd: None,
            stdin: None,
            stdout: None,
            stderr: None,
        }
    }

    pub fn arg(&mut self, arg: &OsStr) {
        self.args.push(arg.to_owned());
    }

    pub fn env_mut(&mut self) -> &mut CommandEnv {
        &mut self.env
    }

    pub fn cwd(&mut self, dir: &OsStr) {
        self.cwd = Some(dir.to_owned());
    }

    pub fn stdin(&mut self, stdin: Stdio) {
        self.stdin = Some(stdin);
    }

    pub fn stdout(&mut self, stdout: Stdio) {
        self.stdout = Some(stdout);
    }

    pub fn stderr(&mut self, stderr: Stdio) {
        self.stderr = Some(stderr);
    }

    pub fn get_program(&self) -> &OsStr {
        &self.program
    }

    pub fn get_args(&self) -> CommandArgs<'_> {
        CommandArgs { iter: self.args.iter() }
    }

    pub fn get_envs(&self) -> CommandEnvs<'_> {
        self.env.iter()
    }

    pub fn get_env_clear(&self) -> bool {
        self.env.does_clear()
    }

    pub fn get_resolved_envs(&self) -> CommandResolvedEnvs {
        CommandResolvedEnvs::new(self.env.capture())
    }

    pub fn get_current_dir(&self) -> Option<&Path> {
        self.cwd.as_ref().map(Path::new)
    }

    pub fn spawn(
        &mut self,
        default: Stdio,
        needs_stdin: bool,
    ) -> io::Result<(Process, StdioPipes)> {
        let null_stdin = Stdio::Null;
        let stdin_spec = self
            .stdin
            .as_ref()
            .unwrap_or(if needs_stdin { &default } else { &null_stdin });
        let stdout_spec = self.stdout.as_ref().unwrap_or(&default);
        let stderr_spec = self.stderr.as_ref().unwrap_or(&default);

        let mut stdin = PreparedStdio::new(stdin_spec, true)?;
        let mut stdout = PreparedStdio::new(stdout_spec, false)?;
        let mut stderr = PreparedStdio::new(stderr_spec, false)?;

        let sources = [stdin.child_fd, stdout.child_fd, stderr.child_fd];
        let child_fds = core::array::from_fn(|target| sources[target].unwrap_or(target as i32));

        let old_cwd = if let Some(cwd) = self.cwd.as_ref() {
            let old = match crate::sys::paths::getcwd() {
                Ok(path) => path,
                Err(error) => return Err(error),
            };
            if let Err(error) = crate::sys::paths::chdir(Path::new(cwd)) {
                return Err(error);
            }
            Some(old)
        } else {
            None
        };

        let spawn_result = self.spawn_inner(child_fds);

        if let Some(old) = old_cwd.as_ref() {
            let _ = crate::sys::paths::chdir(old);
        }
        let pid = spawn_result?;

        // The child inherited the prepared descriptors during spawn. The child
        // ends can now be closed in the parent; parent ends are returned to std.
        drop(stdin.child_hold.take());
        drop(stdout.child_hold.take());
        drop(stderr.child_hold.take());

        Ok((
            Process { pid, status: None },
            StdioPipes {
                stdin: stdin.parent.take(),
                stdout: stdout.parent.take(),
                stderr: stderr.parent.take(),
            },
        ))
    }

    fn spawn_inner(&self, child_fds: [i32; 3]) -> io::Result<i32> {
        let environment = self.env.capture();
        let resolved_program = resolve_program(&self.program, &environment);
        let executable_path = make_c_string(resolved_program.as_os_str())?;

        let mut arg_storage = Vec::<Vec<u8>>::with_capacity(self.args.len() + 1);
        // argv[0] remains exactly what the caller passed to Command::new; only
        // the executable path itself is resolved through PATH.
        arg_storage.push(make_c_string(&self.program)?);
        for arg in &self.args {
            arg_storage.push(make_c_string(arg)?);
        }
        let mut argv = Vec::<*const u8>::with_capacity(arg_storage.len() + 1);
        for arg in &arg_storage {
            argv.push(arg.as_ptr());
        }
        argv.push(core::ptr::null());

        let mut env_storage = Vec::<Vec<u8>>::with_capacity(environment.len());
        for (key, value) in environment {
            let key = os_bytes(&key);
            let value = os_bytes(&value);
            if key.contains(&0) || key.contains(&b'=') || value.contains(&0) {
                return Err(io::Error::from_raw_os_error(22));
            }
            let mut entry = Vec::with_capacity(key.len() + value.len() + 2);
            entry.extend_from_slice(key);
            entry.push(b'=');
            entry.extend_from_slice(value);
            entry.push(0);
            env_storage.push(entry);
        }
        let mut envp = Vec::<*const u8>::with_capacity(env_storage.len() + 1);
        for entry in &env_storage {
            envp.push(entry.as_ptr());
        }
        envp.push(core::ptr::null());

        let params = abi::ExecParams {
            stdin: child_fds[0],
            stdout: child_fds[1],
            stderr: child_fds[2],
            argc: arg_storage.len() as u32,
            argv: argv.as_ptr(),
            envc: env_storage.len() as u32,
            envp: envp.as_ptr(),
            pgid: -1,
            foreground: 0,
        };
        cvt(unsafe { abi::spawn_path(executable_path.as_ptr(), &params) })
    }
}

struct PreparedStdio {
    child_fd: Option<i32>,
    child_hold: Option<Pipe>,
    parent: Option<Pipe>,
}

impl PreparedStdio {
    fn new(spec: &Stdio, child_reads: bool) -> io::Result<Self> {
        match spec {
            Stdio::Inherit => Ok(Self { child_fd: None, child_hold: None, parent: None }),
            Stdio::ParentStdout => Ok(Self { child_fd: Some(1), child_hold: None, parent: None }),
            Stdio::ParentStderr => Ok(Self { child_fd: Some(2), child_hold: None, parent: None }),
            Stdio::File(file) => Ok(Self {
                child_fd: Some(file.as_raw_fd()),
                child_hold: None,
                parent: None,
            }),
            Stdio::Pipe(pipe) => Ok(Self {
                child_fd: Some(pipe.as_raw_fd()),
                child_hold: None,
                parent: None,
            }),
            Stdio::Null if child_reads => {
                // An empty pipe is a portable /dev/null equivalent for stdin:
                // with the write end closed before spawn, every read returns EOF.
                let (read_end, write_end) = pipe::pipe()?;
                drop(write_end);
                let fd = read_end.as_raw_fd();
                Ok(Self { child_fd: Some(fd), child_hold: Some(read_end), parent: None })
            }
            Stdio::Null => {
                // For stdout/stderr we need a real sink.
                let path = b"/dev/null\0";
                let fd = cvt(unsafe { abi::open(path.as_ptr(), O_RDWR) })?;
                let hold = unsafe { Pipe::from_raw_fd(fd) };
                Ok(Self { child_fd: Some(fd), child_hold: Some(hold), parent: None })
            }
            Stdio::MakePipe => {
                let (read_end, write_end) = pipe::pipe()?;
                if child_reads {
                    let child_fd = read_end.as_raw_fd();
                    Ok(Self {
                        child_fd: Some(child_fd),
                        child_hold: Some(read_end),
                        parent: Some(write_end),
                    })
                } else {
                    let child_fd = write_end.as_raw_fd();
                    Ok(Self {
                        child_fd: Some(child_fd),
                        child_hold: Some(write_end),
                        parent: Some(read_end),
                    })
                }
            }
        }
    }
}

fn os_bytes(value: &OsStr) -> &[u8] {
    &value.as_inner().inner
}

fn resolve_program(
    program: &OsStr,
    environment: &crate::collections::BTreeMap<OsString, OsString>,
) -> PathBuf {
    if os_bytes(program).contains(&b'/') {
        return PathBuf::from(program);
    }

    let path = environment
        .iter()
        .find(|(key, _)| key.as_os_str() == "PATH")
        .map(|(_, value)| value.as_os_str())
        .unwrap_or_else(|| OsStr::new("/:/bin:/usr/bin"));

    for directory in crate::env::split_paths(path) {
        let directory = if directory.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            directory
        };
        let candidate = directory.join(program);
        if candidate.is_file() {
            return candidate;
        }
    }

    PathBuf::from(program)
}

fn make_c_string(value: &OsStr) -> io::Result<Vec<u8>> {
    let bytes = os_bytes(value);
    if bytes.contains(&0) {
        return Err(io::Error::from_raw_os_error(22));
    }
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    Ok(out)
}

pub fn output(cmd: &mut Command) -> io::Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let (mut process, mut pipes) = cmd.spawn(Stdio::MakePipe, false)?;
    drop(pipes.stdin.take());

    if let Some(pipe) = pipes.stdout.as_ref() {
        pipe.set_nonblocking(true)?;
    }
    if let Some(pipe) = pipes.stderr.as_ref() {
        pipe.set_nonblocking(true)?;
    }

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdout_open = pipes.stdout.is_some();
    let mut stderr_open = pipes.stderr.is_some();
    let mut status = None;
    let mut buffer = [0u8; 1024];

    while status.is_none() || stdout_open || stderr_open {
        let mut progress = false;

        if stdout_open {
            match pipes.stdout.as_ref().unwrap().read(&mut buffer) {
                Ok(0) => stdout_open = false,
                Ok(n) => {
                    stdout.extend_from_slice(&buffer[..n]);
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }

        if stderr_open {
            match pipes.stderr.as_ref().unwrap().read(&mut buffer) {
                Ok(0) => stderr_open = false,
                Ok(n) => {
                    stderr.extend_from_slice(&buffer[..n]);
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }

        if status.is_none() {
            status = process.try_wait()?;
        }

        if !progress && (status.is_none() || stdout_open || stderr_open) {
            crate::thread::sleep(crate::time::Duration::from_millis(1));
        }
    }

    Ok((status.unwrap_or_else(|| ExitStatus(0)), stdout, stderr))
}

impl From<ChildPipe> for Stdio {
    fn from(pipe: ChildPipe) -> Stdio {
        Stdio::Pipe(pipe)
    }
}

impl From<File> for Stdio {
    fn from(file: File) -> Stdio {
        Stdio::File(file)
    }
}

impl From<io::Stdout> for Stdio {
    fn from(_: io::Stdout) -> Stdio {
        Stdio::ParentStdout
    }
}

impl From<io::Stderr> for Stdio {
    fn from(_: io::Stderr) -> Stdio {
        Stdio::ParentStderr
    }
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.program)?;
        for arg in &self.args {
            write!(f, " {:?}", arg)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ExitStatus(i32);

impl ExitStatus {
    pub fn exit_ok(&self) -> Result<(), ExitStatusError> {
        if self.code() == Some(0) { Ok(()) } else { Err(ExitStatusError(*self)) }
    }

    pub fn code(&self) -> Option<i32> {
        if self.0 & 0x7f == 0 { Some((self.0 >> 8) & 0xff) } else { None }
    }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code() {
            Some(code) => write!(f, "exit code: {code}"),
            None => write!(f, "process terminated by signal {}", self.0 & 0x7f),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ExitStatusError(ExitStatus);

impl Into<ExitStatus> for ExitStatusError {
    fn into(self) -> ExitStatus {
        self.0
    }
}

impl ExitStatusError {
    pub fn code(self) -> Option<NonZeroI32> {
        self.0.code().and_then(NonZeroI32::new)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ExitCode(i32);

impl ExitCode {
    pub const SUCCESS: ExitCode = ExitCode(0);
    pub const FAILURE: ExitCode = ExitCode(1);

    pub fn as_i32(&self) -> i32 {
        self.0
    }
}

impl From<u8> for ExitCode {
    fn from(code: u8) -> Self {
        ExitCode(code as i32)
    }
}

pub struct Process {
    pid: i32,
    status: Option<ExitStatus>,
}

impl Process {
    pub fn id(&self) -> u32 {
        self.pid as u32
    }

    pub fn kill(&mut self) -> io::Result<()> {
        if self.try_wait()?.is_some() {
            return Ok(());
        }
        cvt(unsafe { abi::kill(self.pid, SIGKILL) }).map(drop)
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        let mut raw = 0i32;
        cvt(unsafe { abi::waitpid(self.pid, &mut raw, 0) })?;
        let status = ExitStatus(raw);
        self.status = Some(status);
        Ok(status)
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        let mut raw = 0i32;
        let result = cvt(unsafe { abi::waitpid(self.pid, &mut raw, abi::WNOHANG) })?;
        if result == 0 {
            Ok(None)
        } else {
            let status = ExitStatus(raw);
            self.status = Some(status);
            Ok(Some(status))
        }
    }
}

pub struct CommandArgs<'a> {
    iter: crate::slice::Iter<'a, OsString>,
}

impl<'a> Iterator for CommandArgs<'a> {
    type Item = &'a OsStr;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|value| &**value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl ExactSizeIterator for CommandArgs<'_> {
    fn len(&self) -> usize {
        self.iter.len()
    }

    fn is_empty(&self) -> bool {
        self.iter.is_empty()
    }
}

impl fmt::Debug for CommandArgs<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter.clone()).finish()
    }
}

pub type ChildPipe = Pipe;

pub fn read_output(
    out: ChildPipe,
    stdout: &mut Vec<u8>,
    err: ChildPipe,
    stderr: &mut Vec<u8>,
) -> io::Result<()> {
    out.set_nonblocking(true)?;
    err.set_nonblocking(true)?;
    let mut out_open = true;
    let mut err_open = true;
    let mut buffer = [0u8; 1024];

    while out_open || err_open {
        let mut progress = false;
        if out_open {
            match out.read(&mut buffer) {
                Ok(0) => out_open = false,
                Ok(n) => {
                    stdout.extend_from_slice(&buffer[..n]);
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }
        if err_open {
            match err.read(&mut buffer) {
                Ok(0) => err_open = false,
                Ok(n) => {
                    stderr.extend_from_slice(&buffer[..n]);
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }
        if !progress && (out_open || err_open) {
            crate::thread::sleep(crate::time::Duration::from_millis(1));
        }
    }
    Ok(())
}

pub fn getpid() -> u32 {
    let pid = unsafe { abi::getpid() };
    if pid < 0 { 0 } else { pid as u32 }
}
