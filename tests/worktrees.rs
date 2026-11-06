//! All Git mutations are confined to disposable directories, never real checkouts.
mod support;
use serde_json::json;
use std::{fs, path::Path, process::Command};
use support::*;

fn git_binary() -> std::path::PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join("git"))
        .find(|p| p.is_file())
        .unwrap()
}
fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new(git_binary())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .current_dir(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn repo() -> Harness {
    let mut h = Harness::new();
    h.set("HATCH_TEST_WORKTREE_GIT", git_binary());
    fs::create_dir(h.path("repo")).unwrap();
    let root = h.path("repo");
    git(&root, &["init", "--initial-branch=main"]);
    git(&root, &["config", "user.name", "Fixture"]);
    git(&root, &["config", "user.email", "fixture@example.test"]);
    fs::write(root.join("tracked"), "initial\n").unwrap();
    git(&root, &["add", "tracked"]);
    git(&root, &["commit", "-m", "initial"]);
    std::os::unix::fs::symlink(git_binary(), h.path("git")).unwrap();
    h.edit_fixture(|v| {
        v[0]["Config"]["Labels"]["devcontainer.local_folder"] = json!(root);
        v[0]["Config"]["WorkingDir"] = json!(root);
        v[0]["Mounts"] = json!([{"Type":"bind", "Source": h.path(""), "Destination": h.path("")}]);
    });
    h
}
fn base() -> Vec<std::ffi::OsString> {
    strings(&["--container", "alpha-dev", "--metadata-only", "--no-input"])
}
fn create(h: &Harness, name: &str) {
    h.run_os(
        &append(
            &base(),
            &[
                "worktree",
                "create",
                h.path(name).to_str().unwrap(),
                "--branch",
                name,
                "--from",
                "HEAD",
            ],
        ),
        b"",
        0,
    );
}
#[test]
fn worktree_lifecycle_and_readonly_json() {
    let h = repo();
    let destination = h.path("feature path");
    h.run_os(
        &append(
            &base(),
            &[
                "worktree",
                "create",
                destination.to_str().unwrap(),
                "--branch",
                "feature",
                "--from",
                "HEAD",
            ],
        ),
        b"",
        0,
    );
    assert_eq!(
        git(&destination, &["branch", "--show-current"]).trim(),
        "feature"
    );
    let links = fs::read_to_string(destination.join(".git")).unwrap();
    let version = git(&h.path("repo"), &["--version"]);
    let minor = version
        .trim()
        .split('.')
        .nth(1)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    if minor >= 48 {
        assert!(links.starts_with("gitdir: ../"), "{links}");
    }
    let report = h.run_os(&append(&base(), &["worktrees", "--json"]), b"", 0);
    let rows: serde_json::Value = serde_json::from_str(&report.out).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[1]["path"], json!(destination));
    h.no_launch();
    h.no_install();
    let tail = [
        "--worktree",
        destination.to_str().unwrap(),
        "--",
        "resume",
        "",
        "a b'\";$HOME",
    ];
    h.run_os(&append(&base(), &tail), b"", 0);
    let calls = h.calls();
    let call = calls.iter().find(|c| c.launches()).unwrap();
    assert_eq!(call.args.last().unwrap(), "a b'\";$HOME");
    let cwd = call.args.iter().position(|s| s == "--workdir").unwrap();
    assert_eq!(call.args[cwd + 1], destination.as_os_str());
    h.run_os(
        &append(
            &base(),
            &["worktree", "remove", destination.to_str().unwrap()],
        ),
        b"",
        1,
    );
    assert!(destination.is_dir());
    h.run_os(
        &append(
            &base(),
            &["worktree", "remove", destination.to_str().unwrap(), "--yes"],
        ),
        b"",
        0,
    );
    assert!(!destination.exists());
    git(
        &h.path("repo"),
        &["show-ref", "--verify", "refs/heads/feature"],
    );
}
#[test]
fn refusal_of_dirty_ignored_locked_main_and_unknown_worktrees() {
    let h = repo();
    create(&h, "feature");
    let path = h.path("feature");
    let remove = append(
        &base(),
        &["worktree", "remove", path.to_str().unwrap(), "--yes"],
    );
    for filename in ["tracked", "untracked"] {
        fs::write(path.join(filename), "changed").unwrap();
        h.run_os(&remove, b"", 1);
        assert!(path.is_dir());
        if filename == "tracked" {
            git(&path, &["restore", "tracked"]);
        } else {
            fs::remove_file(path.join(filename)).unwrap();
        }
    }
    fs::write(path.join(".gitignore"), "ignored\n").unwrap();
    git(&path, &["add", ".gitignore"]);
    git(&path, &["commit", "-m", "ignore"]);
    fs::write(path.join("ignored"), "keep me").unwrap();
    h.run_os(&remove, b"", 1);
    fs::remove_file(path.join("ignored")).unwrap();
    git(
        &h.path("repo"),
        &["worktree", "lock", path.to_str().unwrap()],
    );
    contains(&h.run_os(&remove, b"", 1).err, "locked");
    h.run_os(
        &append(
            &base(),
            &[
                "worktree",
                "remove",
                h.path("repo").to_str().unwrap(),
                "--yes",
            ],
        ),
        b"",
        1,
    );
    h.run_os(
        &append(&base(), &["--worktree", h.path("home").to_str().unwrap()]),
        b"",
        1,
    );
    h.no_launch();
}
#[test]
fn create_refuses_existing_paths_branches_bad_refs_and_ephemeral_storage() {
    let h = repo();
    for (path, branch, start) in [
        ("home", "fresh", "HEAD"),
        ("absent", "main", "HEAD"),
        ("badref", "fresh", "no-such-ref"),
        ("badbranch", "bad branch", "HEAD"),
    ] {
        h.run_os(
            &append(
                &base(),
                &[
                    "worktree",
                    "create",
                    h.path(path).to_str().unwrap(),
                    "--branch",
                    branch,
                    "--from",
                    start,
                ],
            ),
            b"",
            1,
        );
        assert!(!h.path(path).exists() || path == "home");
    }
    // An existing branch is attached, not silently detached or reset.
    git(&h.path("repo"), &["branch", "existing"]);
    h.run_os(
        &append(
            &base(),
            &[
                "worktree",
                "create",
                h.path("existing").to_str().unwrap(),
                "--branch",
                "existing",
            ],
        ),
        b"",
        0,
    );
    assert_eq!(
        git(&h.path("existing"), &["branch", "--show-current"]).trim(),
        "existing"
    );
    h.edit_fixture(|v| v[0]["Mounts"] = json!([]));
    let mut args = append(&base(), &["--workspace", h.path("repo").to_str().unwrap()]);
    args.extend(strings(&[
        "worktree",
        "create",
        h.path("ephemeral").to_str().unwrap(),
        "--branch",
        "new",
        "--from",
        "HEAD",
    ]));
    contains(&h.run_os(&args, b"", 1).err, "persistent");
    assert!(!h.path("ephemeral").exists());
}
#[test]
fn setup_failure_keeps_checkout_and_creates_no_terminal_and_skill_is_explicit() {
    let mut h = repo();
    create(&h, "feature");
    h.herdr();
    // Restore repo mapping after generic Herdr helper changes its labels.
    h.edit_fixture(|v| {
        v[0]["Config"]["Labels"]["devcontainer.local_folder"] = json!(h.path("repo"))
    });
    let args = append(
        &base(),
        &[
            "--worktree",
            h.path("feature").to_str().unwrap(),
            "--herdr",
            "--setup-command",
            "exit 7",
        ],
    );
    contains(&h.run_os(&args, b"", 1).err, "checkout retained");
    assert!(h.path("feature").is_dir());
    assert!(h.herdr_calls().is_empty());
    let skill = h.path("feature/init skill.md");
    fs::write(&skill, "Fixture initialization instructions").unwrap();
    h.run_os(
        &append(
            &base(),
            &[
                "--here",
                "--worktree",
                h.path("feature").to_str().unwrap(),
                "--init-skill",
                skill.to_str().unwrap(),
            ],
        ),
        b"",
        0,
    );
    let call = h.calls().into_iter().find(|c| c.launches()).unwrap();
    contains(
        call.args.last().unwrap().to_str().unwrap(),
        "Read the initialization skill",
    );
    assert!(!call.has("--init-skill"));
    for tail in [vec!["--agent", "grok"], vec!["--", "resume"]] {
        let mut args = append(&base(), &["--init-skill", skill.to_str().unwrap()]);
        args.extend(strings(&tail));
        h.run_os(&args, b"", 1);
    }
}
#[test]
fn escape_from_herdr_returns_through_checkout_to_agent() {
    let mut h = repo();
    h.herdr();
    h.edit_fixture(|v| {
        v[0]["Config"]["Labels"]["devcontainer.local_folder"] = json!(h.path("repo"))
    });
    let mut p = h.pty(&strings(&["--container", "alpha-dev", "--metadata-only"]));
    p.expect(PROMPT);
    p.send(b"1\r");
    p.expect("Choose a checkout / worktree");
    p.expect(PROMPT);
    p.send(b"1\r");
    p.expect("Open agent in Herdr");
    p.expect(PROMPT);
    p.send(b"\x1b");
    p.expect("Choose a checkout / worktree");
    p.expect(PROMPT);
    p.send(b"\x1b");
    p.expect("Select an agent");
    p.expect(PROMPT);
    p.send(b"\x1b");
    p.finish(1);
    h.no_launch();
    assert!(h.herdr_calls().is_empty());
}
#[test]
fn escape_returns_from_initialization_and_checkout_without_changing_workspace() {
    for plain in [false, true] {
        let mut h = repo();
        create(&h, "feature");
        if !plain {
            h.env.remove(std::ffi::OsStr::new("HATCH_PLAIN"));
        }
        h.set("TERM", "xterm-256color");
        h.set("NO_COLOR", "1");
        let mut p = h.pty(&strings(&[
            "--container",
            "alpha-dev",
            "--metadata-only",
            "--here",
        ]));
        p.expect(if plain { PROMPT } else { "Select an agent" });
        p.send(b"\r");
        p.expect(if plain {
            PROMPT
        } else {
            "Choose a checkout / worktree"
        });
        p.send(if plain { b"2\r" } else { b"\x1b[B\r" });
        p.expect(if plain {
            PROMPT
        } else {
            "Initialize this checkout?"
        });
        p.send(b"\x1b");
        p.expect(if plain {
            PROMPT
        } else {
            "Choose a checkout / worktree"
        });
        p.send(b"\x1b");
        p.expect(if plain { PROMPT } else { "Select an agent" });
        p.send(b"\r");
        p.expect(if plain {
            PROMPT
        } else {
            "Choose a checkout / worktree"
        });
        p.send(b"\r");
        let output = p.finish_with_review(0);
        assert!(!output.contains("Cancel launch"));
        contains(&output, "Path:");
        let call = h.calls().into_iter().find(|c| c.launches()).unwrap();
        assert!(call.args.contains(&h.path("repo").into_os_string()));
        assert!(!call.args.contains(&h.path("feature").into_os_string()));
    }
}
#[test]
fn guided_checkout_selection_cancel_and_creation_cancel_are_readonly() {
    let h = repo();
    create(&h, "feature");
    let args = strings(&[
        "--container",
        "alpha-dev",
        "--metadata-only",
        "--agent",
        "codex",
        "--here",
    ]);
    h.menu(
        &args,
        &[(PROMPT, "4"), (PROMPT, "1"), (PROMPT, "1"), (PROMPT, "5")],
        1,
    );
    assert!(h.path("feature").is_dir());
    h.no_launch();
}

