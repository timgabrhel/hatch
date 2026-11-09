use crate::{process::capture, prompt, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Container {
    pub id: String,
    pub name: String,
    pub state: State,
    pub config: Config,
    #[serde(default)]
    pub mounts: Vec<Mount>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct State {
    pub running: bool,
    #[serde(default)]
    pub paused: bool,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Config {
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub working_dir: String,
    pub labels: Option<BTreeMap<String, String>>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Mount {
    #[serde(rename = "Type")]
    pub kind: String,
    pub source: String,
    pub destination: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Context {
    pub configuration_source: String,
    pub config_file: Option<PathBuf>,
    pub host_workspace: Option<PathBuf>,
    pub user_env_probe: Option<String>,
    pub warnings: Vec<String>,
    pub remote_env_names: Vec<String>,
    #[serde(skip)]
    pub remote_env: BTreeMap<String, Option<String>>,
    pub container_id: String,
    pub container_name: String,
    pub user: String,
    pub user_source: String,
    pub workspace: String,
    pub workspace_source: String,
    /// This is a path on the Docker daemon's machine, NOT proven local.
    pub daemon_host_path: Option<PathBuf>,
}
#[derive(Serialize)]
pub struct Summary<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub local_folder_label: Option<&'a str>,
    pub configured_user: &'a str,
    pub configured_workdir: &'a str,
}

impl Container {
    pub(crate) fn label(&self, key: &str) -> Option<&str> {
        self.config.labels.as_ref()?.get(key).map(String::as_str)
    }
    pub fn local_folder(&self) -> Option<&str> {
        self.label("devcontainer.local_folder")
            .or_else(|| self.label("vsch.local.folder"))
    }
    pub fn is_devcontainer(&self) -> bool {
        self.local_folder().is_some()
            || self.label("devcontainer.config_file").is_some()
            || self.label("devcontainer.metadata").is_some()
    }
    pub fn display_label(&self) -> String {
        let project = self
            .local_folder()
            .map(|p| prompt::local_path(Path::new(p)))
            .unwrap_or_else(|| "workspace unknown".into());
        prompt::path_item(self.name.trim_start_matches('/'), &project)
    }
    pub fn summary(&self) -> Summary<'_> {
        Summary {
            id: &self.id,
            name: self.name.trim_start_matches('/'),
            local_folder_label: self.local_folder(),
            configured_user: &self.config.user,
            configured_workdir: &self.config.working_dir,
        }
    }
    fn remote_user(&self) -> Result<Option<String>> {
        let Some(label) = self.label("devcontainer.metadata") else {
            return Ok(None);
        };
        let value: Value = serde_json::from_str(label).map_err(|e| {
            format!(
                "invalid devcontainer.metadata on {:?}: {e}; supply --user",
                self.name
            )
        })?;
        let entries = match value {
            Value::Array(v) => v,
            Value::Object(_) => vec![value],
            _ => {
                return Err(
                    "devcontainer.metadata must be an object or array; supply --user".into(),
                )
            }
        };
        let mut user = None;
        for entry in entries {
            if !entry.is_object() {
                return Err("devcontainer.metadata entries must be objects; supply --user".into());
            }
            if let Some(v) = entry.get("remoteUser") {
                user = Some(
                    v.as_str()
                        .ok_or("remoteUser must be a string; supply --user")?
                        .to_owned(),
                );
            }
        }
        Ok(user)
    }
}

pub fn discover(explicit: Option<&str>) -> Result<Vec<Container>> {
    discover_inner(explicit, false)
}

pub fn discover_all() -> Result<Vec<Container>> {
    discover_inner(None, true)
}

fn discover_inner(explicit: Option<&str>, all: bool) -> Result<Vec<Container>> {
    let ids: Vec<String> = if let Some(name) = explicit {
        if name.is_empty() || name.starts_with('-') {
            return Err("--container requires a nonempty name or ID, not an option".into());
        }
        vec![name.to_owned()]
    } else {
        let mut command = Command::new("docker");
        command.args(["ps", "--quiet", "--no-trunc"]);
        if all {
            command.arg("--all");
        }
        let bytes = capture(&mut command, "list running containers")?;
        String::from_utf8(bytes)
            .map_err(|e| e.to_string())?
            .lines()
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect()
    };
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = capture(
        Command::new("docker")
            .args(["inspect", "--type", "container", "--"])
            .args(ids),
        "inspect containers",
    )?;
    let mut containers: Vec<Container> = serde_json::from_slice(&bytes)
        .map_err(|e| format!("invalid Docker inspect response: {e}"))?;
    if explicit.is_some() {
        if containers.len() != 1 {
            return Err("Docker did not resolve exactly one container".into());
        }
        if !containers[0].state.running || containers[0].state.paused {
            return Err(
                "selected container is stopped or paused; start/unpause it before launching Hatch"
                    .into(),
            );
        }
    } else {
        containers.retain(|c| (all || (c.state.running && !c.state.paused)) && c.is_devcontainer());
    }
    containers.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(containers)
}

pub fn select(containers: &[Container], interactive: bool) -> Result<&Container> {
    match containers.len() {
        0 => Err("no running devcontainers found; start one first or specify --container NAME --workspace /path --user USER".into()),
        1 => Ok(&containers[0]),
        _ => {
            if !interactive {
                let names: Vec<_> = containers.iter().map(|c| c.name.trim_start_matches('/')).collect();
                return Err(format!("multiple devcontainers found: {names:?}; specify --container NAME"));
            }
            let labels = containers.iter().map(Container::display_label).collect::<Vec<_>>();
            Ok(&containers[prompt::choose("Choose a devcontainer", &labels, interactive)?])
        }
    }
}

pub fn absolute_path(value: &str) -> Result<()> {
    let path = Path::new(value);
    if !path.is_absolute()
        || value.contains("${")
        || value.contains('\0')
        || path.components().any(|p| matches!(p, Component::ParentDir))
    {
        return Err(format!(
            "expected a resolved absolute container path without '..', got {value:?}"
        ));
    }
    Ok(())
}

/// Respect the most specific mount, including a volume shadowing a bind mount.
pub(crate) fn host_path(mounts: &[Mount], workspace: &str) -> Option<PathBuf> {
    let path = Path::new(workspace);
    let mount = mounts
        .iter()
        .filter(|m| path.starts_with(&m.destination))
        .max_by_key(|m| Path::new(&m.destination).components().count())?;
    if mount.kind != "bind" || !Path::new(&mount.source).is_absolute() {
        return None;
    }
    let suffix = path.strip_prefix(&mount.destination).ok()?;
    Some(if suffix.as_os_str().is_empty() {
        PathBuf::from(&mount.source)
    } else {
        Path::new(&mount.source).join(suffix)
    })
}

pub fn resolve(
    container: &Container,
    workspace: Option<&str>,
    user: Option<&str>,
    interactive: bool,
) -> Result<Context> {
    let (user, user_source) = if let Some(u) = user {
        (u.to_owned(), "--user")
    } else if let Some(u) = container.remote_user()? {
        (u, "devcontainer.metadata remoteUser")
    } else if !container.config.user.is_empty() {
        (container.config.user.clone(), "Docker Config.User")
    } else {
        (
            "0".into(),
            "Docker default user (root); override with --user if needed",
        )
    };
    if user.is_empty() || user.contains("${") || user.contains('\0') || user.starts_with('-') {
        return Err("unresolved or invalid container user; supply --user NAME or UID".into());
    }
    let (workspace, workspace_source) = if let Some(path) = workspace {
        (path.to_owned(), "--workspace")
    } else {
        // Labels refer to the daemon host. Match component boundaries, not string
        // prefixes; a parent bind can contain several projects.
        let mut mapped = Vec::new();
        if let Some(local) = container.local_folder() {
            if absolute_path(local).is_ok() {
                for mount in &container.mounts {
                    if mount.kind != "bind" || absolute_path(&mount.source).is_err() {
                        continue;
                    }
                    if let Ok(suffix) = Path::new(local).strip_prefix(&mount.source) {
                        let candidate = if suffix.as_os_str().is_empty() {
                            PathBuf::from(&mount.destination)
                        } else {
                            Path::new(&mount.destination).join(suffix)
                        };
                        if let Some(candidate) = candidate.to_str() {
                            // A more specific mount must still map to the labeled project.
                            if host_path(&container.mounts, candidate).as_deref()
                                == Some(Path::new(local))
                            {
                                mapped.push(candidate.to_owned());
                            }
                        }
                    }
                }
            }
        }
        mapped.sort();
        mapped.dedup();
        match mapped.len() {
            1 => (mapped.remove(0), "local folder label + bind mount"),
            n if n > 1 => {
                if !interactive { return Err(format!("multiple workspace mappings: {mapped:?}; specify --workspace /path")); }
                let i = prompt::choose("Choose a workspace", &mapped, interactive)?;
                (mapped.remove(i), "selected bind mount")
            }
            _ if !container.config.working_dir.is_empty() && container.config.working_dir != "/" =>
                (container.config.working_dir.clone(), "Docker Config.WorkingDir (override with --workspace if needed)"),
            _ => return Err("cannot infer workspace from labels/mounts or Docker WorkingDir; supply --workspace /absolute/container/path".into()),
        }
    };
    absolute_path(&workspace)?;
    Ok(Context {
        configuration_source: "docker-metadata".into(),
        config_file: None,
        host_workspace: None,
        user_env_probe: None,
        warnings: Vec::new(),
        remote_env_names: Vec::new(),
        remote_env: BTreeMap::new(),
        container_id: container.id.clone(),
        container_name: container.name.trim_start_matches('/').into(),
        user,
        user_source: user_source.into(),
        daemon_host_path: host_path(&container.mounts, &workspace),
        workspace,
        workspace_source: workspace_source.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixtures() -> Vec<Container> {
        serde_json::from_str(include_str!("../tests/fixtures/containers.json")).unwrap()
    }
    #[test]
    fn layouts_and_user_precedence() {
        let c = fixtures();
        let first = resolve(&c[0], None, None, false).unwrap();
        assert_eq!(first.workspace, "/workspaces/alpha");
        assert_eq!(first.user, "developer");
        assert_eq!(
            first.daemon_host_path.unwrap(),
            Path::new("/projects/alpha")
        );
        let second = resolve(&c[1], None, None, false).unwrap();
        assert_eq!(second.workspace, "/srv/beta");
        assert_eq!(second.user, "1001:1001");
        let ordinary = resolve(&c[2], Some("/app"), Some("1000"), false).unwrap();
        assert_eq!(ordinary.user, "1000");
        assert!(!c[2].is_devcontainer());
    }
    #[test]
    fn missing_and_ambiguous_metadata_do_not_guess() {
        let mut c = fixtures().remove(0);
        c.mounts.clear();
        assert!(resolve(&c, None, None, false).is_err());
        c.config
            .labels
            .as_mut()
            .unwrap()
            .insert("devcontainer.metadata".into(), "broken".into());
        assert!(resolve(&c, Some("/app"), None, false).is_err());
        assert!(resolve(&c, Some("/app"), Some("me"), false).is_ok());
        assert!(select(&[], false).is_err());
        assert!(select(&fixtures(), false).is_err());
    }
    #[test]
    fn generic_parent_mount_and_explicit_overrides() {
        let mut container = fixtures().remove(0);
        container.mounts[0].source = "/projects".into();
        container.mounts[0].destination = "/src".into();
        let context = resolve(&container, None, None, false).unwrap();
        assert_eq!(context.workspace, "/src/alpha");
        assert_eq!(context.user, "developer");
        let context = resolve(&container, Some("/custom/project"), Some("4567"), false).unwrap();
        assert_eq!(context.workspace, "/custom/project");
        assert_eq!(context.user, "4567");
        assert_eq!(context.workspace_source, "--workspace");
        assert_eq!(context.user_source, "--user");
        assert!(context.daemon_host_path.is_none());
    }

    #[test]
    fn ambiguous_mount_mapping_requires_an_explicit_choice() {
        let mut container = fixtures().remove(0);
        let mut duplicate = container.mounts[0].clone();
        duplicate.destination = "/alternate/alpha".into();
        container.mounts.push(duplicate);
        let error = resolve(&container, None, None, false).unwrap_err();
        assert!(error.contains("multiple workspace mappings"));
        let context = resolve(&container, Some("/alternate/alpha"), None, false).unwrap();
        assert_eq!(context.workspace, "/alternate/alpha");
    }

    #[test]
    fn paths_and_mount_boundaries() {
        for bad in [
            "relative",
            "/work/../etc",
            "${containerWorkspaceFolder}",
            "/a\0b",
        ] {
            assert!(absolute_path(bad).is_err());
        }
        let mut c = fixtures().remove(0);
        assert!(host_path(&c.mounts, "/workspaces/alphabeta").is_none());
        c.mounts.push(Mount {
            kind: "volume".into(),
            source: "/daemon/volume".into(),
            destination: "/workspaces/alpha/private".into(),
        });
        assert!(host_path(&c.mounts, "/workspaces/alpha/private/x").is_none());
    }
}
