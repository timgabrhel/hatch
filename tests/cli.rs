mod support;
use serde_json::{json, Value};
use std::{ffi::OsString, fs, os::unix::ffi::OsStringExt};
use support::*;
fn last(h: &Harness) -> Vec<OsString> {
    h.calls().pop().unwrap().args
}
#[test]
fn help_version_and_errors_do_not_need_docker() {
    let h = Harness::new();
    fs::remove_file(h.path("docker")).unwrap();
    for a in [vec!["--help"], vec!["--version"], vec!["options"]] {
        h.run(&a, 0);
    }
    for a in [
        vec!["--agent", "wrong"],
        vec!["--agent", "antigravity"],
        vec!["--agent", "agy"],
        vec!["agents"],
        vec!["menu"],
        vec!["resume"],
        vec!["--container"],
    ] {
        h.run(&a, 2);
    }
    let simple = h.run(&["--help"], 0).out;
    assert!(!simple.contains("--no-tty"));
    contains(&simple, "hatch options");
    let advanced = h.run(&["options"], 0).out;
    contains(&advanced, "--no-tty");
    contains(&advanced, "Terminal features (TTY) are automatic");
    h.run(&["doctor"], 2);
    assert!(h.calls().is_empty());
}
#[test]
fn discovery_filters_ordinary_containers_and_never_executes() {
    let h = Harness::new();
    let v: Value = serde_json::from_str(&h.run(&["containers", "--json"], 0).out).unwrap();
    assert_eq!(
        v.as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].clone())
            .collect::<Vec<_>>(),
        vec![json!("alpha-dev"), json!("beta-compose")]
    );
    assert_eq!(
        h.calls()
            .iter()
            .map(|c| c.args[0].clone())
            .collect::<Vec<_>>(),
        strings(&["ps", "inspect"])
    );
}
#[test]
fn resolve_layouts() {
    let h = Harness::new();
    for (name, workspace, user, host) in [
        (
            "alpha-dev",
            "/workspaces/alpha",
            "developer",
            json!("/projects/alpha"),
        ),
        ("beta-compose", "/srv/beta", "1001:1001", Value::Null),
    ] {
        let v: Value =
            serde_json::from_str(&h.run(&["--container", name, "inspect"], 0).out).unwrap();
        assert_eq!(v["workspace"], workspace);
        assert_eq!(v["user"], user);
        assert_eq!(v["daemon_host_path"], host);
    }
    h.no_launch();
    h.no_install();
}
#[test]
fn exact_forwarding_stdin_exit_status_and_exec_pid() {
    let mut h = Harness::new();
    h.set("HATCH_TEST_EXIT", "23");
    let mut tail = strings(&["exec", "", "white space", "'\";$HOME`echo x`\n"]);
    tail.push(OsString::from_vec(vec![255]));
    tail.extend(strings(&["--", "--help"]));
    let mut args = strings(&["--container", "alpha-dev", "--no-input", "--"]);
    args.extend(tail.clone());
    let r = h.run_os(&args, b"input\0data\n", 23);
    let calls = h.calls();
    let final_call = calls.last().unwrap();
    assert_eq!(
        final_call.pid, r.pid,
        "Hatch must replace itself with Docker"
    );
    assert_eq!(final_call.agent, "codex");
    let mut expected = strings(&[
        "exec",
        "--user",
        "developer",
        "--workdir",
        "/workspaces/alpha",
        "-i",
        &"a".repeat(64),
        "codex",
    ]);
    expected.extend(tail);
    assert_eq!(final_call.args, expected);
    assert_eq!(
        serde_json::from_str::<Value>(&r.out).unwrap()["stdin"],
        "696e70757400646174610a"
    );
    assert!(calls[calls.len() - 2]
        .args
        .ends_with(&strings(&["codex", "--version"])));
}
#[test]
fn claude_admin_command_has_no_injected_flags() {
    let h = Harness::new();
    h.run(
        &[
            "--container",
            "alpha-dev",
            "--agent",
            "claude",
            "--",
            "login",
        ],
        0,
    );
    assert!(last(&h).ends_with(&strings(&["claude", "login"])));
    assert_eq!(h.calls().last().unwrap().agent, "claude");
}
#[test]
fn explicit_ordinary_container_and_executable() {
    let h = Harness::new();
    h.run(
        &[
            "--container",
            "ordinary",
            "--workspace",
            "/some dir",
            "--user",
            "42",
            "--agent-path",
            "/home/dev/.local/bin/codex",
        ],
        0,
    );
    assert_eq!(
        last(&h),
        strings(&[
            "exec",
            "--user",
            "42",
            "--workdir",
            "/some dir",
            "-i",
            &"c".repeat(64),
            "/home/dev/.local/bin/codex"
        ])
    );
}
#[test]
fn noninteractive_ambiguity_and_empty_discovery() {
    for (mode, message) in [
        ("", "multiple devcontainers"),
        ("empty", "no running devcontainers"),
    ] {
        let mut h = Harness::new();
        h.set("HATCH_TEST_MODE", mode);
        let r = h.run(&["--no-input"], 1);
        contains(
            &r.err.split_whitespace().collect::<Vec<_>>().join(" "),
            message,
        );
        h.no_exec();
    }
}
#[test]
fn failure_cases_never_launch() {
    for (mode, message) in [
        ("docker-error", "socket inaccessible"),
        ("malformed", "invalid Docker inspect"),
        ("stopped", "stopped or paused"),
        ("paused", "stopped or paused"),
        ("missing-agent", "container-local installation"),
        ("stdout-error", "container-local installation"),
        ("exec-failure", "could not replace Hatch"),
    ] {
        let mut h = Harness::new();
        h.set("HATCH_TEST_MODE", mode);
        let r = h.run(&["--container", "alpha-dev"], 1);
        contains(
            &r.err.split_whitespace().collect::<Vec<_>>().join(" "),
            message,
        );
        h.no_launch();
    }
}
#[test]
fn invalid_arguments_do_not_contact_docker() {
    let h = Harness::new();
    h.run(&["menu"], 2);
    h.run(&["inspect", "--", "login"], 1);
    h.run(&["--install", "--agent-path", "/usr/bin/codex"], 2);
    h.run(&["--install", "inspect"], 1);
    for path in ["relative", "/work/../etc", "${containerWorkspaceFolder}"] {
        h.run(&["--workspace", path], 1);
    }
    h.run(&["--agent-path", "codex"], 1);
    assert!(h.calls().is_empty());
}
#[test]
fn terminal_allocation_and_no_tty_override() {
    for no_tty in [false, true] {
        let h = Harness::new();
        let mut a = strings(&["--container", "alpha-dev", "--no-input"]);
        if no_tty {
            a.push("--no-tty".into());
        }
        h.pty(&a).finish(0);
        assert_eq!(last(&h).contains(&"-t".into()), !no_tty);
    }
}
#[test]
fn all_agents_forward_exact_arguments_and_install_paths() {
    for (name, executable, relative) in [
        ("codex", "codex", ".local/bin/codex"),
        ("claude", "claude", ".local/bin/claude"),
        ("copilot", "copilot", ".local/bin/copilot"),
        ("opencode", "opencode", ".opencode/bin/opencode"),
        ("cursor", "cursor-agent", ".local/bin/cursor-agent"),
        ("grok", "grok", ".grok/bin/grok"),
    ] {
        let mut h = Harness::new();
        let mut tail = strings(&["", "a b", "$HOME;whoami"]);
        tail.push(OsString::from_vec(vec![255]));
        for install in [false, true] {
            let mut a = strings(&["--container", "alpha-dev", "--agent", name]);
            if install {
                h.set("HATCH_TEST_MODE", "needs-install");
                a.push("--install".into());
            }
            a.push("--".into());
            a.extend(tail.clone());
            h.run_os(&a, b"", 0);
            let mut expected = vec![OsString::from(if install {
                format!("/home/dev/{relative}")
            } else {
                executable.to_owned()
            })];
            expected.extend(tail.clone());
            assert!(last(&h).ends_with(&expected));
            if install {
                let calls = h.calls();
                let install = calls.iter().rev().find(|c| c.has("hatch-install")).unwrap();
                assert_eq!(install.args.last().unwrap(), "install");
                assert!(install.has("developer"));
            }
        }
    }
}
#[test]
fn local_agent_is_found_without_installation() {
    let mut h = Harness::new();
    h.set("HATCH_TEST_MODE", "local-agent");
    h.run(&["--container", "alpha-dev"], 0);
    assert_eq!(last(&h).last().unwrap(), "/home/dev/.local/bin/codex");
    h.no_install();
}
#[test]
fn broken_agent_is_not_reinstalled() {
    let mut h = Harness::new();
    h.set("HATCH_TEST_MODE", "broken-agent");
    h.run(&["--container", "alpha-dev", "--install"], 1);
    h.no_install();
}
#[test]
fn installer_failures_do_not_launch() {
    for mode in ["no-prereqs", "install-fails", "verify-fails"] {
        let mut h = Harness::new();
        h.set("HATCH_TEST_MODE", mode);
        h.run(&["--container", "alpha-dev", "--install"], 1);
        h.no_launch();
    }
}
#[test]
fn noninteractive_install_requires_opt_in() {
    let mut h = Harness::new();
    h.set("HATCH_TEST_MODE", "needs-install");
    contains(
        &h.run(&["--container", "alpha-dev", "--no-input"], 1).err,
        "--install",
    );
    h.no_install();
}
#[test]
fn install_is_not_an_upgrade() {
    let h = Harness::new();
    h.run(&["--container", "alpha-dev", "--install"], 0);
    h.no_install();
}
#[test]
fn final_review_accepts_or_declines_installation() {
    for (answer, status, count) in [("q", 1, 0), ("1", 0, 1)] {
        let mut h = Harness::new();
        h.set("HATCH_TEST_MODE", "needs-install");
        h.menu(
            &strings(&["--container", "alpha-dev", "--agent", "codex"]),
            &[(PROMPT, answer)],
            status,
        );
        assert_eq!(
            h.calls()
                .iter()
                .filter(|c| c.has("hatch-install") && c.args.last().unwrap() == "install")
                .count(),
            count
        );
    }
}
#[test]
fn menu_container_then_agent_and_argument_forwarding() {
    let h = Harness::new();
    let t = h.menu(
        &strings(&["--", "resume", "a b", ""]),
        &answers(&["2", "2"]),
        0,
    );
    assert!(t.find("Select a container") < t.find("Select an agent"));
    assert!(last(&h).ends_with(&strings(&[&"b".repeat(64), "claude", "resume", "a b", ""])));
    assert!(last(&h).contains(&"/srv/beta".into()));
}
#[test]
fn menu_default_agent_and_single_container_still_prompt() {
    let mut h = Harness::new();
    h.set("HATCH_TEST_MODE", "single");
    let t = h.menu(&strings(&[]), &answers(&["1", ""]), 0);
    contains(&t, "Select a container");
    contains(&t, "Codex");
    assert_eq!(h.calls().last().unwrap().agent, "codex");
}
#[test]
fn menu_explicit_container_and_agent_default() {
    let h = Harness::new();
    let t = h.menu(
        &strings(&["--container", "alpha-dev", "--agent", "claude"]),
        &[],
        0,
    );
    assert!(!t.contains("Select a container"));
    assert!(!t.contains("Back to containers"));
    assert!(!t.contains("Select an agent"));
    assert_eq!(h.calls().last().unwrap().agent, "claude");
}
#[test]
fn menu_back_then_select_another_container() {
    let h = Harness::new();
    let t = h.menu(&strings(&[]), &answers(&["1", "q", "2", "1"]), 0);
    assert_eq!(t.matches("Select a container").count(), 2);
    assert!(!t.contains("Back to containers"));
    assert_eq!(t.matches("Launch agent in container").count(), 1);
    contains(&t, "Choose where to work and which coding agent to run.");
    assert!(last(&h).ends_with(&strings(&[&"b".repeat(64), "codex"])));
}
#[test]
fn prompt_cancellation_has_no_exec() {
    for a in [vec!["q"], vec!["1", "q", "q"]] {
        let h = Harness::new();
        h.menu(&strings(&["--install"]), &answers(&a), 1);
        h.no_exec();
    }
}
#[test]
fn default_is_guided_even_for_single_container_and_flags_skip_steps() {
    let mut h = Harness::new();
    h.set("HATCH_TEST_MODE", "single");
    let t = h.menu(&[], &answers(&["1", "2"]), 0);
    contains(&t, "Select a container");
    contains(&t, "Select an agent");
    assert_eq!(last(&h).last().unwrap(), "claude");
    let t = h.menu(&strings(&["--agent", "grok"]), &answers(&["1"]), 0);
    assert!(!t.contains("Select an agent"));
    assert_eq!(last(&h).last().unwrap(), "grok");
    h.menu(
        &strings(&["--container", "alpha-dev", "--agent", "cursor"]),
        &[],
        0,
    );
    assert_eq!(last(&h).last().unwrap(), "cursor-agent");
    h.run(&["--no-input"], 1);
}
#[test]
fn arrow_menu_and_terminal_restoration() {
    let mut h = Harness::new();
    h.env.remove(std::ffi::OsStr::new("HATCH_PLAIN"));
    h.set("TERM", "xterm-256color");
    let mut p = h.pty(&strings(&[]));
    p.expect("Select a container");
    p.send(b"\x1b[B\r");
    p.expect("Select an agent");
    p.send(b"\x1b[B\r");
    let t = p.finish_with_review(0);
    assert!(last(&h).ends_with(&strings(&[&"b".repeat(64), "claude"])));
    contains(&t, "| |__   __ _| |_ ___| |__");
    contains(&t, concat!("v", env!("CARGO_PKG_VERSION")));
    assert!(
        t.find("Step inside. Get to work.").unwrap()
            < t.find(concat!("v", env!("CARGO_PKG_VERSION"))).unwrap()
    );
}
#[test]
fn arrow_escape_back_then_cancel_restores_terminal() {
    let mut h = Harness::new();
    h.env.remove(std::ffi::OsStr::new("HATCH_PLAIN"));
    h.set("TERM", "xterm-256color");
    let mut p = h.pty(&strings(&[]));
    p.expect("Select a container");
    p.send(b"\r");
    p.expect("Select an agent");
    p.send(b"\x1b");
    p.expect("Select a container");
    p.send(b"\x1b");
    let t = p.finish(1);
    assert_eq!(t.matches("Esc back").count(), 1);
    h.no_exec();
}
#[test]
fn resolved_configuration_and_redacted_inspect() {
    let mut h = Harness::new();
    let a = h.resolver();
    let r = h.run_os(&append(&a, &["inspect"]), b"", 0);
    let v: Value = serde_json::from_str(&r.out).unwrap();
    assert_eq!(v["workspace"], "/custom/project");
    assert_eq!(v["user"], "2000");
    assert_eq!(v["configuration_source"], "devcontainer-cli");
    assert!(!r.out.contains("super-secret"));
    assert_eq!(
        v["remote_env_names"],
        json!(["HATCH_REMOVED", "HATCH_TOKEN"])
    );
    let calls = h.resolver_calls();
    let c = calls[0].as_array().unwrap();
    assert!(c.contains(&json!("--include-merged-configuration")));
    assert_eq!(
        c[c.iter().position(|v| v == "--container-id").unwrap() + 1],
        "a".repeat(64)
    );
    h.no_launch();
    h.no_install();
}
#[test]
fn config_identity_reuses_running_without_up() {
    let mut h = Harness::new();
    let a = h.provision(true);
    for command in ["", "up", "inspect"] {
        let mut args = if command.is_empty() {
            vec![]
        } else {
            strings(&[command])
        };
        args.extend(append(&a, &["--no-input"]));
        h.run_os(&args, b"", 0);
    }
    assert!(h
        .resolver_calls()
        .iter()
        .all(|c| c[0] == "read-configuration"));
    assert!(h
        .calls()
        .iter()
        .any(|c| c.args[0] == "inspect" && c.args.last().unwrap() == "fixture-id"));
}
#[test]
fn config_readonly_and_unattended_never_create() {
    let mut h = Harness::new();
    let a = h.provision(false);
    for command in ["", "inspect"] {
        let mut args = if command.is_empty() {
            vec![]
        } else {
            strings(&[command])
        };
        args.extend(append(&a, &["--no-input"]));
        contains(&h.run_os(&args, b"", 1).err, "hatch up");
    }
    let r = h.run_os(&append(&a, &["containers", "--json"]), b"", 0);
    assert_eq!(serde_json::from_str::<Value>(&r.out).unwrap(), json!([]));
    assert!(h.resolver_calls().is_empty());
    h.no_exec();
}
#[test]
fn up_forwards_identity_input_and_agent_arguments() {
    let mut h = Harness::new();
    let a = h.provision(false);
    let mut args = append(
        &a,
        &[
            "up",
            "--no-input",
            "--agent",
            "claude",
            "--",
            "",
            "a b",
            "$HOME",
        ],
    );
    args.push(OsString::from_vec(vec![255]));
    let r = h.run_os(&args, b"agent stdin", 0);
    let expected = json!([
        "up",
        "--config",
        a[1].to_str().unwrap(),
        "--workspace-folder",
        a[3].to_str().unwrap(),
        "--id-label",
        format!("devcontainer.local_folder={}", a[3].to_string_lossy()),
        "--id-label",
        format!("devcontainer.config_file={}", a[1].to_string_lossy()),
        "--no-lockfile"
    ]);
    assert_eq!(h.resolver_calls()[0], expected);
    assert_eq!(h.calls().last().unwrap().pid, r.pid);
    assert!(last(&h).ends_with(&args[args.len() - 4..]));
    assert_eq!(
        serde_json::from_str::<Value>(&r.out).unwrap()["stdin"],
        "6167656e7420737464696e"
    );
}
#[test]
fn up_failures_never_launch() {
    for mode in ["error", "invalid", "unsuccessful", "wrong-target"] {
        let mut h = Harness::new();
        let a = h.provision(false);
        h.set("HATCH_TEST_UP_MODE", mode);
        let r = h.run_os(&append(&a, &["up", "--no-input"]), b"", 1);
        assert!(!format!("{}{}", r.out, r.err).contains("secret-invalid-result"));
        h.no_exec();
    }
}
#[test]
fn up_argument_validation_and_missing_tool() {
    let mut h = Harness::new();
    h.run(&["up"], 1);
    let a = h.provision(false);
    h.run_os(&append(&a, &["up", "--container", "alpha-dev"]), b"", 1);
    assert!(h.calls().is_empty());
    fs::remove_file(h.path("devcontainer")).unwrap();
    contains(
        &h.run_os(&append(&a, &["up"]), b"", 1).err,
        "could not start Dev Container CLI",
    );
}
#[test]
fn stopped_match_is_started_but_ambiguous_running_matches_fail() {
    let mut h = Harness::new();
    let a = h.provision(true);
    h.edit_fixture(|v| v[0]["State"]["Running"] = json!(false));
    h.run_os(&append(&a, &["up", "--no-input"]), b"", 0);
    assert_eq!(h.resolver_calls()[0][0], "up");
    h.edit_fixture(|v| {
        let mut duplicate = v[0].clone();
        duplicate["Id"] = json!("d".repeat(64));
        duplicate["Name"] = json!("/duplicate");
        v.as_array_mut().unwrap().push(duplicate);
    });
    fs::remove_file(h.path("calls.jsonl")).unwrap();
    fs::remove_file(h.path("resolver.log")).unwrap();
    h.run_os(&append(&a, &["up", "--no-input"]), b"", 1);
    assert!(h.resolver_calls().is_empty());
    h.no_exec();
}
#[test]
fn matching_workspace_with_different_profile_never_attaches() {
    let mut h = Harness::new();
    let a = h.provision(true);
    h.edit_fixture(|v| {
        v[0]["Config"]["Labels"]["devcontainer.config_file"] =
            json!(format!("{}.different", a[1].to_string_lossy()))
    });
    h.run_os(&append(&a, &["--no-input"]), b"", 1);
    assert!(h.resolver_calls().is_empty());
    h.no_exec();
}
#[test]
fn config_missing_match_can_cancel_or_create_interactively() {
    let mut h = Harness::new();
    let a = h.provision(false);
    let args = append(&a, &[]);
    h.menu(&args, &answers(&["1", ""]), 1);
    assert!(h.resolver_calls().is_empty());
    h.menu(&args, &answers(&["2", "2"]), 0);
    assert_eq!(h.resolver_calls()[0][0], "up");
    assert_eq!(last(&h).last().unwrap(), "claude");
}
#[test]
fn resolved_env_launch_and_explicit_override() {
    let mut h = Harness::new();
    let a = h.resolver();
    h.run_os(
        &append(
            &a,
            &["--user", "42", "--workspace", "/override", "--", "login"],
        ),
        b"",
        0,
    );
    let c = last(&h);
    for s in ["42", "/override", "HATCH_TOKEN=super-secret-config-value"] {
        assert!(c.contains(&s.into()));
    }
    assert!(c.ends_with(&strings(&["env", "-u", "HATCH_REMOVED", "codex", "login"])));
}
#[test]
fn resolver_failures_do_not_leak_or_launch() {
    for mode in ["error", "invalid", "compose", "unresolved"] {
        let mut h = Harness::new();
        let a = h.resolver();
        h.set("HATCH_TEST_RESOLVER_MODE", mode);
        let r = h.run_os(&a, b"", 1);
        assert!(!format!("{}{}", r.out, r.err).contains("super-secret"));
        h.no_exec();
    }
}
#[test]
fn require_config_and_metadata_only() {
    let h = Harness::new();
    h.run(
        &["--container", "alpha-dev", "--require-config", "inspect"],
        1,
    );
    let v: Value = serde_json::from_str(
        &h.run(
            &["--container", "alpha-dev", "--metadata-only", "inspect"],
            0,
        )
        .out,
    )
    .unwrap();
    assert_eq!(v["configuration_source"], "docker-metadata");
    assert_eq!(v["warnings"], json!([]));
}
#[test]
fn config_discovery_without_profile_label() {
    let mut h = Harness::new();
    h.resolver();
    h.edit_fixture(|v| {
        v[0]["Config"]["Labels"]
            .as_object_mut()
            .unwrap()
            .remove("devcontainer.config_file");
        v[0]["Config"]["Labels"]["devcontainer.local_folder"] = json!(h.path(""));
    });
    let folder = h.path(".devcontainer");
    fs::create_dir(&folder).unwrap();
    let default = folder.join("devcontainer.json");
    fs::write(&default, "{}").unwrap();
    let r = h.run(&["--container", "alpha-dev", "inspect"], 0);
    assert_eq!(
        serde_json::from_str::<Value>(&r.out).unwrap()["config_file"],
        json!(default)
    );
    fs::remove_file(default).unwrap();
    for n in ["one", "two"] {
        fs::create_dir(folder.join(n)).unwrap();
        fs::write(folder.join(n).join("devcontainer.json"), "{}").unwrap();
    }
    contains(
        &h.run(&["--container", "alpha-dev", "inspect"], 1).err,
        "Multiple devcontainer profiles",
    );
}
#[test]
fn inspect_reports_status_without_credentials_or_installation() {
    let h = Harness::new();
    let r = h.run(&["--container", "alpha-dev", "inspect"], 0);
    let v: Value = serde_json::from_str(&r.out).unwrap();
    let agents = v["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 6);
    assert!(agents.iter().all(|a| a.get("authentication").is_none()));
    assert!(!r.out.contains("private@example.test"));
    assert!(!r.out.contains("fixture credentials"));
    let auth_calls = h
        .calls()
        .into_iter()
        .filter(|c| c.args.last().is_some_and(|a| a == "status"))
        .collect::<Vec<_>>();
    assert!(auth_calls.is_empty());
    h.no_install();
}
#[test]
fn terminal_diagnostics_are_readable_and_json_remains_available() {
    let h = Harness::new();
    let t = h.menu(&strings(&["inspect", "--container", "alpha-dev"]), &[], 0);
    contains(&t, "Agents in alpha-dev");
    assert!(!t.contains("AUTH CHECK"));
    assert!(!t.contains("LOGIN"));
    assert!(!t.contains("\"remote_env_names\""));
    let t = h.menu(&strings(&["inspect", "--container", "alpha-dev"]), &[], 0);
    contains(&t, "Launch context");
    contains(&t, "hatch inspect --json");
    let r = h.run(&["inspect", "--json", "--container", "alpha-dev"], 0);
    assert_eq!(
        serde_json::from_str::<Value>(&r.out).unwrap()["agents"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}
#[test]
fn inspect_icons_distinguish_available_and_missing() {
    let mut h = Harness::new();
    h.env.remove(std::ffi::OsStr::new("HATCH_PLAIN"));
    h.set("TERM", "xterm-256color");
    h.set("NO_COLOR", "1");
    let a = strings(&["inspect", "--container", "alpha-dev"]);
    let t = h.menu(&a, &[], 0);
    assert_eq!(t.matches("✓ Available").count(), 6);
    assert!(!t.contains("Authenticated"));
    assert!(!t.contains("Unsupported check"));
    h.set("HATCH_PLAIN", "1");
    h.set("HATCH_TEST_MODE", "missing-agent");
    let t = h.menu(&a, &[], 0);
    assert_eq!(t.matches("[x] Missing/unusable").count(), 6);
    assert!(!t.contains("[ok] Authenticated"));
}
fn handoff(h: &Harness) -> String {
    h.herdr_calls()
        .iter()
        .find(|c| c[0] == "pane" && c[1] == "run")
        .unwrap()[3]
        .as_str()
        .unwrap()
        .to_owned()
}
fn parse_handoff(h: &Harness, command: &str) -> Vec<String> {
    let marker = h.path("forwarded.json");
    let mut c = h.command("/bin/sh");
    c.args([
        "-c",
        &format!("set -- {command}\nexec \"$HATCH_FIXTURE\" payload \"$@\""),
    ])
    .env("HATCH_FIXTURE", FIXTURE)
    .env("HATCH_TEST_MARKER", &marker);
    h.execute(&mut c, b"", 0);
    serde_json::from_value(read_json(marker)["args"].clone()).unwrap()
}
#[test]
fn herdr_explicit_handoff_creates_fresh_workspaces() {
    let mut h = Harness::new();
    h.herdr();
    let tail = ["resume", "a b'$HOME;\n", ""];
    let a = append(
        &strings(&[
            "--container",
            "alpha-dev",
            "--agent",
            "claude",
            "--herdr",
            "--",
        ]),
        &tail,
    );
    h.run_os(&a, b"", 0);
    h.run(
        &["--container", "alpha-dev", "--agent", "codex", "--herdr"],
        0,
    );
    let calls = h.herdr_calls();
    assert_eq!(
        calls
            .iter()
            .filter(|c| c[0] == "workspace" && c[1] == "create")
            .count(),
        2
    );
    assert!(!calls.iter().any(|c| c[0] == "tab"));
    let argv = parse_handoff(&h, &handoff(&h));
    assert_eq!(&argv[argv.len() - tail.len()..], tail);
    for a in [
        "--here",
        "--no-input",
        "DOCKER_CONTEXT=default",
        &"a".repeat(64),
    ] {
        assert!(argv.contains(&a.to_string()));
    }
    assert_eq!(
        argv[argv.iter().position(|s| s == "--agent").unwrap() + 1],
        "claude"
    );
    assert!(!argv.contains(&"--herdr".to_owned()));
    h.no_launch();
    assert!(h
        .calls()
        .iter()
        .any(|c| c.args.last().unwrap() == "--version"));
}
#[test]
fn herdr_failure_does_not_retry_or_launch_locally() {
    let mut h = Harness::new();
    h.herdr();
    h.set("HATCH_TEST_HERDR_MODE", "run-fails");
    contains(
        &h.run(&["--container", "alpha-dev", "--herdr"], 1).err,
        "may have received",
    );
    assert_eq!(
        h.herdr_calls()
            .iter()
            .filter(|c| c[0] == "pane" && c[1] == "run")
            .count(),
        1
    );
    h.no_launch();
}
#[test]
fn herdr_unavailable_falls_back() {
    let mut h = Harness::new();
    h.herdr();
    h.set("HATCH_TEST_HERDR_MODE", "unavailable");
    h.run(&["--container", "alpha-dev", "--herdr"], 0);
    assert_eq!(last(&h).last().unwrap(), "codex");
    assert_eq!(h.herdr_calls(), vec![json!(["workspace", "list"])]);
}
#[test]
fn herdr_rejects_non_utf8_before_creating_anything() {
    let mut h = Harness::new();
    h.herdr();
    let mut a = strings(&["--container", "alpha-dev", "--herdr", "--"]);
    a.push(OsString::from_vec(vec![255]));
    h.run_os(&a, b"", 1);
    assert!(h
        .herdr_calls()
        .iter()
        .all(|c| *c == json!(["workspace", "list"])));
}
#[test]
fn default_herdr_launch_opens_agent_here_without_destination_menu() {
    let mut h = Harness::new();
    h.herdr();
    let output = h.menu(&strings(&[]), &answers(&["1", "5"]), 0);
    assert!(!output.contains("Open agent in Herdr"));
    assert!(!output.contains("Herdr:"));
    contains(&output, "CLI equivalent");
    contains(&output, "--here");
    assert!(h.herdr_calls().is_empty());
    assert_eq!(last(&h).last().unwrap(), "cursor-agent");
}

#[test]
fn here_and_piped_input_stay_in_current_terminal() {
    let mut h = Harness::new();
    h.herdr();
    h.menu(&strings(&["--here"]), &answers(&["1", "1"]), 0);
    assert!(h.herdr_calls().is_empty());
    let r = h.run_os(&strings(&["--container", "alpha-dev"]), b"piped text", 0);
    assert_eq!(
        serde_json::from_str::<Value>(&r.out).unwrap()["stdin"],
        "70697065642074657874"
    );
    assert!(h.herdr_calls().is_empty());
}
#[test]
fn combined_install_and_launch_follows_herdr_selection_and_review() {
    for (answer, status) in [(b"q\n", 1), (b"1\n", 0)] {
        let mut h = Harness::new();
        h.herdr();
        h.set("HATCH_TEST_MODE", "needs-install");
        let mut p = h.pty(&strings(&[
            "--container",
            "alpha-dev",
            "--agent",
            "codex",
            "--herdr",
        ]));
        p.expect("Your session");
        p.expect("Install Agent & Launch");
        p.expect(PROMPT);
        assert!(!p.output.contains("Install now?"));
        assert!(!p.output.contains("Container selected"));
        assert!(!p.output.contains("Agent not installed"));
        contains(&p.output, "! Install");
        assert!(!p.output.contains("reinstall the agent"));
        assert!(h
            .herdr_calls()
            .iter()
            .all(|c| *c == json!(["workspace", "list"])));
        p.send(answer);
        let output = p.finish_with_review(status);
        if status == 0 {
            let summary = output.split("Your session").last().unwrap();
            contains(summary, "Herdr:");
            contains(summary, "New workspace");
            assert!(!summary.contains("Dockerfile"));
            assert!(!summary.contains("reinstall the agent"));
        }
        if status == 1 {
            assert!(h
                .herdr_calls()
                .iter()
                .all(|c| *c == json!(["workspace", "list"])));
        } else {
            let argv = parse_handoff(&h, &handoff(&h));
            assert!(argv.contains(&"--no-input".into()));
            assert!(!argv.contains(&"--install".into()));
            assert_eq!(
                argv[argv.iter().position(|a| a == "--agent-path").unwrap() + 1],
                "/home/dev/.local/bin/codex"
            );
        }
    }
}
#[test]
fn review_back_clears_wrapped_summary_and_does_not_accumulate_copies() {
    for plain in [false, true] {
        let mut h = Harness::new();
        h.herdr();
        h.set("HATCH_TEST_MODE", "needs-install");
        h.set("NO_COLOR", "1");
        if !plain {
            h.env.remove(std::ffi::OsStr::new("HATCH_PLAIN"));
            h.set("TERM", "xterm-256color");
        }
        let mut p = h.pty(&strings(&[
            "--container",
            "alpha-dev",
            "--",
            &"long argument ".repeat(15),
        ]));
        p.resize(40, 80);
        p.expect("Select an agent");
        for _ in 0..2 {
            p.expect(if plain { PROMPT } else { "Grok Build" });
            p.send(b"\r");
            p.expect("Launch this session?");
            p.expect(if plain {
                PROMPT
            } else {
                "Install Agent & Launch"
            });
            assert!(!p.output.contains("Install now?"));
            if plain {
                assert_eq!(p.output.matches("Your session").count(), 1);
            }
            let before_back = p.output.len();
            p.send(b"\x1b");
            p.expect("Select an agent");
            if !plain {
                // Clearing must include the summary and its wrapped argument,
                // not merely the three rows of the confirmation menu.
                let cleared = p.output[before_back..]
                    .split("\x1b[")
                    .filter_map(|s| s.split_once('A').and_then(|(n, _)| n.parse::<usize>().ok()))
                    .max()
                    .unwrap_or(0);
                assert!(
                    cleared >= 14,
                    "review left behind: {}",
                    &p.output[before_back..]
                );
            }
        }
        p.expect(if plain { PROMPT } else { "Grok Build" });
        p.send(b"\x1b");
        p.finish(1);
        assert!(h
            .calls()
            .iter()
            .all(|c| !c.has("hatch-install") || c.args.last().unwrap() != "install"));
        h.no_launch();
        assert!(h
            .herdr_calls()
            .iter()
            .all(|c| *c == json!(["workspace", "list"])));
    }
}
#[test]
fn session_review_gates_install_setup_and_herdr_handoff() {
    for plain in [false, true] {
        for confirm in [false, true] {
            let mut h = Harness::new();
            h.herdr();
            h.edit_fixture(|v| v[0]["Mounts"][0]["Destination"] = json!(h.dir.path()));
            h.set("HATCH_TEST_WORKTREE_GIT", "/usr/bin/git");
            h.set("HATCH_TEST_MODE", "needs-install");
            if !plain {
                h.env.remove(std::ffi::OsStr::new("HATCH_PLAIN"));
                h.set("TERM", "xterm-256color");
            }
            let mut p = h.pty(&strings(&[
                "--container",
                "alpha-dev",
                "--agent",
                "codex",
                "--install",
                "--herdr",
                "--workspace",
                h.dir.path().to_str().unwrap(),
                "--setup-command",
                "echo fixture-setup",
                "--",
                "--version",
            ]));
            p.expect("Your session");
            p.expect("Launch this session?");
            p.expect(if plain {
                PROMPT
            } else {
                "Install Agent & Launch"
            });
            contains(&p.output, "! Install");
            contains(&p.output, "echo fixture-setup");
            contains(&p.output, "Run setup command before launch");
            assert!(!p.output.contains(") Back"));
            contains(&p.output, "--version");
            assert!(h.calls().iter().all(|c| !(c.has("hatch-install")
                && c.args.last().unwrap() == "install")
                && !c.has("hatch-project-setup")));
            assert!(h
                .herdr_calls()
                .iter()
                .all(|c| *c == json!(["workspace", "list"])));
            h.no_launch();
            if confirm {
                p.send(if plain { b"1\r" } else { b"\r" });
                p.finish(0);
                assert!(h
                    .calls()
                    .iter()
                    .any(|c| c.has("hatch-install") && c.args.last().unwrap() == "install"));
                assert!(h.calls().iter().any(|c| c.has("hatch-project-setup")));
                assert!(h
                    .herdr_calls()
                    .iter()
                    .any(|c| c[0] == "pane" && c[1] == "run"));
            } else {
                p.send(b"\x1b");
                p.finish(1);
                assert!(h.calls().iter().all(|c| !(c.has("hatch-install")
                    && c.args.last().unwrap() == "install")
                    && !c.has("hatch-project-setup")));
                assert!(h
                    .herdr_calls()
                    .iter()
                    .all(|c| *c == json!(["workspace", "list"])));
                h.no_launch();
            }
        }
    }
}
#[test]
fn cancelling_review_never_requests_or_runs_installation() {
    for consent_flag in [false, true] {
        let mut h = Harness::new();
        h.herdr();
        h.set("HATCH_TEST_MODE", "needs-install");
        let mut args = strings(&["--container", "alpha-dev", "--agent", "codex"]);
        if consent_flag {
            args.push("--install".into());
        }
        let mut p = h.pty(&args);
        p.expect("Launch this session?");
        p.expect(PROMPT);
        assert!(!p.output.contains("Install now?"));
        assert!(h
            .calls()
            .iter()
            .all(|c| !c.has("hatch-install") || c.args.last().unwrap() != "install"));
        p.send(b"\x1b");
        p.finish(1);
        assert!(h
            .calls()
            .iter()
            .all(|c| !c.has("hatch-install") || c.args.last().unwrap() != "install"));
        h.no_launch();
        assert!(h
            .herdr_calls()
            .iter()
            .all(|c| *c == json!(["workspace", "list"])));
    }
}
#[test]
fn failed_install_does_not_open_herdr() {
    for mode in [
        "install-fails",
        "no-prereqs",
        "verify-fails",
        "broken-agent",
    ] {
        let mut h = Harness::new();
        h.herdr();
        h.set("HATCH_TEST_MODE", mode);
        h.run(&["--container", "alpha-dev", "--herdr", "--install"], 1);
        assert!(h
            .herdr_calls()
            .iter()
            .all(|c| *c == json!(["workspace", "list"])));
    }
}
#[test]
fn container_browser_accepts_config_and_detects_workspace_file() {
    let mut h = Harness::new();
    let a = h.provision(true);
    fs::write(
        h.path("project.code-workspace"),
        "{ /* comment */ \"folders\": [{\"path\": \".\",},], }",
    )
    .unwrap();
    let t = h.menu(
        &strings(&["containers"]),
        &[
            (PROMPT, "3"),
            (PROMPT, "2"),
            ("devcontainer.json path", a[1].to_str().unwrap()),
            (PROMPT, "1"),
            (PROMPT, "2"),
        ],
        0,
    );
    contains(&t, "New from devcontainer.json");
    contains(&t, "project.code-workspace");
    assert!(t.find("Select a workspace") < t.find("Select an agent"));
    assert_eq!(last(&h).last().unwrap(), "claude");
    assert!(h
        .resolver_calls()
        .iter()
        .all(|c| c[0] == "read-configuration"));
}
#[test]
fn first_run_without_containers_can_enter_config_or_cancel() {
    let mut h = Harness::new();
    h.set("HATCH_TEST_MODE", "empty");
    let t = h.menu(
        &[],
        &[(PROMPT, "1"), ("devcontainer.json path", ""), (PROMPT, "q")],
        1,
    );
    contains(&t, "New from devcontainer.json");
    h.no_exec();
}
#[test]
fn manual_workspace_file_with_multiple_roots_and_spaces() {
    let mut h = Harness::new();
    let a = h.provision(true);
    fs::create_dir(h.path("workspace files")).unwrap();
    let file = h.path("workspace files/two roots.code-workspace");
    fs::write(&file,"{\"folders\":[{\"name\":\"Parent\",\"path\":\"..\"},{\"name\":\"Nested\",\"path\":\".\"}]}").unwrap();
    let quoted = format!("\"{}\"", file.display());
    let t = h.menu(
        &strings(&[]),
        &[
            (PROMPT, "3"),
            (PROMPT, "2"),
            ("devcontainer.json path", a[1].to_str().unwrap()),
            (PROMPT, "1"),
            ("Workspace file or folder path", &quoted),
            (PROMPT, "1"),
            (PROMPT, "1"),
        ],
        0,
    );
    contains(&t, "Select a project folder");
    assert_eq!(last(&h).last().unwrap(), "codex");
    assert_eq!(
        h.resolver_calls()[0].as_array().unwrap().last().unwrap(),
        &json!(h.path(""))
    );
}
#[test]
fn workspace_back_and_invalid_path_never_launch_or_build() {
    let mut h = Harness::new();
    let a = h.provision(false);
    h.menu(
        &strings(&[]),
        &[
            (PROMPT, "3"),
            (PROMPT, "2"),
            ("devcontainer.json path", "/missing/devcontainer.json"),
            (PROMPT, "3"),
            (PROMPT, "2"),
            ("devcontainer.json path", a[1].to_str().unwrap()),
            (PROMPT, "q"),
            (PROMPT, "q"),
            (PROMPT, "q"),
        ],
        1,
    );
    assert!(h.resolver_calls().is_empty());
    h.no_exec();
}
#[test]
fn workspace_file_also_works_as_explicit_host_workspace() {
    let mut h = Harness::new();
    let mut a = h.provision(true);
    let file = h.path("project.code-workspace");
    fs::write(&file, "{\"folders\":[{\"path\":\".\"}]}").unwrap();
    *a.last_mut().unwrap() = file.into();
    let r = h.run_os(&append(&a, &["inspect"]), b"", 0);
    assert_eq!(
        serde_json::from_str::<Value>(&r.out).unwrap()["host_workspace"],
        json!(h.path(""))
    );
}

#[test]
fn config_browser_detects_current_directory_layouts_without_pasting() {
    for relative in [
        "devcontainer.json",
        ".devcontainer.json",
        ".devcontainer/devcontainer.json",
        ".devcontainer/rust/devcontainer.json",
    ] {
        let mut h = Harness::new();
        h.provision(true);
        let config = h.path(relative);
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        if relative != "devcontainer.json" {
            fs::rename(h.path("devcontainer.json"), &config).unwrap();
        }
        h.edit_fixture(|v| v[0]["Config"]["Labels"]["devcontainer.config_file"] = json!(config));
        fs::write(
            h.path("project.code-workspace"),
            r#"{"folders":[{"path":"."}]}"#,
        )
        .unwrap();
        let t = h.menu(&strings(&[]), &answers(&["3", "", "1", "1"]), 0);
        contains(&t, "Select a devcontainer config");
        contains(&t, relative);
        assert!(!t.contains(h.dir.path().to_str().unwrap()));
        assert!(
            !t.contains("(Esc, empty Enter or q goes back)"),
            "unexpected path entry: {t}"
        );
        assert_eq!(last(&h).last().unwrap(), "codex");
        let calls = h.resolver_calls();
        assert!(calls.iter().all(|c| c[0] == "read-configuration"));
        assert!(calls[0].as_array().unwrap().contains(&json!(config)));
    }
}

#[test]
fn multiple_local_configs_require_selection_and_allow_back() {
    let h = Harness::new();
    for profile in ["one", "two"] {
        fs::create_dir_all(h.path(&format!(".devcontainer/{profile}"))).unwrap();
        fs::write(
            h.path(&format!(".devcontainer/{profile}/devcontainer.json")),
            "{}",
        )
        .unwrap();
    }
    let t = h.menu(&strings(&[]), &answers(&["3", "q", "q"]), 1);
    contains(&t, ".devcontainer/one/devcontainer.json");
    contains(&t, ".devcontainer/two/devcontainer.json");
    h.no_exec();
    assert!(h.resolver_calls().is_empty());
}

#[test]
fn escape_from_config_path_returns_immediately_and_restores_terminal() {
    for plain in [false, true] {
        for detected in [false, true] {
            let mut h = Harness::new();
            if detected {
                h.provision(false);
            }
            if !plain {
                h.env.remove(std::ffi::OsStr::new("HATCH_PLAIN"));
                h.set("TERM", "xterm-256color");
            }
            let mut p = h.pty(&strings(&[]));
            p.expect(if plain { PROMPT } else { "Select a container" });
            p.send(if plain { b"3\n" } else { b"\x1b[B\x1b[B\r" });
            if detected {
                p.expect(if plain {
                    PROMPT
                } else {
                    "Select a devcontainer config"
                });
                p.send(if plain { b"2\n" } else { b"\x1b[B\r" });
            }
            p.expect("devcontainer.json path");
            p.send(b"/partial/path");
            p.send(b"\x1b"); // No Enter: this used to be ignored by read_line.
            if detected {
                p.expect(if plain {
                    PROMPT
                } else {
                    "Select a devcontainer config"
                });
                p.send(b"\x1b");
            }
            p.expect(if plain { PROMPT } else { "Select a container" });
            p.send(if plain { b"q\n" } else { b"\x1b" });
            let t = p.finish(1);
            assert!(!t.contains("Choose an existing devcontainer"));
            h.no_exec();
            assert!(h.resolver_calls().is_empty());
        }
    }
}

#[test]
fn escape_from_workspace_path_returns_to_workspace_selection() {
    let mut h = Harness::new();
    h.provision(false);
    let mut p = h.pty(&strings(&[]));
    for answer in [b"3\n", b"1\n", b"1\n"] {
        p.expect(PROMPT);
        p.send(answer);
    }
    p.expect("Workspace file or folder path");
    p.send(b"/unfinished\x1b");
    p.expect("Select a workspace");
    p.expect(PROMPT);
    p.send(b"\x1b");
    p.expect("Select a devcontainer config");
    p.expect(PROMPT);
    p.send(b"\x1b");
    p.expect("Select a container");
    p.expect(PROMPT);
    p.send(b"q\n");
    p.finish(1);
    h.no_exec();
    assert!(h.resolver_calls().is_empty());
}

#[test]
fn workspace_menu_detects_vscode_file_in_current_directory_for_external_config() {
    let mut h = Harness::new();
    h.provision(true);
    fs::create_dir(h.path("external config")).unwrap();
    let config = h.path("external config/devcontainer.json");
    fs::rename(h.path("devcontainer.json"), &config).unwrap();
    h.edit_fixture(|v| v[0]["Config"]["Labels"]["devcontainer.config_file"] = json!(config));
    fs::write(
        h.path("My Project.code-workspace"),
        r#"{"folders":[{"path":"."}]}"#,
    )
    .unwrap();
    let t = h.menu(
        &strings(&["--config", config.to_str().unwrap()]),
        &answers(&["1", "1"]),
        0,
    );
    contains(&t, "My Project.code-workspace");
    assert_eq!(
        t.split("Workspace selected")
            .next()
            .unwrap()
            .matches("My Project.code-workspace")
            .count(),
        1
    );
    assert_eq!(last(&h).last().unwrap(), "codex");
    let calls = h.resolver_calls();
    assert!(calls.iter().all(|c| c[0] == "read-configuration"));
    assert_eq!(
        calls[0].as_array().unwrap().last().unwrap(),
        &json!(h.path(""))
    );
}

#[test]
fn workspace_menu_deduplicates_files_found_from_multiple_locations() {
    let mut h = Harness::new();
    h.provision(true);
    fs::create_dir(h.path(".devcontainer")).unwrap();
    let config = h.path(".devcontainer/devcontainer.json");
    fs::rename(h.path("devcontainer.json"), &config).unwrap();
    h.edit_fixture(|v| v[0]["Config"]["Labels"]["devcontainer.config_file"] = json!(config));
    fs::write(
        h.path("project.code-workspace"),
        r#"{"folders":[{"path":"."}]}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(
        h.path("project.code-workspace"),
        h.path(".devcontainer/alias.code-workspace"),
    )
    .unwrap();
    let t = h.menu(
        &strings(&["--config", config.to_str().unwrap()]),
        &answers(&["1", "1"]),
        0,
    );
    assert_eq!(
        t.split("Workspace selected")
            .next()
            .unwrap()
            .matches("project.code-workspace")
            .count(),
        1
    );
    contains(&t, "Detected project folder");
    contains(&t, "Paste a .code-workspace file or folder path");
    assert_eq!(last(&h).last().unwrap(), "codex");
}

#[test]
fn inspect_selects_container_even_when_single_and_never_launches() {
    let mut h = Harness::new();
    h.set("HATCH_TEST_MODE", "single");
    let t = h.menu(&strings(&["inspect"]), &answers(&["1"]), 0);
    assert_eq!(t.matches("Select a container").count(), 1);
    contains(
        &t,
        "Choose a container to review its workspace and available agents.",
    );
    contains(&t, "Agents in alpha-dev");
    assert!(!t.contains("Select an agent"));
    assert!(!t.contains("antigravity"));
    for name in ["codex", "claude", "copilot", "opencode", "cursor", "grok"] {
        contains(&t, &format!("--agent {name}"));
    }
    let t = h.menu(&strings(&["inspect", "--container", "alpha-dev"]), &[], 0);
    assert!(!t.contains("Select a container"));
    contains(&t, "Inspect container");
    h.run(&["inspect", "--no-input"], 1);
    h.no_launch();
    h.no_install();
}

#[test]
fn inspect_json_identifies_target_and_reports_availability_and_paths() {
    let h = Harness::new();
    let result = h.run(&["inspect", "--container", "beta-compose", "--json"], 0);
    let value: Value = serde_json::from_str(&result.out).unwrap();
    assert_eq!(value["container_name"], "beta-compose");
    assert_eq!(value["user"], "1001:1001");
    assert_eq!(
        value["agents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["agent"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!("codex"),
            json!("claude"),
            json!("copilot"),
            json!("opencode"),
            json!("cursor"),
            json!("grok")
        ]
    );
    for agent in value["agents"].as_array().unwrap() {
        assert_eq!(agent["available"], true);
        assert!(agent["executable"].is_string());
        assert!(agent.get("executable_on_persistent_mount").is_none());
    }
    assert!(h
        .calls()
        .iter()
        .filter(|c| c.args[0] == "exec")
        .all(|c| c.has(&"b".repeat(64))));
    h.no_launch();
    h.no_install();
}

#[test]
fn inspect_reports_executables_without_mount_coverage_or_mutations() {
    for mounted in [false, true] {
        let mut h = Harness::new();
        h.set("HATCH_TEST_MODE", "local-agent");
        if mounted {
            h.edit_fixture(|v| v[0]["Mounts"].as_array_mut().unwrap().push(json!({"Type":"volume", "Source":"/daemon/volume", "Destination":"/home/dev/.local"})));
        }
        let result = h.run(&["inspect", "--container", "alpha-dev", "--json"], 0);
        let value: Value = serde_json::from_str(&result.out).unwrap();
        assert_eq!(value["container_id"], "a".repeat(64));
        assert_eq!(
            value["agents"][0]["executable"],
            "/home/dev/.local/bin/codex"
        );
        assert!(value["agents"][0]
            .get("executable_on_persistent_mount")
            .is_none());
        assert!(value["agents"][0].get("authentication").is_none());
        assert!(h
            .calls()
            .iter()
            .filter(|c| c.args[0] == "exec")
            .all(|c| c.has("hatch-locate") || c.args.last().unwrap() == "--version"));
        h.no_launch();
        h.no_install();
    }
}

#[test]
fn inspect_rejects_launch_flags_and_agent_tail_before_docker() {
    let h = Harness::new();
    h.run(&["inspect", "--install"], 1);
    h.run(&["inspect", "--herdr"], 1);
    h.run(&["inspect", "--", "login"], 1);
    assert!(h.calls().is_empty());
}

#[test]
fn inspect_streams_results_while_another_probe_is_still_running() {
    for mode in ["", "missing-agent"] {
        let mut h = Harness::new();
        let gate = h.path("release-codex");
        h.set("HATCH_TEST_PROBE_GATE", &gate);
        h.set("HATCH_TEST_MODE", mode);
        let mut p = h.pty(&strings(&["inspect", "--container", "alpha-dev"]));
        p.expect("Results appear as they finish.");
        p.expect("--agent grok");
        // Codex's version check is still waiting. Serial scans cannot get here.
        assert!(!gate.exists());
        assert!(!p.output.contains("--agent codex"));
        fs::write(&gate, "continue").unwrap();
        p.expect("--agent codex");
        let output = p.finish(0);
        let expected = if mode.is_empty() {
            "[ok] Available"
        } else {
            "[x] Missing/unusable"
        };
        assert_eq!(output.matches(expected).count(), 6);
        h.no_launch();
        h.no_install();
    }
}

#[test]
fn new_from_config_selects_agent_before_starting_container() {
    let mut h = Harness::new();
    h.provision(false);
    fs::write(
        h.path("project.code-workspace"),
        r#"{"folders":[{"path":"."}]}"#,
    )
    .unwrap();
    let mut p = h.pty(&[]);
    for answer in [b"3\n", b"1\n", b"1\n"] {
        p.expect(PROMPT);
        p.send(answer);
    }
    p.expect("Select an agent");
    p.expect(PROMPT);
    assert!(h.resolver_calls().is_empty());
    h.no_exec();
    p.send(b"2\n");
    p.expect("Create this devcontainer?");
    p.expect(PROMPT);
    contains(&p.output, "Ready to create\r\n  Config:");
    contains(&p.output, "Agent:             Claude");
    assert!(!p.output.contains("No running container matches"));
    assert!(h.resolver_calls().is_empty());
    p.send(b"2\n");
    let t = p.finish_with_review(0);
    assert_eq!(t.matches("Select an agent").count(), 1);
    assert_eq!(h.resolver_calls()[0][0], "up");
    assert_eq!(last(&h).last().unwrap(), "claude");
}

#[test]
fn new_from_config_cancel_agent_never_starts_container() {
    let mut h = Harness::new();
    h.provision(false);
    fs::write(
        h.path("project.code-workspace"),
        r#"{"folders":[{"path":"."}]}"#,
    )
    .unwrap();
    h.menu(&[], &answers(&["3", "1", "1", "q", "q", "q", "q"]), 1);
    assert!(h.resolver_calls().is_empty());
    h.no_exec();
}

fn standard_workspace_fixture(h: &mut Harness) -> std::path::PathBuf {
    h.provision(true);
    fs::create_dir(h.path(".devcontainer")).unwrap();
    let config = h.path(".devcontainer/devcontainer.json");
    fs::rename(h.path("devcontainer.json"), &config).unwrap();
    h.edit_fixture(|v| v[0]["Config"]["Labels"]["devcontainer.config_file"] = json!(config));
    config
}

#[test]
fn empty_workspace_selects_detected_project_without_rewriting_or_looping() {
    for contents in ["", "{}", "{\"folders\": []}"] {
        let mut h = Harness::new();
        let config = standard_workspace_fixture(&mut h);
        let file = h.path("project.code-workspace");
        fs::write(&file, contents).unwrap();
        let t = h.menu(
            &strings(&["--config", config.to_str().unwrap()]),
            &answers(&["1", "2"]),
            0,
        );
        contains(&t, "project.code-workspace");
        assert!(!t.contains("existing, empty"));
        assert_eq!(t.matches("Select a workspace").count(), 1);
        assert!(!t.contains("Workspace file has no folders"));
        assert_eq!(fs::read_to_string(file).unwrap(), contents);
        assert_eq!(last(&h).last().unwrap(), "claude");
    }
}

#[test]
fn workspace_creation_is_explicit_and_leaves_empty_file() {
    let mut h = Harness::new();
    let config = standard_workspace_fixture(&mut h);
    let file = h.path(&format!(
        "{}.code-workspace",
        h.dir.path().file_name().unwrap().to_str().unwrap()
    ));
    let args = strings(&["--config", config.to_str().unwrap()]);
    let t = h.menu(&args, &answers(&["q"]), 1);
    contains(&t, "Create ");
    assert!(!file.exists());
    h.no_exec();
    let t = h.menu(&args, &answers(&["2", "1"]), 0);
    contains(&t, "Workspace created");
    assert_eq!(read_json(file), json!({"folders": []}));
    assert_eq!(last(&h).last().unwrap(), "codex");
    assert!(h
        .resolver_calls()
        .iter()
        .all(|c| c[0] == "read-configuration"));
}

#[test]
fn herdr_current_workspace_choice_follows_up_and_creates_only_a_fresh_tab() {
    let mut h = Harness::new();
    let args = h.provision(false);
    h.herdr();
    h.edit_fixture(|v| v[0]["Mounts"][0]["Destination"] = json!("/custom/project"));
    h.set("HERDR_WORKSPACE_ID", "current-workspace");
    write_json(
        h.path("herdr.log.state"),
        &json!({"workspaces":[{"workspace_id":"current-workspace"}],"tabs":0}),
    );
    let mut p = h.pty(&append(
        &args,
        &["up", "--herdr-workspace", "current-workspace"],
    ));
    p.expect("Select an agent");
    p.expect(PROMPT);
    p.send(b"2\n");
    p.expect("Launch this session?");
    p.expect(PROMPT);
    assert!(h
        .herdr_calls()
        .iter()
        .all(|c| *c == json!(["workspace", "list"])));
    assert_eq!(h.resolver_calls()[0][0], "up");
    h.no_launch();
    let t = p.finish_with_review(0);
    assert_eq!(t.matches("Open agent in Herdr").count(), 0);
    assert!(!t.contains("Startup runs this project's build"));
    let calls = h.herdr_calls();
    assert!(!calls
        .iter()
        .any(|c| c[0] == "workspace" && c[1] == "create"));
    let tabs = calls
        .iter()
        .filter(|c| c[0] == "tab" && c[1] == "create")
        .collect::<Vec<_>>();
    assert_eq!(tabs.len(), 1);
    assert!(tabs[0]
        .as_array()
        .unwrap()
        .contains(&json!("current-workspace")));
    let run = calls
        .iter()
        .find(|c| c[0] == "pane" && c[1] == "run")
        .unwrap();
    assert_eq!(run[2], "current-workspace:p2");
    assert_eq!(h.resolver_calls()[0][0], "up");
    h.no_launch();
}

#[test]
fn default_herdr_review_cancellation_never_creates_terminals() {
    for choice in ["q", "1"] {
        let mut h = Harness::new();
        h.herdr();
        h.menu(
            &strings(&["--container", "alpha-dev", "--agent", "codex"]),
            &answers(&[choice]),
            if choice == "q" { 1 } else { 0 },
        );
        assert!(h
            .herdr_calls()
            .iter()
            .all(|c| *c == json!(["workspace", "list"])));
        if choice == "q" {
            h.no_launch();
        } else {
            assert_eq!(last(&h).last().unwrap(), "codex");
        }
    }
}

fn core_fixture(executable: &str) -> Harness {
    let mut h = Harness::new();
    h.herdr(); // Also install legacy Herdr: Core must take precedence.
    h.tool(executable);
    h.set("HC_TERMINAL_ID", "t9");
    h.set("HATCH_TEST_CORE_LOG", h.path("core.log"));
    h
}

#[test]
fn core_detects_desktop_and_forwards_exact_arguments_after_review() {
    for executable in ["hc", "herdr-core"] {
        let mut h = core_fixture(executable);
        h.set("HERDR_ENV", "");
        let args = strings(&[
            "--container",
            "alpha-dev",
            "--agent",
            "claude",
            "--herdr",
            "--",
            "",
            "a b'\";$(false)",
        ]);
        let mut p = h.pty(&args);
        p.expect("CLI equivalent");
        assert!(!rows(h.path("core.log"))
            .iter()
            .any(|c| c[0] == "run" && c[1] != "--help"));
        let output = p.finish_with_review(0);
        contains(&output, "New Core session (open here)");
        contains(&output, "terminal t10 (detached)");
        contains(
            &output,
            &format!("Open this session: {executable} attach t10"),
        );
        contains(&output, "fixture Core attached");
        let calls = rows(h.path("core.log"));
        assert_eq!(calls.iter().filter(|c| c[0] == "attach").count(), 1);
        let run = calls
            .iter()
            .find(|c| c[0] == "run" && c[1] != "--help")
            .unwrap()
            .as_array()
            .unwrap();
        let args = run.iter().map(|v| v.as_str().unwrap()).collect::<Vec<_>>();
        assert!(args.contains(&"--bg"));
        assert!(args.contains(&"--no-parent"));
        assert!(args.contains(&"--no-notify"));
        assert!(args.contains(&"--here"));
        assert!(args.contains(&"--no-input"));
        assert!(!args.contains(&"--install"));
        assert_eq!(&args[args.len() - 2..], &["", "a b'\";$(false)"]);
        assert!(h.herdr_calls().is_empty());
        assert!(!h
            .calls()
            .iter()
            .any(|c| c.args.starts_with(&strings(&["exec", "-i"]))));
    }
}

#[test]
fn core_here_choice_and_explicit_here_launch_locally() {
    for here in [false, true] {
        let h = core_fixture("hc");
        let mut args = strings(&["--container", "alpha-dev", "--agent", "codex"]);
        if here {
            args.push("--here".into());
        }
        h.menu(&args, &[], 0);
        assert!(!rows(h.path("core.log"))
            .iter()
            .any(|c| c[0] == "run" && c[1] != "--help"));
        assert!(h.herdr_calls().is_empty());
        assert_eq!(last(&h).last().unwrap(), "codex");
    }
}

#[test]
fn core_failure_never_retries_or_launches_locally() {
    for mode in ["failure", "malformed", "finished"] {
        let mut h = core_fixture("hc");
        h.set("HATCH_TEST_CORE_MODE", mode);
        h.run(&["--container", "alpha-dev", "--herdr", "--no-input"], 1);
        assert_eq!(
            rows(h.path("core.log"))
                .iter()
                .filter(|c| c[0] == "run" && c[1] != "--help")
                .count(),
            1
        );
        assert!(h.herdr_calls().is_empty());
        assert!(!h
            .calls()
            .iter()
            .any(|c| c.args.starts_with(&strings(&["exec", "-i"]))));
    }
}

#[test]
fn core_rejects_workspace_and_non_utf8_before_creating_terminal() {
    let h = core_fixture("hc");
    h.run(
        &[
            "--container",
            "alpha-dev",
            "--herdr-workspace",
            "w1",
            "--no-input",
        ],
        1,
    );
    assert!(!rows(h.path("core.log"))
        .iter()
        .any(|c| c[0] == "run" && c[1] != "--help"));
    let mut args = strings(&["--container", "alpha-dev", "--herdr", "--no-input", "--"]);
    args.push(OsString::from_vec(vec![255]));
    h.run_os(&args, b"", 1);
    assert!(!rows(h.path("core.log"))
        .iter()
        .any(|c| c[0] == "run" && c[1] != "--help"));
    assert!(h.herdr_calls().is_empty());
}

#[test]
fn startup_pane_scrolls_and_restores_terminal_on_success_and_cancel() {
    for cancel in [false, true] {
        let mut h = Harness::new();
        let args = h.provision(false);
        h.env.remove(std::ffi::OsStr::new("HATCH_PLAIN"));
        h.set("TERM", "xterm-256color");
        let gate = h.path("release-startup");
        h.set("HATCH_TEST_UP_GATE", &gate);
        let mut p = h.pty(&append(
            &args,
            &["up", "--agent", "codex", "--workspace", "/custom/project"],
        ));
        p.resize(24, 100);
        p.expect("build-line-39");
        contains(
            &console::strip_ansi_codes(&p.output),
            "\r\n\r\n  Dev container logs",
        );
        contains(&p.output, "\x1b[38;5;245m");
        assert!(!p.output.contains("build-line-00"));
        p.send(b"kkkkkkkkk");
        p.expect("build-line-03");
        p.send(b"f");
        p.expect("build-line-39");
        if cancel {
            p.send(&[3]);
            contains(&p.finish(1), "dev container cancelled");
            h.no_exec();
        } else {
            fs::write(gate, "done").unwrap();
            p.finish_with_review(0);
        }
    }
}

#[test]
fn startup_failure_retains_private_complete_log() {
    use std::os::unix::fs::PermissionsExt;
    let mut h = Harness::new();
    let args = h.provision(false);
    h.set("TMPDIR", h.path(""));
    h.set("HATCH_TEST_UP_MODE", "error");
    let r = h.run_os(&append(&args, &["up", "--no-input"]), b"", 1);
    let logs: Vec<_> = fs::read_dir(h.path(""))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("hatch-devcontainer-")
        })
        .collect();
    assert_eq!(logs.len(), 1);
    contains(&r.err, &format!("Dev container log: {}", logs[0].display()));
    contains(
        &fs::read_to_string(&logs[0]).unwrap(),
        "fixture build failed",
    );
    assert_eq!(
        fs::metadata(&logs[0]).unwrap().permissions().mode() & 0o777,
        0o600
    );
    h.no_exec();
}

#[test]
fn startup_confirmation_distinguishes_stopped_from_absent() {
    for matching in [false, true] {
        let mut h = Harness::new();
        let args = h.provision(matching);
        h.edit_fixture(|v| v[0]["State"]["Running"] = json!(false));
        let mut p = h.pty(&append(&args, &["--agent", "codex"]));
        p.expect(if matching {
            "Start this devcontainer?"
        } else {
            "Create this devcontainer?"
        });
        p.expect(PROMPT);
        contains(
            &p.output,
            if matching {
                "Start container"
            } else {
                "Create container"
            },
        );
        assert!(!p.output.contains("and enter"));
        p.send(b"1\n");
        p.finish(1);
        assert!(h.resolver_calls().is_empty());
    }
}

#[test]
fn installation_pane_preserves_review_and_logs_both_streams() {
    for mode in ["needs-install", "install-fails"] {
        let mut h = Harness::new();
        h.env.remove(std::ffi::OsStr::new("HATCH_PLAIN"));
        h.set("TERM", "xterm-256color");
        h.set("TMPDIR", h.path(""));
        h.set("HATCH_TEST_MODE", mode);
        let gate = h.path("release-install");
        h.set("HATCH_TEST_INSTALL_GATE", &gate);
        let mut p = h.pty(&strings(&[
            "--container",
            "alpha-dev",
            "--agent",
            "codex",
            "--workspace",
            "/workspaces/project",
            "--here",
        ]));
        p.resize(40, 100);
        p.expect("Install Agent & Launch");
        assert!(!h
            .calls()
            .iter()
            .any(|c| c.has("hatch-install") && c.args.last().unwrap() == "install"));
        p.send(b"\r");
        p.expect("Your session");
        p.expect("install-stderr-39");
        assert!(!p.output.contains("install-stdout-00"));
        p.send(b"kkkkkkkkkkkkkkkkkkkk");
        p.expect("install-stdout-00");
        fs::write(gate, "done").unwrap();
        p.expect("Your session");
        p.expect("CLI equivalent");
        let result = p.finish(if mode == "needs-install" { 0 } else { 1 });
        if mode == "install-fails" {
            contains(&result, "Installation log:");
            h.no_launch();
        }
        let log = fs::read_dir(h.path(""))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("hatch-install-")
            })
            .unwrap();
        let contents = fs::read_to_string(log).unwrap();
        contains(&contents, "install-stdout-00");
        contains(&contents, "install-stderr-39");
    }
}

