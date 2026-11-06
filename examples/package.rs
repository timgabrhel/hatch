//! Native release packaging: cargo run --locked --example package.
use serde_json::Value;
use std::{
    env, fs, io,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
};
fn checked(command: &mut Command) -> io::Result<std::process::Output> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "{command:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(output)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::consts::OS != "linux" || !["x86_64", "aarch64"].contains(&env::consts::ARCH) {
        return Err("Packaging supports native Linux x86_64/aarch64".into());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    checked(Command::new(&cargo).current_dir(root).args([
        "build",
        "--release",
        "--locked",
        "--bin",
        "hatch",
    ]))?;
    let metadata: Value = serde_json::from_slice(
        &checked(Command::new(cargo).current_dir(root).args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
        ]))?
        .stdout,
    )?;
    let mut packages = metadata["packages"]
        .as_array()
        .ok_or("missing package metadata")?
        .clone();
    packages.sort_by_key(|p| {
        (
            p["name"].as_str().unwrap().to_owned(),
            p["version"].as_str().unwrap().to_owned(),
        )
    });
    let package = packages
        .iter()
        .find(|p| p["name"] == "hatch-cli")
        .ok_or("missing Hatch metadata")?;
    let name = format!(
        "hatch-{}-linux-{}",
        package["version"].as_str().unwrap(),
        env::consts::ARCH
    );
    let staging = tempfile::tempdir()?;
    let base = staging.path().join(&name);
    fs::create_dir(&base)?;
    let copy = |source: &Path, target: &Path, mode: u32| -> io::Result<()> {
        fs::create_dir_all(target.parent().unwrap())?;
        fs::copy(source, target)?;
        fs::set_permissions(target, fs::Permissions::from_mode(mode))
    };
    let binary = Path::new(metadata["target_directory"].as_str().unwrap()).join("release/hatch");
    copy(&binary, &base.join("hatch"), 0o755)?;
    for file in [
        "README.md",
        "DESIGN.md",
        "scripts/install.sh",
        "tooling/package.json",
        "tooling/package-lock.json",
    ] {
        copy(
            &root.join(file),
            &base.join(file),
            if file.ends_with(".sh") { 0o755 } else { 0o644 },
        )?;
    }
    let mut notices = String::new();
    for p in &packages {
        if p["name"] == "hatch-cli" {
            continue;
        }
        let n = p["name"].as_str().unwrap();
        let v = p["version"].as_str().unwrap();
        notices.push_str(&format!(
            "{n} {v}: {}\n",
            p["license"].as_str().unwrap_or("see source")
        ));
        let directory = Path::new(p["manifest_path"].as_str().unwrap())
            .parent()
            .unwrap();
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            let filename = path.file_name().unwrap();
            let upper = filename.to_string_lossy().to_uppercase();
            if path.is_file()
                && ["LICENSE", "COPYING", "NOTICE"]
                    .iter()
                    .any(|s| upper.starts_with(s))
            {
                copy(
                    &path,
                    &base
                        .join("licenses")
                        .join(format!("{n}-{v}"))
                        .join(filename),
                    0o644,
                )?;
            }
        }
    }
    fs::write(base.join("THIRD_PARTY_NOTICES.txt"), notices)?;
    fs::set_permissions(
        base.join("THIRD_PARTY_NOTICES.txt"),
        fs::Permissions::from_mode(0o644),
    )?;
    // GNU tar and gzip are packaging tools only. Fixed ownership, modes, ordering,
    // and timestamps make repeated archives reproducible for identical inputs.
    let epoch = env::var("SOURCE_DATE_EPOCH")
        .unwrap_or_else(|_| "0".into())
        .parse::<u64>()?;
    let tar = staging.path().join("archive.tar");
    checked(
        Command::new("tar")
            .args([
                "--sort=name",
                "--format=gnu",
                "--owner=0",
                "--group=0",
                "--numeric-owner",
                "--mode=u+rwX,go+rX,go-w",
            ])
            .arg(format!("--mtime=@{epoch}"))
            .arg("-cf")
            .arg(&tar)
            .arg("-C")
            .arg(staging.path())
            .arg(&name),
    )?;
    let dist = root.join("dist");
    fs::create_dir_all(&dist)?;
    let archive = dist.join(format!("{name}.tar.gz"));
    let mut temporary = tempfile::NamedTempFile::new_in(&dist)?;
    checked(
        Command::new("gzip")
            .args(["-n", "-c"])
            .arg(&tar)
            .stdout(Stdio::from(temporary.as_file_mut().try_clone()?)),
    )?;
    temporary.persist(&archive)?;
    let checksum = checked(
        Command::new("sha256sum")
            .current_dir(&dist)
            .arg(archive.file_name().unwrap()),
    )?;
    fs::write(dist.join(format!("{name}.tar.gz.sha256")), &checksum.stdout)?;
    println!(
        "{}\n{}",
        archive.display(),
        String::from_utf8_lossy(&checksum.stdout).trim()
    );
    Ok(())
}
