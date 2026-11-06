#![allow(dead_code)] // Shared harness; each integration target uses a subset.
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStringExt, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
pub const ROOT: &str = env!("CARGO_MANIFEST_DIR");
pub const FIXTURE: &str = env!("CARGO_BIN_EXE_hatch-test-fixture");
pub fn binary() -> PathBuf {
    let p = std::env::var_os("HATCH_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_hatch").into());
    fs::canonicalize(p).unwrap()
}
pub fn strings(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}
pub fn read_json(p: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
pub fn rows(p: impl AsRef<Path>) -> Vec<Value> {
    fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
pub fn write_json(p: impl AsRef<Path>, value: &Value) {
    fs::write(p, value.to_string()).unwrap();
}
pub fn contains(text: &str, expected: &str) {
    assert!(text.contains(expected), "missing {expected:?} in {text}");
}
pub struct Harness {
    pub dir: tempfile::TempDir,
    pub env: BTreeMap<OsString, OsString>,
}
pub struct Run {
    pub pid: u32,
    pub out: String,
    pub err: String,
}
#[derive(Debug)]
pub struct Call {
    pub args: Vec<OsString>,
    pub pid: u32,
    pub agent: Value,
}
impl Call {
    pub fn has(&self, arg: &str) -> bool {
        self.args.iter().any(|a| a == arg)
    }
    pub fn launches(&self) -> bool {
        self.args.first().is_some_and(|a| a == "exec")
            && self
                .args
                .iter()
                .take_while(|a| a.len() != 64)
                .any(|a| a == "-i")
    }
}
impl Harness {
    pub fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("hatch test ")
            .tempdir()
            .unwrap();
        let mut h = Self {
            dir,
            env: BTreeMap::new(),
        };
        fs::create_dir(h.path("home")).unwrap();
        fs::copy(
            Path::new(ROOT).join("tests/fixtures/containers.json"),
            h.path("containers.json"),
        )
        .unwrap();
        for (k, v) in [
            ("PATH", h.path("")),
            ("HOME", h.path("home")),
            ("HATCH_TEST_LOG", h.path("calls.jsonl")),
            ("HATCH_TEST_FIXTURE", h.path("containers.json")),
        ] {
            h.set(k, v);
        }
        for (k, v) in [
            ("HATCH_PLAIN", "1"),
            ("HERDR_ENV", "0"),
            ("LANG", "C.UTF-8"),
        ] {
            h.set(k, v);
        }
        h.tool("docker");
        h
    }
    pub fn path(&self, p: &str) -> PathBuf {
        if p.is_empty() {
            self.dir.path().to_path_buf()
        } else {
            self.dir.path().join(p)
        }
    }
    pub fn set(&mut self, k: &str, v: impl AsRef<OsStr>) {
        self.env.insert(k.into(), v.as_ref().into());
    }
    pub fn tool(&self, name: &str) {
        std::os::unix::fs::symlink(FIXTURE, self.path(name)).unwrap();
    }
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut c = Command::new(program);
        c.env_clear().envs(&self.env).current_dir(self.dir.path());
        c
    }
    pub fn run(&self, args: &[&str], status: i32) -> Run {
        self.run_os(&strings(args), b"", status)
    }
    pub fn run_os(&self, args: &[OsString], input: &[u8], status: i32) -> Run {
        let mut c = self.command(binary());
        c.args(args);
        self.execute(&mut c, input, status)
    }
    pub fn execute(&self, c: &mut Command, input: &[u8], status: i32) -> Run {
        let mut out = tempfile::tempfile().unwrap();
        let mut err = tempfile::tempfile().unwrap();
        c.stdin(Stdio::piped())
            .stdout(out.try_clone().unwrap())
            .stderr(err.try_clone().unwrap());
        let mut child = Guard(c.spawn().unwrap());
        let pid = child.0.id();
        child.0.stdin.take().unwrap().write_all(input).unwrap();
        let code = wait(&mut child.0);
        use std::io::{Seek, SeekFrom};
        out.seek(SeekFrom::Start(0)).unwrap();
        err.seek(SeekFrom::Start(0)).unwrap();
        let mut stdout = String::new();
        let mut stderr = String::new();
        out.read_to_string(&mut stdout).unwrap();
        err.read_to_string(&mut stderr).unwrap();
        assert_eq!(
            code,
            Some(status),
            "{c:?}\nstdout: {stdout}\nstderr: {stderr}"
        );
        Run {
            pid,
            out: stdout,
            err: stderr,
        }
    }
    pub fn calls(&self) -> Vec<Call> {
        rows(self.path("calls.jsonl"))
            .into_iter()
            .map(|v| Call {
                args: v["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| {
                        let s = a.as_str().unwrap();
                        OsString::from_vec(
                            (0..s.len())
                                .step_by(2)
                                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                                .collect(),
                        )
                    })
                    .collect(),
                pid: v["pid"].as_u64().unwrap() as u32,
                agent: v["agent"].clone(),
            })
            .collect()
    }
    pub fn no_exec(&self) {
        assert!(self.calls().iter().all(|c| c.args[0] != "exec"));
    }
    pub fn no_launch(&self) {
        assert!(self.calls().iter().all(|c| !c.launches()));
    }
    pub fn no_install(&self) {
        assert!(self.calls().iter().all(|c| !c.has("hatch-install")));
    }
    pub fn resolver(&mut self) -> Vec<OsString> {
        self.tool("devcontainer");
        fs::write(
            self.path("devcontainer.json"),
            "{ /* resolver owns JSONC parsing */ }",
        )
        .unwrap();
        self.set("HATCH_TEST_RESOLVER_LOG", self.path("resolver.log"));
        vec![
            "--container".into(),
            "alpha-dev".into(),
            "--config".into(),
            self.path("devcontainer.json").into(),
            "--host-workspace".into(),
            self.path("").into(),
        ]
    }
    pub fn provision(&mut self, matching: bool) -> Vec<OsString> {
        let args = self.resolver()[2..].to_vec();
        if matching {
            self.edit_fixture(|v| {
                v[0]["Config"]["Labels"]["devcontainer.config_file"] =
                    json!(args[1].to_str().unwrap());
                v[0]["Config"]["Labels"]["devcontainer.local_folder"] =
                    json!(args[3].to_str().unwrap());
            });
        }
        args
    }
    pub fn edit_fixture(&self, edit: impl FnOnce(&mut Value)) {
        let mut v = read_json(self.path("containers.json"));
        edit(&mut v);
        write_json(self.path("containers.json"), &v);
    }
    pub fn resolver_calls(&self) -> Vec<Value> {
        rows(self.path("resolver.log"))
    }
    pub fn herdr_calls(&self) -> Vec<Value> {
        rows(self.path("herdr.log"))
    }
    pub fn herdr(&mut self) {
        self.tool("herdr");
        self.set("HATCH_TEST_HERDR_LOG", self.path("herdr.log"));
        self.set("HERDR_ENV", "1");
        fs::write(self.path("herdr.sock"), "").unwrap();
        self.set("HERDR_SOCKET_PATH", self.path("herdr.sock"));
        self.set("XDG_STATE_HOME", self.path("state"));
        self.edit_fixture(|v| {
            v[0]["Mounts"][0]["Source"] = json!(self.path(""));
            v[0]["Config"]["Labels"]["devcontainer.local_folder"] = json!(self.path(""));
        });
    }
    pub fn pty(&self, args: &[OsString]) -> Pty {
        let mut c = self.command(binary());
        c.args(args);
        Pty::new(c)
    }
    pub fn menu(&self, args: &[OsString], answers: &[(&str, &str)], status: i32) -> String {
        let mut p = self.pty(args);
        for (marker, answer) in answers {
            p.expect(marker);
            p.send(format!("{answer}\n").as_bytes());
        }
        p.finish_with_review(status)
    }
}
struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn wait(child: &mut Child) -> Option<i32> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(s) = child.try_wait().unwrap() {
            return s.code();
        }
        assert!(Instant::now() < deadline, "child {} timed out", child.id());
        thread::sleep(Duration::from_millis(10));
    }
}
pub struct Pty {
    child: Guard,
    master: File,
    slave: File,
    original: libc::termios,
    bytes: Vec<u8>,
    pub output: String,
    cursor: usize,
}
impl Pty {
    pub fn new(mut command: Command) -> Self {
        let mut master = 0;
        let mut slave = 0;
        // SAFETY: openpty initializes the two descriptors; all optional pointers are null.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        // SAFETY: these are newly allocated, owned descriptors from openpty.
        let (master, slave) = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
        // Keep the original PTY descriptors in the test process only. The cloned
        // slave handles below become the child's standard input/output/error.
        for file in [&master, &slave] {
            assert_eq!(
                unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) },
                0
            );
        }
        let original = termios(&slave);
        command
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave.try_clone().unwrap());
        // SAFETY: child setup only calls async-signal-safe libc functions, with valid stdin.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Self {
            child: Guard(command.spawn().unwrap()),
            master,
            slave,
            original,
            bytes: Vec::new(),
            output: String::new(),
            cursor: 0,
        }
    }
    pub fn pid(&self) -> u32 {
        self.child.0.id()
    }
    pub fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).unwrap();
    }
    pub fn resize(&self, rows: u16, cols: u16) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &size) },
            0
        );
    }
    fn read(&mut self) {
        let mut fd = libc::pollfd {
            fd: self.master.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut fd, 1, 20) } > 0 {
            let mut b = [0; 8192];
            if let Ok(n) = self.master.read(&mut b) {
                self.bytes.extend_from_slice(&b[..n]);
                // A terminal read can split a multi-byte status icon.
                self.output = String::from_utf8_lossy(&self.bytes).into_owned();
            }
        }
    }
    pub fn expect(&mut self, marker: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(i) = self.output[self.cursor..].find(marker) {
                self.cursor += i + marker.len();
                return;
            }
            assert!(
                Instant::now() < deadline,
                "missing {marker:?}: {}",
                self.output
            );
            self.read();
        }
    }
    pub fn finish(self, status: i32) -> String {
        self.finish_inner(status, false)
    }
    // Existing flow fixtures accept the new final review explicitly when it appears.
    // Tests of confirmation itself use finish(), which never supplies input.
    pub fn finish_with_review(self, status: i32) -> String {
        self.finish_inner(status, true)
    }
    fn finish_inner(mut self, status: i32, mut accept_review: bool) -> String {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.read();
            if accept_review {
                if let Some((_, review)) = self.output.rsplit_once("Launch this session?") {
                    if review.contains("Confirm and launch")
                        || review.contains("Install Agent & Launch")
                    {
                        if review.contains("1) Confirm and launch")
                            || review.contains("1) Install Agent & Launch")
                        {
                            if review.contains(PROMPT) {
                                self.send(b"1\n");
                                accept_review = false;
                            }
                        } else {
                            self.send(b"\r");
                            accept_review = false;
                        }
                    }
                }
            }
            if let Some(s) = self.child.0.try_wait().unwrap() {
                self.read();
                assert_eq!(s.code(), Some(status), "{}", self.output);
                break;
            }
            assert!(Instant::now() < deadline, "PTY timeout: {}", self.output);
        }
        let now = termios(&self.slave);
        assert_eq!(
            (now.c_iflag, now.c_oflag, now.c_cflag, now.c_lflag, now.c_cc),
            (
                self.original.c_iflag,
                self.original.c_oflag,
                self.original.c_cflag,
                self.original.c_lflag,
                self.original.c_cc
            ),
            "terminal settings were not restored"
        );
        self.output.clone()
    }
}
impl Drop for Pty {
    fn drop(&mut self) {
        if self.child.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::kill(-(self.child.0.id() as i32), libc::SIGKILL);
            }
        }
    }
}
fn termios(file: &File) -> libc::termios {
    let mut t = std::mem::MaybeUninit::uninit();
    assert_eq!(
        unsafe { libc::tcgetattr(file.as_raw_fd(), t.as_mut_ptr()) },
        0
    );
    unsafe { t.assume_init() }
}
pub const PROMPT: &str = "(Esc/q goes back): ";
pub fn answers<'a>(values: &[&'a str]) -> Vec<(&'static str, &'a str)> {
    values.iter().map(|v| (PROMPT, *v)).collect()
}
pub fn append(args: &[OsString], tail: &[&str]) -> Vec<OsString> {
    let mut a = args.to_vec();
    a.extend(strings(tail));
    a
}