#[test]
fn rebuild_is_explicit_scoped_and_preserves_cache_by_default() {
    for stopped in [false, true] {
        for no_cache in [false, true] {
            let mut h = Harness::new();
            let args = h.provision(true);
            if stopped {
                h.edit_fixture(|v| v[0]["State"]["Running"] = json!(false));
            }
            let mut args = append(&args, &["rebuild", "--agent", "claude", "--no-input"]);
            if no_cache {
                args.push("--no-cache".into());
            }
            h.run_os(&args, b"", 0);
            let calls = h.resolver_calls();
            let up = calls[0].as_array().unwrap();
            assert_eq!(up[0], "up");
            assert!(up.contains(&json!("--remove-existing-container")));
            assert!(up.contains(&json!("--expect-existing-container")));
            assert_eq!(up.contains(&json!("--build-no-cache")), no_cache);
            assert_eq!(calls.iter().filter(|c| c[0] == "up").count(), 1);
            assert!(!h.calls().iter().any(|c| c.args[0] == "rm"));
            assert_eq!(last(&h).last().unwrap(), "claude");
        }
    }
}

#[test]
fn rebuild_refuses_missing_or_ambiguous_identity_and_invalid_options() {
    let mut h = Harness::new();
    let args = h.provision(false);
    h.run(&["rebuild"], 1);
    h.run_os(
        &append(&args, &["rebuild", "--container", "alpha-dev"]),
        b"",
        1,
    );
    h.run_os(&append(&args, &["up", "--no-cache"]), b"", 2);
    h.run_os(&append(&args, &["rebuild", "--no-input"]), b"", 1);
    h.edit_fixture(|v| {
        v[0]["Config"]["Labels"]["devcontainer.config_file"] = json!(args[1].to_str().unwrap());
        v[0]["Config"]["Labels"]["devcontainer.local_folder"] = json!(args[3].to_str().unwrap());
    });
    h.edit_fixture(|v| v[1]["Config"]["Labels"] = v[0]["Config"]["Labels"].clone());
    h.run_os(&append(&args, &["rebuild", "--no-input"]), b"", 1);
    assert!(h.resolver_calls().is_empty());
    h.no_exec();
}

