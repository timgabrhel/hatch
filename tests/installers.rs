mod support;
use serde_json::json;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
};
use support::*;
#[test]
fn piped_host_install_downloads_and_verifies_before_replacing_binary() {
    let mut h = Harness::new();
    let tools = h.path("download-tools");
    let assets = h.path("assets");
    fs::create_dir(&tools).unwrap();
    fs::create_dir(&assets).unwrap();
    let next_major: u64 = env!("CARGO_PKG_VERSION")
        .split('.')
        .next()
        .unwrap()
        .parse::<u64>()
        .unwrap()
        + 1;
    let release_version = format!("{next_major}.0.0");
    h.set("RELEASE_VERSION", &release_version);
    let name = format!("hatch-{release_version}-linux-{}", std::env::consts::ARCH);
    let package = assets.join(&name);
    fs::create_dir(&package).unwrap();
    fs::create_dir(package.join("tooling")).unwrap();
    for file in ["package.json", "package-lock.json"] {
        fs::write(package.join("tooling").join(file), "{}").unwrap();
    }
    let executable = package.join("hatch");
    fs::write(
        &executable,
        format!("#!/bin/sh\necho 'hatch {release_version}'\n"),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let archive = format!("{name}.tar.gz");
    assert!(std::process::Command::new("tar")
        .current_dir(&assets)
        .args(["-czf", &archive, &name])
        .status()
        .unwrap()
        .success());
    let checksum = std::process::Command::new("sha256sum")
        .current_dir(&assets)
        .arg(&archive)
        .output()
        .unwrap();
    assert!(checksum.status.success());
    let checksum_path = assets.join(format!("{archive}.sha256"));
    fs::write(&checksum_path, &checksum.stdout).unwrap();
    let curl = tools.join("curl");
    fs::write(
        &curl,
        r#"#!/bin/sh
set -eu
[ "${FAIL_DOWNLOAD:-}" != 1 ] || exit 22
output=
url=
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o) output=$2; shift 2 ;;
        https://*) url=$1; shift ;;
        *) shift ;;
    esac
done
case "$url" in
    */latest) printf 'https://github.com/timgabrhel/hatch/releases/tag/v%s' "$RELEASE_VERSION" ;;
    */download/v"$RELEASE_VERSION"/*) cp "$ASSETS/${url##*/}" "$output" ;;
    *) exit 90 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
    h.set("PATH", format!("{}:/usr/bin:/bin", tools.display()));
    h.set("ASSETS", assets.to_str().unwrap());
    let script = fs::read(Path::new(ROOT).join("scripts/install.sh")).unwrap();
    let mut c = h.command("sh");
    c.args(["-s", "--", "--prefix"]).arg(h.path("prefix"));
    h.execute(&mut c, &script, 0);
    let installed = h.path("prefix/bin/hatch");
    assert_eq!(
        fs::read(&installed).unwrap(),
        fs::read(&executable).unwrap()
    );
    // The real CLI must replace its own installation, without contacting Docker.
    let update_dir = h.path("custom installation");
    fs::create_dir(&update_dir).unwrap();
    let updater = update_dir.join("hatch");
    fs::copy(binary(), &updater).unwrap();
    let mut update = h.command(&updater);
    update.arg("update");
    h.execute(&mut update, b"", 0);
    assert_eq!(fs::read(&updater).unwrap(), fs::read(&executable).unwrap());
    h.no_exec();
    // Already-current releases do not download assets or rewrite the installation.
    let mut same = h.command("sh");
    same.args([
        "-c",
        std::str::from_utf8(&script).unwrap(),
        "hatch-update",
        "--update",
        "--install-dir",
    ])
    .arg(&update_dir);
    let result = h.execute(&mut same, b"", 0);
    contains(&result.out, "already up to date");
    // A failed download or checksum must leave the installed executable intact.
    let before = fs::read(&installed).unwrap();
    c.env("FAIL_DOWNLOAD", "1");
    h.execute(&mut c, &script, 22);
    c.env_remove("FAIL_DOWNLOAD");
    fs::write(&checksum_path, format!("{}  {archive}\n", "0".repeat(64))).unwrap();
    h.execute(&mut c, &script, 1);
    assert_eq!(fs::read(&installed).unwrap(), before);
    fs::copy(binary(), &updater).unwrap();
    let before = fs::read(&updater).unwrap();
    h.execute(&mut update, b"", 1);
    assert_eq!(fs::read(&updater).unwrap(), before);
}

