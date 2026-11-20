//! Retain bounded failure logs in per-user storage.
//! Uses unique private files and removes older owned logs.

use std::{fs, io::Write, path::PathBuf};

const MAX_BYTES: usize = 1_048_576;
const KEEP_FILES: usize = 20;

pub(super) fn save(stdout: &[u8], stderr: &[u8]) -> Option<PathBuf> {
    let dir = crate::config::dirs_home().join(".ai-summary/tee");
    fs::create_dir_all(&dir).ok()?;
    let mut temporary = tempfile::Builder::new()
        .prefix("wrap-")
        .suffix(".log")
        .tempfile_in(&dir)
        .ok()?;
    let limit = stdout.len().min(MAX_BYTES);
    temporary.write_all(&stdout[..limit]).ok()?;
    let rest = MAX_BYTES - limit;
    if rest > 0 && !stderr.is_empty() {
        temporary.write_all(b"\n").ok()?;
        temporary
            .write_all(&stderr[..stderr.len().min(rest - 1)])
            .ok()?;
    }
    let (_, path) = temporary.keep().ok()?;
    let mut files: Vec<_> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let name = entry.file_name();
            let name = name.to_str()?;
            if !file_type.is_file() || !name.starts_with("wrap-") || !name.ends_with(".log") {
                return None;
            }
            Some((entry.metadata().ok()?.modified().ok()?, entry.path()))
        })
        .collect();
    files.sort_unstable();
    let excess = files.len().saturating_sub(KEEP_FILES);
    for (_, stale) in files
        .into_iter()
        .filter(|(_, file)| file != &path)
        .take(excess)
    {
        let _ = fs::remove_file(stale);
    }
    Some(path)
}