#[test]
fn rebuild_menu_selects_target_agent_then_confirms_before_mutation() {
    for confirm in [false, true] {
        let mut h = Harness::new();
        h.provision(true);
        h.set("TMPDIR", h.path(""));
        let mut p = h.pty(&[]);
        p.expect("Rebuild a devcontainer");
        p.expect(PROMPT);
        p.send(b"4\n");
        p.expect("Select a container to rebuild");
        p.expect(PROMPT);
        p.send(b"1\n");
        p.expect("Select an agent");
        p.expect(PROMPT);
        assert!(h.resolver_calls().is_empty());
        p.send(b"2\n");
        p.expect("Rebuild this devcontainer?");
        p.expect(PROMPT);
        contains(&p.output, "deletes files outside persistent mounts");
        assert!(
            p.output.find("Dev container log:").unwrap()
                < p.output.find("Rebuild stops sessions").unwrap()
        );
        assert!(h.resolver_calls().is_empty());
        p.send(if confirm { b"2\n" } else { b"1\n" });
        if confirm {
            p.finish_with_review(0);
            assert_eq!(
                h.resolver_calls().iter().filter(|c| c[0] == "up").count(),
                1
            );
            assert_eq!(last(&h).last().unwrap(), "claude");
        } else {
            p.expect("Select a container");
            p.expect(PROMPT);
            p.send(b"q\n");
            p.finish(1);
            assert!(h.resolver_calls().is_empty());
            assert!(!fs::read_dir(h.path("")).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("hatch-devcontainer-")));
            h.no_exec();
        }
    }
}

