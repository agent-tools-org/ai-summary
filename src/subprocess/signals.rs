//! Track owned child process groups and stop them when the CLI is interrupted.
//! Holding the registry during wait and unregister prevents stale PID signaling.

use signal_hook::{
    consts::{SIGINT, SIGTERM},
    iterator::Signals,
};
use std::{
    collections::HashSet,
    io,
    process::{Child, Command, ExitStatus},
    sync::{Mutex, MutexGuard, OnceLock},
};

static OWNED: Mutex<Option<HashSet<i32>>> = Mutex::new(None);

pub struct ManagedChild {
    child: Child,
    registered: bool,
}

impl ManagedChild {
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        let mut groups = registry()?;
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        let child = command.spawn()?;
        groups
            .get_or_insert_with(HashSet::new)
            .insert(child.id() as i32);
        Ok(Self {
            child,
            registered: true,
        })
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let mut groups = registry()?;
        let result = self.child.try_wait()?;
        if result.is_some() {
            self.unregister(&mut groups);
        }
        Ok(result)
    }

    fn unregister(&mut self, groups: &mut Option<HashSet<i32>>) {
        if let Some(groups) = groups {
            groups.remove(&(self.child.id() as i32));
        }
        self.registered = false;
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if !self.registered {
            return;
        }
        // Only registered, unreaped leaders are signaled.
        let mut groups = OWNED.lock().unwrap_or_else(|error| error.into_inner());
        kill_group(self.child.id() as i32);
        let _ = self.child.wait();
        self.unregister(&mut groups);
    }
}

fn registry() -> io::Result<MutexGuard<'static, Option<HashSet<i32>>>> {
    static LISTENER: OnceLock<Result<(), String>> = OnceLock::new();
    LISTENER
        .get_or_init(|| {
            let mut signals = Signals::new([SIGINT, SIGTERM]).map_err(|error| error.to_string())?;
            std::thread::Builder::new()
                .name("ai-summary-signals".into())
                .spawn(move || {
                    if let Some(signal) = signals.forever().next() {
                        let groups = OWNED.lock().unwrap_or_else(|error| error.into_inner());
                        for &pid in groups.as_ref().into_iter().flatten() {
                            kill_group(pid);
                        }
                        std::process::exit(128 + signal);
                    }
                })
                .map_err(|error| error.to_string())?;
            Ok(())
        })
        .as_ref()
        .map_err(|error| io::Error::other(error.to_owned()))?;
    Ok(OWNED.lock().unwrap_or_else(|error| error.into_inner()))
}

fn kill_group(pid: i32) {
    // The spawn path allocates a distinct group named after the child PID.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
}
