//! Disposable executable doubles. Never contacts Docker, Herdr, or the network.
use serde_json::{json, Value};
use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, IsTerminal, Read, Write},
    os::unix::ffi::OsStrExt,
    path::Path,
    process,
};
fn variable(name: &str) -> String {
    env::var(name).unwrap_or_default()
}
fn log(name: &str, value: Value) {
    // A single append prevents parallel Docker doubles interleaving JSON fragments.
    let line = format!("{value}\n");
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(variable(name))
        .unwrap()
        .write_all(line.as_bytes())
        .unwrap();
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn fail(code: i32, message: &str) -> ! {
    eprintln!("{message}");
    process::exit(code)
}
fn docker(raw: &[OsString], args: &[String]) {
    log(
        "HATCH_TEST_LOG",
        json!({"args":raw.iter().map(|s| hex(s.as_bytes())).collect::<Vec<_>>(),"pid":process::id(),"agent":env::var("HERDR_AGENT").ok()}),
    );
    let mode = variable("HATCH_TEST_MODE");
    if mode == "docker-error" {
        fail(1, "fixture: Docker socket inaccessible")
    }
    match args[0].as_str() {
        "context" => println!(
            "{}",
            if args[1] == "show" {
                "default"
            } else {
                "unix:///fixture/docker.sock"
            }
        ),
        "ps" => {
            if mode != "empty" {
                println!("fixture-id")
            }
        }
        "inspect" => {
            if mode == "malformed" {
                println!("{{broken");
                return;
            }
            let mut data: Vec<Value> =
                serde_json::from_slice(&fs::read(variable("HATCH_TEST_FIXTURE")).unwrap()).unwrap();
            let target = args.last().unwrap();
            if target != "fixture-id" {
                data.retain(|v| {
                    v["Name"].as_str().unwrap().trim_start_matches('/') == target
                        || v["Id"] == *target
                });
            }
            if mode == "single" {
                data.truncate(1);
            }
            for v in &mut data {
                if mode == "stopped" {
                    v["State"]["Running"] = json!(false);
                }
                if mode == "paused" {
                    v["State"]["Paused"] = json!(true);
                }
            }
            println!("{}", json!(data));
        }
        "exec" => {
            let index = args
                .iter()
                .position(|a| a.len() == 64 && a.bytes().all(|b| b.is_ascii_hexdigit()))
                .unwrap_or(args.len());
            let launching = args[..index].iter().any(|a| a == "-i");
            // Worktree tests execute real Git only in their disposable repository.
            // Other fixtures deliberately model containers without Git.
            if !launching && index + 1 < args.len() && args[index + 1] == "git" {
                if env::var_os("HATCH_TEST_WORKTREE_GIT").is_none() {
                    fail(127, "fixture: Git is unavailable");
                }
                let cwd = args.iter().position(|a| a == "--workdir").unwrap();
                let status = process::Command::new(variable("HATCH_TEST_WORKTREE_GIT"))
                    .env("PATH", "/usr/bin:/bin")
                    .current_dir(&args[cwd + 1])
                    .args(&raw[index + 2..])
                    .status()
                    .unwrap();
                process::exit(status.code().unwrap_or(1));
            }
            if args
                .iter()
                .any(|a| a.starts_with("hatch-worktree-") || a == "hatch-project-setup")
            {
                if env::var_os("HATCH_TEST_WORKTREE_GIT").is_none() {
                    fail(127, "fixture: worktree filesystem unavailable");
                }
                let cwd = args.iter().position(|a| a == "--workdir").unwrap();
                let mut command = process::Command::new("/bin/sh");
                command.current_dir(&args[cwd + 1]).args(&raw[index + 2..]);
                // Forward only to the local shell in this disposable fixture.
                let status = command.status().unwrap();
                process::exit(status.code().unwrap_or(1));
            }
            if args.ends_with(&["login".into(), "status".into()]) {
                if variable("HATCH_TEST_AUTH_MODE") == "signed-out" {
                    process::exit(1);
                }
                println!("Logged in using fixture credentials");
                return;
            }
            if args.ends_with(&["auth".into(), "status".into()]) {
                println!(
                    "{}",
                    json!({"loggedIn":variable("HATCH_TEST_AUTH_MODE")!="signed-out","email":"private@example.test"})
                );
                return;
            }
            let state = format!("{}.installed", variable("HATCH_TEST_LOG"));
            if args.iter().any(|a| a == "hatch-locate") {
                match mode.as_str() {
                    "local-agent" => print!("found\0/home/dev/.local/bin/{}", args.last().unwrap()),
                    "broken-agent" => print!("found\0/usr/bin/{}", args.last().unwrap()),
                    _ => print!("missing\0/home/dev"),
                }
                return;
            }
            if args.iter().any(|a| a == "hatch-install") {
                if mode == "no-prereqs" {
                    fail(1, "Installation requires curl inside the container")
                }
                if args.last().unwrap() == "install" {
                    if mode == "install-fails" {
                        fail(1, "fixture: download failed")
                    }
                    fs::write(state, "").unwrap();
                }
                return;
            }
            if !launching
                && [
                    "local-agent",
                    "broken-agent",
                    "needs-install",
                    "install-fails",
                    "verify-fails",
                    "no-prereqs",
                ]
                .contains(&mode.as_str())
            {
                if (mode == "local-agent" && args[args.len() - 2].contains("/home/dev/.local/bin/"))
                    || (Path::new(&state).exists() && mode != "verify-fails")
                {
                    println!("fixture installed agent");
                    return;
                }
                fail(127, "fixture: cannot run executable");
            }
            if !launching {
                if let Some(gate) = env::var_os("HATCH_TEST_PROBE_GATE") {
                    if args.ends_with(&["codex".into(), "--version".into()]) {
                        let deadline =
                            std::time::Instant::now() + std::time::Duration::from_secs(10);
                        while !Path::new(&gate).exists() {
                            if std::time::Instant::now() >= deadline {
                                fail(124, "fixture gate timed out");
                            }
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }
                }
                if mode == "stdout-error" {
                    println!("fixture: OCI executable missing on PATH");
                    process::exit(127);
                }
                if mode == "missing-agent" {
                    fail(127, "fixture: executable not found")
                }
                if mode == "exec-failure" {
                    fs::remove_file(env::args_os().next().unwrap()).unwrap();
                }
                println!("fixture agent 1.0");
            } else {
                let mut input = Vec::new();
                if !io::stdin().is_terminal() {
                    io::stdin().read_to_end(&mut input).unwrap();
                }
                println!("{}", json!({"stdin":hex(&input)}));
                process::exit(variable("HATCH_TEST_EXIT").parse().unwrap_or(0));
            }
        }
        _ => fail(2, "unexpected fake Docker invocation"),
    }
}
fn resolver(args: &[String]) {
    log("HATCH_TEST_RESOLVER_LOG", json!(args));
    if args[0] == "up" {
        let mode = variable("HATCH_TEST_UP_MODE");
        match mode.as_str() {
            "error" => fail(9, "fixture build failed"),
            "invalid" => {
                println!("secret-invalid-result");
                return;
            }
            "unsuccessful" => {
                println!(
                    "{}",
                    json!({"outcome":"error","containerId":"a".repeat(64)})
                );
                return;
            }
            _ => (),
        }
        let path = variable("HATCH_TEST_FIXTURE");
        let mut data: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if mode != "wrong-target" {
            for (label, flag) in [
                ("devcontainer.local_folder", "--workspace-folder"),
                ("devcontainer.config_file", "--config"),
            ] {
                data[0]["Config"]["Labels"][label] =
                    json!(args[args.iter().position(|a| a == flag).unwrap() + 1]);
            }
        }
        data[0]["State"]["Running"] = json!(true);
        fs::write(path, data.to_string()).unwrap();
        println!(
            "{}",
            json!({"outcome":"success","containerId":"a".repeat(64)})
        );
        return;
    }
    let mode = variable("HATCH_TEST_RESOLVER_MODE");
    if mode == "error" {
        fail(1, "super-secret-config-value")
    }
    if mode == "invalid" {
        println!("super-secret-config-value");
        return;
    }
    let mut data = json!({"mergedConfiguration":{"workspaceFolder":"/custom/project","remoteUser":"2000","userEnvProbe":"none","remoteEnv":{"HATCH_TOKEN":"super-secret-config-value","HATCH_REMOVED":null}}});
    if mode == "compose" {
        data["mergedConfiguration"]["service"] = json!("web");
    }
    if mode == "unresolved" {
        data["mergedConfiguration"]["remoteEnv"]["HATCH_TOKEN"] = json!("${unknown:value}");
    }
    println!("{data}");
}
fn herdr(args: &[String]) {
    log("HATCH_TEST_HERDR_LOG", json!(args));
    let state = format!("{}.state", variable("HATCH_TEST_HERDR_LOG"));
    let mut data: Value = fs::read(&state)
        .ok()
        .map(|s| serde_json::from_slice(&s).unwrap())
        .unwrap_or(json!({"workspaces":[],"tabs":0}));
    let mode = variable("HATCH_TEST_HERDR_MODE");
    if mode == "unavailable" {
        process::exit(1);
    }
    let result = match (args[0].as_str(), args[1].as_str()) {
        ("workspace", "list") => json!({"workspaces":data["workspaces"]}),
        ("workspace", "create") => {
            let id = format!("w{}", data["workspaces"].as_array().unwrap().len() + 1);
            data["workspaces"]
                .as_array_mut()
                .unwrap()
                .push(json!({"workspace_id":id,"label":"same label is not identity"}));
            json!({"workspace":{"workspace_id":id},"root_pane":{"pane_id":format!("{id}:p1")}})
        }
        ("tab", "create") => {
            let n = data["tabs"].as_u64().unwrap() + 1;
            data["tabs"] = json!(n);
            let id = &args[args.iter().position(|a| a == "--workspace").unwrap() + 1];
            json!({"root_pane":{"pane_id":format!("{id}:p{}",n+1)}})
        }
        ("pane", "run") => {
            if mode == "run-fails" {
                println!("{}", json!({"error":{"message":"fixture failure"}}));
                return;
            }
            json!({"accepted":true})
        }
        _ => fail(2, "unexpected Herdr command"),
    };
    fs::write(state, data.to_string()).unwrap();
    if args[0] != "pane" {
        println!("{}", json!({"result":result}));
    }
}
fn main() {
    let raw: Vec<_> = env::args_os().skip(1).collect();
    let args: Vec<_> = raw
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let executable = env::args_os().next().unwrap();
    match Path::new(&executable)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
    {
        "docker" => docker(&raw, &args),
        "devcontainer" => resolver(&args),
        "herdr" => herdr(&args),
        "curl" => {
            fs::write(variable("HATCH_TEST_DOWNLOAD_LOG"), json!(args).to_string()).unwrap();
            fs::copy(variable("HATCH_TEST_PAYLOAD"), args.last().unwrap()).unwrap();
            process::exit(variable("HATCH_TEST_DOWNLOAD_EXIT").parse().unwrap_or(0));
        }
        _ if args.first().is_some_and(|s| s == "payload") => {
            fs::write(variable("HATCH_TEST_MARKER"),json!({"args":args[1..],"prefix":env::var("PREFIX").ok(),"path":variable("PATH"),"shell":env::var("SHELL").ok(),"codex_dir":env::var("CODEX_INSTALL_DIR").ok()}).to_string()).unwrap();
        }
        _ => fail(2, "unknown fixture role"),
    }
}
