//! Local workspace selection. Only folder entries are read; no editor tasks run.
use crate::{prompt, provision, Result};
use std::path::{Path, PathBuf};

pub fn pasted_path(value: &str) -> PathBuf {
    let value = value.trim();
    let value = if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len().saturating_sub(1)]
    } else {
        value
    };
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(value)
}

// JSONC normalization preserves quoted content. serde_json still validates the
// grammar; comments and trailing commas are the only accepted extensions.
fn jsonc(text: &str) -> Result<serde_json::Value> {
    let mut bytes = text.trim_start_matches('\u{feff}').as_bytes().to_vec();
    let (mut i, mut quoted) = (0, false);
    while i < bytes.len() {
        if quoted {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                quoted = false;
            }
        } else if bytes[i] == b'"' {
            quoted = true;
        } else if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                bytes[i] = b' ';
                i += 1;
            }
            continue;
        } else if bytes[i..].starts_with(b"/*") {
            bytes[i] = b' ';
            bytes[i + 1] = b' ';
            i += 2;
            while i + 1 < bytes.len() && !bytes[i..].starts_with(b"*/") {
                bytes[i] = b' ';
                i += 1;
            }
            if i + 1 >= bytes.len() {
                return Err("Unclosed comment in workspace file".into());
            }
            bytes[i] = b' ';
            bytes[i + 1] = b' ';
            i += 2;
            continue;
        }
        i += 1;
    }
    let (mut i, mut quoted) = (0, false);
    while i < bytes.len() {
        if quoted {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                quoted = false;
            }
        } else if bytes[i] == b'"' {
            quoted = true;
        } else if bytes[i] == b',' {
            let next = bytes[i + 1..].iter().find(|b| !b.is_ascii_whitespace());
            if matches!(next, Some(b']' | b'}')) {
                bytes[i] = b' ';
            }
        }
        i += 1;
    }
    serde_json::from_slice(&bytes).map_err(|e| {
        format!(
            "Invalid workspace JSON at line {}, column {}",
            e.line(),
            e.column()
        )
    })
}

pub fn folder(path: &Path, interactive: bool) -> Result<PathBuf> {
    if path.is_dir() {
        return path.canonicalize().map_err(|e| e.to_string());
    }
    if path.extension().is_none_or(|e| e != "code-workspace") {
        return Err("Choose an existing folder or a .code-workspace file".into());
    }
    let path = path
        .canonicalize()
        .map_err(|e| format!("Workspace file: {e}"))?;
    let value = jsonc(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)?;
    let entries = value
        .get("folders")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty())
        .ok_or("Workspace file has no folders; choose a project folder instead")?;
    let mut folders = Vec::new();
    let mut labels = Vec::new();
    for entry in entries {
        let relative = entry.get("path").and_then(|v| v.as_str()).ok_or(
            "Workspace contains a remote/URI folder. Choose a local project folder instead",
        )?;
        if relative.contains("${") {
            return Err(
                "Workspace path variables are not supported; choose a local project folder instead"
                    .into(),
            );
        }
        let folder = path.parent().unwrap().join(relative);
        if !folder.is_dir() {
            return Err(format!(
                "Workspace folder is unavailable: {}",
                folder.display()
            ));
        }
        let folder = folder.canonicalize().map_err(|e| e.to_string())?;
        if folders.contains(&folder) {
            continue;
        }
        let name = entry
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Folder");
        labels.push(prompt::path_item(name, &prompt::local_path(&folder)));
        folders.push(folder);
    }
    let index = if folders.len() == 1 {
        0
    } else if interactive {
        prompt::note("This workspace contains multiple folders. Choose the project folder for devcontainer setup.");
        prompt::choose("Select a project folder", &labels, true)?
    } else {
        return Err(
            "Workspace contains multiple folders; supply --host-workspace with the chosen folder"
                .into(),
        );
    };
    Ok(folders.remove(index))
}

fn empty_workspace(path: &Path) -> Result<bool> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Workspace file: {e}"))?;
    if text.trim().is_empty() {
        return Ok(true);
    }
    let value = jsonc(&text)?;
    Ok(value.is_object()
        && match value.get("folders") {
            None => true,
            Some(serde_json::Value::Array(folders)) => folders.is_empty(),
            _ => false,
        })
}

fn create_empty_workspace(path: &Path) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            format!("Could not create workspace file (existing files are never overwritten): {e}")
        })?;
    file.write_all(b"{\n  \"folders\": []\n}\n")
        .map_err(|e| format!("Workspace file: {e}"))
}