#[test]
fn core_unavailable_capabilities_are_resolved_before_review() {
    for mode in ["unavailable", "old-cli"] {
        let mut h = core_fixture("hc");
        h.set("HATCH_TEST_CORE_MODE", mode);
        let mut p = h.pty(&strings(&[
            "--container",
            "alpha-dev",
            "--agent",
            "codex",
            "--herdr",
        ]));
        p.expect("Launch this session?");
        assert!(!p.output.contains("Herdr:"));
        assert!(!p.output.contains("Open agent in Herdr"));
        assert!(!p.output.contains("New Core terminal"));
        p.finish_with_review(0);
        assert!(!rows(h.path("core.log"))
            .iter()
            .any(|c| c[0] == "run" && c[1] != "--help"));
        assert!(h.herdr_calls().is_empty());
        assert_eq!(last(&h).last().unwrap(), "codex");
    }
}

#[test]
fn rebuild_back_after_completion_does_not_repeat_replacement() {
    let mut h = Harness::new();
    let args = h.provision(true);
    let mut p = h.pty(&append(&args, &["rebuild"]));
    p.expect("Select an agent");
    p.expect(PROMPT);
    p.send(b"2\n");
    p.expect("Rebuild this devcontainer?");
    p.expect(PROMPT);
    p.send(b"2\n");
    p.expect("Launch this session?");
    p.expect(PROMPT);
    p.send(b"q\n");
    p.expect("Select an agent");
    p.expect(PROMPT);
    p.send(b"1\n");
    p.finish_with_review(0);
    assert_eq!(
        h.resolver_calls().iter().filter(|c| c[0] == "up").count(),
        1
    );
}

