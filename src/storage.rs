//! Provide atomic file replacement and process-wide writer locks.
//! Used by local state and integration installers.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
};

pub fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub fn lock(path: &Path) -> io::Result<File> {
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    fs::create_dir_all(parent)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(path)?;
    lock.lock()?;
    Ok(lock)
}
