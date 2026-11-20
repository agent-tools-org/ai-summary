//! Plan changes to the ai-summary hooks without touching other entries.
//! Uses exact command ownership and keeps user-provided matcher groups intact.

use serde_json::{json, Value};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

pub(super) const SCRIPTS: [(&str, &str, &str); 3] = [
    (
        "ai-summary-prebash.sh",
        include_str!("../../hooks/prebash.sh"),
        "Bash",
    ),
    (
        "ai-summary-prewebfetch.sh",
        include_str!("../../hooks/prewebfetch.sh"),
        "WebFetch",
    ),
    (
        "ai-summary-prewebsearch.sh",
        include_str!("../../hooks/prewebsearch.sh"),
        "WebSearch",
    ),
];

pub(super) fn plan(path: &Path, folder: &Path, uninstall: bool) -> Result<Option<Vec<u8>>, String> {
    let mut settings = match fs::read(path) {
        Ok(data) => {
            serde_json::from_slice::<Value>(&data).map_err(|_| "invalid Claude settings JSON")?
        }
        Err(error) if error.kind() == ErrorKind::NotFound && uninstall => return Ok(None),
        Err(error) if error.kind() == ErrorKind::NotFound => json!({}),
        Err(error) => return Err(format!("cannot read Claude settings: {error}")),
    };
    let original = settings.clone();
    let root = settings
        .as_object_mut()
        .ok_or("Claude settings must be a JSON object")?;
    if uninstall && root.get("hooks").is_none() {
        return Ok(None);
    }
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("Claude hooks must be a JSON object")?;
    if uninstall && hooks.get("PreToolUse").is_none() {
        return Ok(None);
    }
    let groups = hooks
        .entry("PreToolUse")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("Claude PreToolUse hooks must be an array")?;
    update_groups(groups, folder, uninstall);
    if uninstall && groups.is_empty() {
        hooks.remove("PreToolUse");
    }
    if uninstall && hooks.is_empty() {
        root.remove("hooks");
    }
    if settings == original {
        return Ok(None);
    }
    let mut bytes =
        serde_json::to_vec_pretty(&settings).map_err(|_| "cannot encode Claude settings")?;
    bytes.push(b'\n');
    Ok(Some(bytes))
}

fn update_groups(groups: &mut Vec<Value>, folder: &Path, uninstall: bool) {
    let commands: Vec<_> = SCRIPTS
        .iter()
        .map(|(name, _, matcher)| {
            let path = folder.join(name);
            let raw = path.to_string_lossy().into_owned();
            (*matcher, quote(&raw), raw)
        })
        .collect();
    groups.retain_mut(|group| {
        let Some(children) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            return true;
        };
        let previous = children.len();
        children.retain(|child| {
            !commands.iter().any(|(_, command, raw)| {
                child["type"] == "command"
                    && child["command"]
                        .as_str()
                        .is_some_and(|value| value == raw || value == command)
            })
        });
        children.len() == previous || !children.is_empty()
    });
    if !uninstall {
        for (matcher, command, _) in commands {
            groups.push(
                json!({"matcher": matcher, "hooks": [{"type": "command", "command": command}]}),
            );
        }
    }
}

fn quote(path: &str) -> String {
    if path
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"/_.-".contains(&byte))
    {
        return path.to_owned();
    }
    format!("'{}'", path.replace('\'', r"'\''"))
}

pub(super) fn path_for_update(path: PathBuf) -> Result<PathBuf, String> {
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)
            .map_err(|error| format!("cannot resolve Claude configuration link: {error}")),
        Ok(_) => Ok(path),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(path),
        Err(error) => Err(format!("cannot inspect Claude configuration: {error}")),
    }
}