#[test]
fn rebuild_handoff_does_not_repeat_rebuild_in_child() {
    let mut h = core_fixture("hc");
    let args = h.provision(true);
    h.edit_fixture(|v| v[0]["Mounts"][0]["Destination"] = json!("/custom/project"));
    h.run_os(
        &append(&args, &["rebuild", "--herdr", "--no-input"]),
        b"",
        0,
    );
    let calls = rows(h.path("core.log"));
    let run = calls
        .iter()
        .find(|c| c[0] == "run" && c[1] != "--help")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(!run.contains(&json!("rebuild")));
    assert!(!run.contains(&json!("--remove-existing-container")));
    assert!(run.contains(&json!("--here")));
    assert!(run.contains(&json!("--no-input")));
}

#[test]
fn stale_legacy_workspace_does_not_affect_default_local_launch() {
    let mut h = Harness::new();
    h.herdr();
    h.set("HERDR_WORKSPACE_ID", "gone");
    let output = h.menu(
        &strings(&["--container", "alpha-dev", "--agent", "codex"]),
        &[],
        0,
    );
    assert!(!output.contains("Open agent in Herdr"));
    assert!(h.herdr_calls().is_empty());
    assert_eq!(last(&h).last().unwrap(), "codex");
}

#[test]
fn core_failure_after_review_does_not_change_the_selected_destination() {
    let h = core_fixture("hc");
    let mut p = h.pty(&strings(&[
        "--container",
        "alpha-dev",
        "--agent",
        "codex",
        "--herdr",
    ]));
    p.expect("Launch this session?");
    p.expect(PROMPT);
    // Remove only the disposable CLI after capability detection, before launch.
    fs::remove_file(h.path("hc")).unwrap();
    h.tool("herdr-core");
    p.send(b"1\n");
    p.finish(1);
    assert!(h.herdr_calls().is_empty());
    assert!(!rows(h.path("core.log"))
        .iter()
        .any(|c| c[0] == "run" && c[1] != "--help"));
    h.no_launch();
}