pub fn choose(config: &Path) -> Result<PathBuf> {
    let inferred = provision::workspace(config, None).ok();
    let mut roots = vec![std::env::current_dir().map_err(|e| format!("Current directory: {e}"))?];
    let config_dir = config.parent().unwrap().to_owned();
    if !roots.contains(&config_dir) {
        roots.push(config_dir);
    }
    if let Some(root) = &inferred {
        if !roots.contains(root) {
            roots.push(root.clone());
        }
    }
    let mut candidates = Vec::new();
    for root in &roots {
        let mut files = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "code-workspace"))
            .collect::<Vec<_>>();
        files.sort();
        for file in files {
            let file = file
                .canonicalize()
                .map_err(|e| format!("Workspace file: {e}"))?;
            if !candidates.contains(&file) {
                candidates.push(file);
            }
        }
    }
    let create = if candidates.is_empty() {
        inferred.as_ref().map(|root| {
            root.join(format!(
                "{}.code-workspace",
                root.file_name().unwrap_or_default().to_string_lossy()
            ))
        })
    } else {
        None
    };
    if candidates.is_empty() {
        prompt::note("No VS Code .code-workspace files found. Choose a project folder or paste a .code-workspace file path.");
    }
    if let Some(root) = &inferred {
        candidates.push(root.clone());
    }
    loop {
        let mut labels = candidates
            .iter()
            .map(|p| {
                if p.is_file() {
                    prompt::path_item("VS Code workspace", &prompt::local_path(p))
                } else {
                    prompt::path_item("Detected project folder", &prompt::local_path(p))
                }
            })
            .collect::<Vec<_>>();
        let create_index = create.as_ref().map(|path| {
            let index = labels.len();
            labels.push(prompt::path_item(
                "Create workspace file",
                &prompt::local_path(path),
            ));
            index
        });
        let paste_index = labels.len();
        labels.extend([
            "Paste a .code-workspace file or folder path…".into(),
            "Back".into(),
        ]);
        let index = prompt::choose_default("Select a workspace", &labels, true, Some(0))?;
        if index == paste_index + 1 {
            return Err("cancelled; nothing launched".into());
        }
        if Some(index) == create_index {
            let path = create.as_ref().unwrap();
            match create_empty_workspace(path) {
                Ok(()) => {
                    let root = inferred.as_ref().unwrap();
                    prompt::section(
                        "Workspace created",
                        &[
                            ("File", &prompt::local_path(path)),
                            ("Folder", &prompt::local_path(root)),
                        ],
                    );
                    return Ok(root.clone());
                }
                Err(error) => return Err(error),
            }
        }
        let path = if index == paste_index {
            let text = prompt::input("Workspace file or folder path")?;
            if text.is_empty() || text == "q" {
                continue;
            }
            pasted_path(&text)
        } else {
            candidates[index].clone()
        };
        let resolved = (|| {
            if path.is_file()
                && path.extension().is_some_and(|e| e == "code-workspace")
                && empty_workspace(&path)?
            {
                if let Some(root) = &inferred {
                    return Ok(root.clone());
                }
                prompt::note("This workspace file is empty. Choose the project folder to use; the file will stay empty.");
                let text = prompt::input("Project folder path")?;
                if text.is_empty() || text == "q" {
                    return Err("cancelled; nothing launched".into());
                }
                let root = pasted_path(&text);
                if !root.is_dir() {
                    return Err("Choose an existing project folder".into());
                }
                return folder(&root, false);
            }
            folder(&path, true)
        })();
        match resolved {
            Ok(root) => {
                let file_label = path.is_file().then(|| prompt::local_path(&path));
                let folder_label = prompt::local_path(&root);
                let mut fields = Vec::new();
                if let Some(file) = &file_label {
                    fields.push(("File", file.as_str()));
                }
                fields.push(("Folder", folder_label.as_str()));
                prompt::section("Workspace selected", &fields);
                return Ok(root);
            }
            Err(error) if error.starts_with("cancelled") => continue,
            Err(error) => prompt::warning(&error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creating_workspace_never_overwrites_existing_content() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("project.code-workspace");
        std::fs::write(&path, "existing sentinel").unwrap();
        assert!(create_empty_workspace(&path).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "existing sentinel");
    }
    #[test]
    fn jsonc_preserves_strings_and_rejects_broken_input() {
        let v = jsonc(
            r#"{/* comment */ "url":"https://x//y/*z*/,}","folders":[{"path":"a",},],}// eof"#,
        )
        .unwrap();
        assert_eq!(v["url"], "https://x//y/*z*/,}");
        assert!(jsonc("{/* unfinished").is_err());
        assert!(jsonc(r#"{"x":"unterminated}"#).is_err());
    }
    #[test]
    fn workspace_paths_are_relative_to_file_and_ambiguity_is_explicit() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("project")).unwrap();
        let path = temp.path().join("dev.code-workspace");
        std::fs::write(&path, r#"{"folders":[{"path":"project"},],}"#).unwrap();
        assert_eq!(folder(&path, false).unwrap(), temp.path().join("project"));
        std::fs::write(&path, r#"{"folders":[{"path":"project"},{"path":"."}]}"#).unwrap();
        assert!(folder(&path, false)
            .unwrap_err()
            .contains("multiple folders"));
        std::fs::write(&path, r#"{"folders":[{"path":"missing"}]}"#).unwrap();
        assert!(folder(&path, false).is_err());
    }
}
