//! Explicit opt-in acceptance tests. Default cargo test never contacts Docker.
mod support;
use std::{env, fs, process::Command};
use support::*;
fn target() -> (String, String) {
    (
        env::var("HATCH_LIVE_CONTAINER")
            .expect("set HATCH_LIVE_CONTAINER to an existing container"),
        env::var("HATCH_LIVE_USER").expect("set HATCH_LIVE_USER to its intended development user"),
    )
}
#[test]
#[ignore = "requires explicit existing container/user and container-local env, sh, id, stty"]
fn terminal_launch() {
    let (container, user) = target();
    // env provides the --version preflight; no agent installation or model call.
    let script="trap 'printf \"INTERRUPTED\\n\"; exit 130' INT; trap 'stty size' WINCH; printf 'READY %s %s\\n' \"$(id -u)\" \"$PWD\"; while :; do if IFS= read -r line; then printf 'INPUT %s\\n' \"$line\"; fi; done";
    let mut c = Command::new(binary());
    c.args([
        "--container",
        &container,
        "--user",
        &user,
        "--here",
        "--no-input",
        "--metadata-only",
        "--workspace",
        "/tmp",
        "--agent-path",
        "/usr/bin/env",
        "--",
        "/bin/sh",
        "-c",
        script,
    ]);
    let mut p = Pty::new(c);
    p.expect("READY");
    assert_eq!(
        fs::read_link(format!("/proc/{}/exe", p.pid()))
            .unwrap()
            .file_name()
            .unwrap(),
        "docker",
        "Hatch remained resident"
    );
    p.resize(31, 101);
    p.expect("31 101");
    p.send(b"hello from host\n");
    p.expect("INPUT hello from host");
    p.send(b"\x03");
    p.expect("INTERRUPTED");
    p.finish(130);
}
struct Sandbox {
    container: String,
    user: String,
    path: String,
}
impl Sandbox {
    fn docker(&self, args: &[&str]) -> std::process::Output {
        let output = Command::new("docker")
            .args(["exec", "--user", &self.user, &self.container])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let result = Command::new("docker")
            .args([
                "exec",
                "--user",
                &self.user,
                &self.container,
                "rm",
                "-rf",
                "--",
                &self.path,
            ])
            .status();
        if !result.is_ok_and(|s| s.success()) {
            eprintln!("cleanup failed for owned sandbox directory {}", self.path);
        }
    }
}
#[test]
#[ignore = "requires explicit existing container/user with Codex already installed"]
fn sandbox_read_only() {
    let (container, user) = target();
    let output = Command::new("docker")
        .args([
            "exec",
            "--user",
            &user,
            &container,
            "mktemp",
            "-d",
            "/tmp/hatch-sandbox.XXXXXXXX",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let path = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    let suffix = path
        .strip_prefix("/tmp/hatch-sandbox.")
        .expect("unexpected temporary directory");
    assert!(suffix.len() == 8 && suffix.bytes().all(|b| b.is_ascii_alphanumeric()));
    let s = Sandbox {
        container,
        user,
        path,
    };
    s.docker(&[
        "/bin/sh",
        "-c",
        "printf sentinel > \"$1/read-check\"; printf writable > \"$1/write-check\"",
        "hatch-test",
        &s.path,
    ]);
    let h = Harness::new();
    let mut c = Command::new(binary());
    c.args(["--container",&s.container,"--user",&s.user,"--here","--no-input","--metadata-only","--workspace",&s.path,"--","sandbox","--","/bin/sh","-c", "test \"$(cat read-check)\" = sentinel || exit 2; error=$(LC_ALL=C sh -c 'printf changed > write-check' 2>&1) && exit 3; case \"$error\" in *'Permission denied'*|*'Read-only file system'*|*'Operation not permitted'*) printf 'PASS: read succeeded and write denied\\n';; *) printf '%s\\n' \"$error\"; exit 4;; esac"]);
    let r = h.execute(&mut c, b"", 0);
    contains(&r.out, "PASS: read succeeded and write denied");
    assert_eq!(
        s.docker(&["cat", &format!("{}/write-check", s.path)])
            .stdout,
        b"writable"
    );
}