#[test]
fn rebuild_failure_keeps_log_and_never_retries_or_launches() {
    let mut h = Harness::new();
    let args = h.provision(true);
    h.run_os(&append(&args, &["rebuild"]), b"", 1);
    assert!(h.resolver_calls().is_empty());
    h.set("HATCH_TEST_UP_MODE", "error");
    let output = h.run_os(&append(&args, &["rebuild", "--no-input"]), b"", 1);
    contains(&output.err, "Dev container log:");
    assert_eq!(
        h.resolver_calls().iter().filter(|c| c[0] == "up").count(),
        1
    );
    h.no_exec();
}

#[test]
fn herdr_child_uses_parent_tool_path_instead_of_terminal_server_path() {
    for core in [false, true] {
        let mut h = if core {
            core_fixture("hc")
        } else {
            let mut h = Harness::new();
            h.herdr();
            h
        };
        let args = h.provision(true);
        h.edit_fixture(|v| v[0]["Mounts"][0]["Destination"] = json!("/custom/project"));
        // Relative PATH entries must keep their meaning after the terminal cwd changes.
        h.set("PATH", ".");
        let args = if core {
            append(&args, &["--devcontainer-cli", "./devcontainer"])
        } else {
            args
        };
        h.run_os(
            &append(
                &args,
                &["--container", "alpha-dev", "--herdr", "--no-input"],
            ),
            b"",
            0,
        );
        let child: Vec<String> = if core {
            let calls = rows(h.path("core.log"));
            let run = calls
                .iter()
                .find(|c| c[0] == "run" && c[1] != "--help")
                .unwrap()
                .as_array()
                .unwrap();
            let delimiter = run.iter().position(|arg| arg == "--").unwrap();
            run[delimiter + 1..]
                .iter()
                .map(|arg| arg.as_str().unwrap().to_owned())
                .collect()
        } else {
            parse_handoff(&h, &handoff(&h))
        };
        fs::create_dir(h.path("server-bin")).unwrap();
        fs::create_dir(h.path("other-cwd")).unwrap();
        // Run the actual child argv using only disposable Docker/resolver fixtures.
        // The stale server PATH intentionally contains no host tools.
        let mut command = h.command("/usr/bin/env");
        command
            .args(&child[1..])
            .env("PATH", h.path("server-bin"))
            .current_dir(h.path("other-cwd"));
        h.execute(&mut command, b"", 0);
        assert!(
            h.resolver_calls()
                .iter()
                .filter(|c| c[0] == "read-configuration")
                .count()
                >= 2
        );
        assert_eq!(last(&h).last().unwrap(), "codex");
    }
}