#[test]
fn guided_skill_discovery_uses_main_clone_and_only_passes_a_prompt() {
    let h = repo();
    let skills = h.path("repo/.agents/skills/fixture-init");
    fs::create_dir_all(&skills).unwrap();
    fs::write(
        skills.join("SKILL.md"),
        "Fixture initialization instructions",
    )
    .unwrap();
    create(&h, "feature");
    let args = strings(&[
        "--container",
        "alpha-dev",
        "--metadata-only",
        "--agent",
        "codex",
        "--here",
    ]);
    h.menu(&args, &[(PROMPT, "2"), (PROMPT, "3"), (PROMPT, "1")], 0);
    let call = h.calls().into_iter().find(|c| c.launches()).unwrap();
    let prompt = call.args.last().unwrap().to_str().unwrap();
    contains(prompt, skills.join("SKILL.md").to_str().unwrap());
    contains(prompt, "existing-worktree initialization");
    assert!(!h.calls().iter().any(|c| c.has("hatch-project-setup")));
}
#[test]
fn guided_create_cancel_has_no_git_mutation_and_selects_existing_checkout() {
    let h = repo();
    let args = strings(&[
        "--container",
        "alpha-dev",
        "--metadata-only",
        "--agent",
        "codex",
        "--here",
    ]);
    let destination = h.path("cancelled");
    h.menu(
        &args,
        &[
            (PROMPT, "2"),
            (PROMPT, "2"),
            ("New branch name", "fresh"),
            ("Starting ref or commit", "HEAD"),
            ("Worktree destination", destination.to_str().unwrap()),
            (PROMPT, "1"),
            (PROMPT, "4"),
        ],
        1,
    );
    assert!(!destination.exists());
    assert!(!git(&h.path("repo"), &["branch", "--list", "fresh"]).contains("fresh"));
    h.no_launch();
    h.menu(&args, &[(PROMPT, "1")], 0);
}
#[test]
fn setup_runs_once_before_herdr_and_child_receives_only_the_final_launch() {
    let mut h = repo();
    create(&h, "feature");
    h.herdr();
    h.edit_fixture(|v| {
        v[0]["Config"]["Labels"]["devcontainer.local_folder"] = json!(h.path("repo"))
    });
    let script = "printf setup > setup-count";
    h.run_os(
        &append(
            &base(),
            &[
                "--worktree",
                h.path("feature").to_str().unwrap(),
                "--herdr",
                "--setup-command",
                script,
            ],
        ),
        b"",
        0,
    );
    assert_eq!(
        fs::read_to_string(h.path("feature/setup-count")).unwrap(),
        "setup"
    );
    assert_eq!(
        h.calls()
            .iter()
            .filter(|c| c.has("hatch-project-setup"))
            .count(),
        1
    );
    let calls = h.herdr_calls();
    let run = calls
        .iter()
        .find(|c| c[0] == "pane" && c[1] == "run")
        .unwrap();
    let command = run[3].as_str().unwrap();
    contains(command, h.path("feature").to_str().unwrap());
    assert!(!command.contains("--setup-command"));
    assert!(!command.contains(script));
    contains(command, "--here");
    contains(command, "--no-input");
}
#[test]
fn mount_topology_symlink_escape_and_broken_links_fail_before_launch() {
    let h = repo();
    let other = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(other.path(), h.path("escape")).unwrap();
    h.run_os(
        &append(
            &base(),
            &[
                "worktree",
                "create",
                h.path("escape/new").to_str().unwrap(),
                "--branch",
                "new",
                "--from",
                "HEAD",
            ],
        ),
        b"",
        1,
    );
    assert!(!other.path().join("new").exists());
    create(&h, "feature");
    fs::write(h.path("feature/.git"), "gitdir: /missing/git/metadata\n").unwrap();
    h.run_os(
        &append(
            &base(),
            &["--worktree", h.path("feature").to_str().unwrap()],
        ),
        b"",
        1,
    );
    h.no_launch();
    // Different mappings of sibling directories must not silently produce host-broken links.
    fs::create_dir(h.path("destination")).unwrap();
    h.edit_fixture(|v| v[0]["Mounts"].as_array_mut().unwrap().push(json!({"Type":"bind", "Source":"/unrelated/layout", "Destination":h.path("destination")})));
    contains(
        &h.run_os(
            &append(
                &base(),
                &[
                    "worktree",
                    "create",
                    h.path("destination/new").to_str().unwrap(),
                    "--branch",
                    "new",
                    "--from",
                    "HEAD",
                ],
            ),
            b"",
            1,
        )
        .err,
        "relative layouts",
    );
}
#[test]
fn container_volume_and_remote_endpoint_do_not_require_a_local_checkout() {
    let mut h = repo();
    h.set("DOCKER_HOST", "ssh://fixture-remote");
    h.edit_fixture(|v| {
        v[0]["Mounts"] =
            json!([{"Type":"volume", "Source":"fixture-volume", "Destination": h.path("")} ])
    });
    create(&h, "feature");
    h.run_os(
        &append(
            &base(),
            &["--worktree", h.path("feature").to_str().unwrap()],
        ),
        b"",
        0,
    );
    assert_eq!(
        git(&h.path("feature"), &["branch", "--show-current"]).trim(),
        "feature"
    );
}
#[test]
fn worktree_options_are_validated_before_docker_and_management_never_starts_container() {
    let h = repo();
    for args in [
        vec!["--worktree", "relative"],
        vec!["worktrees", "--install"],
        vec!["worktrees", "--", "resume"],
        vec!["inspect", "--init-skill", "/skill.md"],
        vec!["worktree", "remove", "relative", "--yes"],
    ] {
        h.run(&args, 1);
    }
    assert!(h.calls().is_empty());
    let mut h = h;
    let args = h.provision(false);
    h.run_os(
        &append(&args, &["worktrees", "--json", "--no-input"]),
        b"",
        1,
    );
    assert!(h.resolver_calls().is_empty());
    h.no_launch();
}

