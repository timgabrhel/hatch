mod support;
use serde_json::json;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
};
use support::*;
#[test]
fn host_install_runs_the_installed_binary_unless_disabled() {
    let mut h = Harness::new();
    h.set("PATH", "/usr/bin:/bin");
    let source = h.path("source");
    fs::write(&source, "#!/bin/sh\nprintf '%s\\n' \"$0\"\nexit 23\n").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).unwrap();
    let mut c = h.command(Path::new(ROOT).join("scripts/install.sh"));
    c.arg("--prefix")
        .arg(h.path("prefix"))
        .arg("--bin")
        .arg(&source);
    let result = h.execute(&mut c, b"", 23);
    assert_eq!(
        result.out.lines().last().unwrap(),
        h.path("prefix/bin/hatch").to_str().unwrap()
    );
    fs::remove_file(h.path("prefix/bin/hatch")).unwrap();
    c.arg("--no-launch");
    let result = h.execute(&mut c, b"", 0);
    assert_eq!(result.out.lines().count(), 1);
}
#[test]
fn host_install_is_atomic_preserves_cf_and_refuses_unrelated_hatch() {
    let mut h = Harness::new();
    h.set("PATH", "/usr/bin:/bin");
    fs::create_dir(h.path("bin")).unwrap();
    fs::write(h.path("bin/cf"), "legacy wrapper sentinel").unwrap();
    let mut c = h.command(Path::new(ROOT).join("scripts/install.sh"));
    c.arg("--prefix")
        .arg(h.path(""))
        .arg("--bin")
        .arg(binary())
        .arg("--no-launch");
    h.execute(&mut c, b"", 0);
    assert_eq!(
        fs::read(h.path("bin/hatch")).unwrap(),
        fs::read(binary()).unwrap()
    );
    assert_eq!(
        fs::read_to_string(h.path("bin/cf")).unwrap(),
        "legacy wrapper sentinel"
    );
    assert!(!fs::read_dir(h.path("bin")).unwrap().any(|p| p
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".hatch.")));
    fs::write(h.path("bin/hatch"), "#!/bin/sh\necho 'Unrelated CLI'\n").unwrap();
    fs::set_permissions(h.path("bin/hatch"), fs::Permissions::from_mode(0o755)).unwrap();
    let before = fs::read(h.path("bin/hatch")).unwrap();
    h.execute(&mut c, b"", 1);
    assert_eq!(fs::read(h.path("bin/hatch")).unwrap(), before);
    assert_eq!(
        fs::read_to_string(h.path("bin/cf")).unwrap(),
        "legacy wrapper sentinel"
    );
}
#[test]
fn native_routes_prerequisites_and_failed_partial_download_cleanup() {
    let mut h = Harness::new();
    fs::create_dir(h.path("tmp")).unwrap();
    for tool in [
        "sh",
        "bash",
        "mktemp",
        "rm",
        "mkdir",
        "chmod",
        "uname",
        "tar",
        "gzip",
        "awk",
        "sed",
        "grep",
        "head",
        "cut",
        "tr",
        "ln",
        "mv",
        "date",
        "basename",
        "dirname",
        "sha256sum",
        "sha512sum",
        "cp",
        "cat",
        "sort",
        "wc",
        "id",
        "ldd",
        "readlink",
        "od",
        "ls",
        "sleep",
    ] {
        let path = Path::new("/usr/bin").join(tool);
        assert!(path.exists(), "missing fixture prerequisite: {tool}");
        symlink(path, h.path(tool)).unwrap();
    }
    h.tool("curl");
    h.set("TMPDIR", h.path("tmp"));
    h.set("HATCH_TEST_MARKER", h.path("executed.json"));
    h.set("HATCH_TEST_PAYLOAD", h.path("payload.sh"));
    h.set("HATCH_TEST_DOWNLOAD_LOG", h.path("download.json"));
    h.set("HATCH_FIXTURE", FIXTURE);
    fs::write(
        h.path("payload.sh"),
        "exec \"$HATCH_FIXTURE\" payload \"$@\"\n",
    )
    .unwrap();
    for (agent, url) in [
        ("codex", "https://chatgpt.com/codex/install.sh"),
        ("claude", "https://claude.ai/install.sh"),
        ("copilot", "https://gh.io/copilot-install"),
        ("opencode", "https://opencode.ai/v2/install"),
        ("cursor-agent", "https://cursor.com/install"),
        ("grok", "https://x.ai/cli/install.sh"),
    ] {
        let mut c = h.command("/bin/sh");
        c.arg(Path::new(ROOT).join("src/scripts/install.sh"))
            .args([agent, "install"]);
        h.execute(&mut c, b"", 0);
        assert!(read_json(h.path("download.json"))
            .as_array()
            .unwrap()
            .contains(&json!(url)));
        let result = read_json(h.path("executed.json"));
        let home = h.path("home");
        match agent {
            "opencode" => assert_eq!(result["args"], json!(["--no-modify-path"])),
            "copilot" => assert_eq!(result["prefix"], json!(home.join(".local"))),
            "codex" => assert_eq!(result["codex_dir"], json!(home.join(".local/bin"))),
            "grok" => {
                assert!(result["path"]
                    .as_str()
                    .unwrap()
                    .split(':')
                    .any(|p| p == home.join(".grok/bin").to_str().unwrap()));
                assert_eq!(result["shell"], "/bin/sh");
            }
            _ => (),
        }
        fs::remove_file(h.path("executed.json")).unwrap();
        c.env("HATCH_TEST_DOWNLOAD_EXIT", "22");
        h.execute(&mut c, b"", 22);
        assert!(
            !h.path("executed.json").exists(),
            "partial download was executed for {agent}"
        );
        assert_eq!(
            fs::read_dir(h.path("tmp")).unwrap().count(),
            0,
            "temporary download left behind for {agent}"
        );
    }
    fs::remove_file(h.path("download.json")).unwrap();
    fs::remove_file(h.path("bash")).unwrap();
    let mut c = h.command("/bin/sh");
    c.arg(Path::new(ROOT).join("src/scripts/install.sh"))
        .args(["cursor-agent", "check"]);
    contains(&h.execute(&mut c, b"", 1).err, "requires bash");
    assert!(!h.path("download.json").exists());
}