#[test]
fn host_path_setup_requires_consent_and_does_not_duplicate_entries() {
    let mut h = Harness::new();
    h.set("PATH", "/usr/bin:/bin");
    h.set("SHELL", "/bin/bash");
    let prefix = h.path("a 'quoted' $(literal) prefix");
    let script = Path::new(ROOT).join("scripts/install.sh");
    let command = || {
        let mut c = h.command(&script);
        c.arg("--prefix")
            .arg(&prefix)
            .arg("--bin")
            .arg(binary())
            .args(["--no-launch", "--without-resolver"]);
        c
    };
    let mut terminal = Pty::new(command());
    terminal.expect("Add Hatch to PATH");
    terminal.send(b"n\n");
    terminal.finish(0);
    let profile = h.path("home/.bashrc");
    assert!(!profile.exists());
    let mut terminal = Pty::new(command());
    terminal.expect("Add Hatch to PATH");
    terminal.send(b"y\n");
    terminal.finish(0);
    let before = fs::read(&profile).unwrap();
    let mut shell = h.command("sh");
    shell.args(["-c", ". \"$HOME/.bashrc\"; printf '%s' \"$PATH\""]);
    let result = h.execute(&mut shell, b"", 0);
    assert!(result.out.starts_with(prefix.join("bin").to_str().unwrap()));
    let result = h.execute(&mut command(), b"", 0);
    contains(&result.err, "already configured");
    assert_eq!(fs::read(&profile).unwrap(), before);
    let mut present = command();
    present.env(
        "PATH",
        format!("{}:/usr/bin:/bin", prefix.join("bin").display()),
    );
    let result = h.execute(&mut present, b"", 0);
    assert!(!result.err.contains("not on PATH"));
    let mut unattended = command();
    unattended.arg("--no-input");
    fs::remove_file(&profile).unwrap();
    Pty::new(unattended).finish(0);
    assert!(!profile.exists());
}

#[test]
fn host_installer_offers_missing_resolver_and_keeps_existing_setup() {
    let mut h = Harness::new();
    let tools = h.path("tools");
    fs::create_dir(&tools).unwrap();
    for (name, script) in [
        ("node", "#!/bin/sh\nexit 0\n"),
        ("npm", "#!/bin/sh\nset -eu\nmkdir -p \"$3/node_modules/.bin\"\nprintf '#!/bin/sh\\nexit 0\\n' > \"$3/node_modules/.bin/devcontainer\"\nchmod +x \"$3/node_modules/.bin/devcontainer\"\n"),
    ] {
        let path = tools.join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let prefix = h.path("home/.local");
    h.set(
        "PATH",
        format!(
            "{}:{}:/usr/bin:/bin",
            tools.display(),
            prefix.join("bin").display()
        ),
    );
    let command = || {
        let mut c = h.command(Path::new(ROOT).join("scripts/install.sh"));
        c.arg("--bin").arg(binary()).arg("--no-launch");
        c
    };
    let mut terminal = Pty::new(command());
    terminal.expect("Install the Dev Container CLI");
    terminal.send(b"n\n");
    terminal.finish(0);
    assert!(!prefix.join("share/hatch/resolver").exists());
    let mut terminal = Pty::new(command());
    terminal.expect("Install the Dev Container CLI");
    terminal.send(b"y\n");
    terminal.finish(0);
    let resolver = prefix.join("bin/devcontainer");
    assert!(resolver.exists());
    let before = fs::read_link(&resolver).unwrap();
    let transcript = Pty::new(command()).finish(0);
    assert!(!transcript.contains("Install the Dev Container CLI"));
    assert_eq!(fs::read_link(&resolver).unwrap(), before);
}

#[test]
fn update_rejects_launch_inputs_without_docker() {
    let h = Harness::new();
    h.run(&["update", "--container", "dev"], 1);
    h.run(&["update", "--", "exec"], 1);
    h.no_exec();
}

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