#[test]
fn nested_destinations_and_older_git_with_identical_paths_are_supported() {
    let mut h = repo();
    // Exercise the distro Git as well as whichever Git is first on developer PATH.
    h.set("HATCH_TEST_WORKTREE_GIT", "/usr/bin/git");
    let destination = h.path("worktrees/topic/nested");
    h.run_os(
        &append(
            &base(),
            &[
                "worktree",
                "create",
                destination.to_str().unwrap(),
                "--branch",
                "topic/nested",
                "--from",
                "HEAD",
            ],
        ),
        b"",
        0,
    );
    assert_eq!(
        git(&destination, &["branch", "--show-current"]).trim(),
        "topic/nested"
    );
}
#[test]
fn submodule_worktree_is_never_force_removed() {
    let h = repo();
    fs::create_dir(h.path("submodule-source")).unwrap();
    let source = h.path("submodule-source");
    git(&source, &["init", "--initial-branch=main"]);
    git(&source, &["config", "user.name", "Fixture"]);
    git(&source, &["config", "user.email", "fixture@example.test"]);
    fs::write(source.join("file"), "submodule").unwrap();
    git(&source, &["add", "file"]);
    git(&source, &["commit", "-m", "initial"]);
    git(
        &h.path("repo"),
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            source.to_str().unwrap(),
            "module",
        ],
    );
    git(&h.path("repo"), &["commit", "-m", "submodule"]);
    create(&h, "feature");
    git(
        &h.path("feature"),
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "update",
            "--init",
        ],
    );
    h.run_os(
        &append(
            &base(),
            &[
                "worktree",
                "remove",
                h.path("feature").to_str().unwrap(),
                "--yes",
            ],
        ),
        b"",
        1,
    );
    assert!(h.path("feature/module/file").is_file());
    assert!(!h.calls().iter().any(|c| c.has("--force")));
}
