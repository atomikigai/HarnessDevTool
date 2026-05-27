//! Global pause kill-switch — flips the scheduler's auto-assignment off.
//!
//! Persistence is a single sentinel file at `<home>/.runtime/pause.flag`. The
//! file's mere existence means "paused"; absence means "running". Atomic
//! create/remove via a temp-file rename keeps the on-disk state consistent
//! across crashes.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::Error;

#[derive(Clone)]
pub struct PauseFlag {
    path: PathBuf,
    flag: Arc<AtomicBool>,
}

impl PauseFlag {
    /// Load (or create) the kill-switch rooted under `<home>/.runtime/`.
    pub fn load(home: &Path) -> Result<Self, Error> {
        let dir = home.join(".runtime");
        fs::create_dir_all(&dir)?;
        let path = dir.join("pause.flag");
        let initial = path.exists();
        Ok(Self {
            path,
            flag: Arc::new(AtomicBool::new(initial)),
        })
    }

    pub fn is_paused(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Set the pause state and persist atomically. `true` writes the sentinel
    /// (via temp + rename); `false` removes it.
    pub fn set(&self, paused: bool) -> Result<(), Error> {
        if paused {
            let tmp = self.path.with_extension("flag.tmp");
            fs::write(&tmp, b"")?;
            fs::rename(&tmp, &self.path)?;
        } else if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        self.flag.store(paused, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip_file_state() {
        let dir = tempdir().unwrap();
        let pf = PauseFlag::load(dir.path()).unwrap();
        assert!(!pf.is_paused());

        pf.set(true).unwrap();
        assert!(pf.is_paused());
        assert!(dir.path().join(".runtime/pause.flag").exists());

        // A fresh PauseFlag rehydrates the state from disk.
        let pf2 = PauseFlag::load(dir.path()).unwrap();
        assert!(pf2.is_paused());

        pf.set(false).unwrap();
        assert!(!pf.is_paused());
        assert!(!dir.path().join(".runtime/pause.flag").exists());

        let pf3 = PauseFlag::load(dir.path()).unwrap();
        assert!(!pf3.is_paused());
    }
}
