//! Per-thread monotonic `T-XXXX` id counter persisted at `tasks/.next_id`.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

use fs2::FileExt;

use crate::Error;

pub fn next_id(tasks_dir: &Path) -> Result<String, Error> {
    fs::create_dir_all(tasks_dir)?;
    let path = tasks_dir.join(".next_id");

    let lock_path = tasks_dir.join(".next_id.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    lock.lock_exclusive()?;

    let n = read_counter(tasks_dir, &path)? + 1;
    write_counter(&path, n)?;

    FileExt::unlock(&lock)?;
    Ok(format!("T-{:04}", n))
}

fn read_counter(tasks_dir: &Path, path: &Path) -> Result<u32, Error> {
    match fs::read_to_string(path) {
        Ok(buf) => match buf.trim().parse::<u32>() {
            Ok(n) => Ok(n),
            Err(_) => max_existing_task_id(tasks_dir),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(err) => Err(err.into()),
    }
}

fn max_existing_task_id(tasks_dir: &Path) -> Result<u32, Error> {
    let mut max_id = 0;
    for entry in fs::read_dir(tasks_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(raw_id) = name
            .strip_prefix("T-")
            .and_then(|value| value.strip_suffix(".toml"))
        else {
            continue;
        };
        if let Ok(id) = raw_id.parse::<u32>() {
            max_id = max_id.max(id);
        }
    }
    Ok(max_id)
}

fn write_counter(path: &Path, n: u32) -> Result<(), Error> {
    let tmp_path = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("next_id"),
        std::process::id()
    ));

    if tmp_path.exists() {
        fs::remove_file(&tmp_path)?;
    }

    {
        let mut tmp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        write!(tmp, "{n}")?;
        tmp.sync_all()?;
    }

    fs::rename(&tmp_path, path)?;
    sync_parent(path)?;
    Ok(())
}

fn sync_parent(path: &Path) -> Result<(), Error> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let dir = File::open(parent)?;
    dir.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_id_persists_monotonic_counter() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(next_id(dir.path()).unwrap(), "T-0001");
        assert_eq!(next_id(dir.path()).unwrap(), "T-0002");
        assert_eq!(
            fs::read_to_string(dir.path().join(".next_id")).unwrap(),
            "2"
        );
    }

    #[test]
    fn next_id_recovers_corrupt_counter_from_existing_tasks() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".next_id"), "").unwrap();
        fs::write(dir.path().join("T-0007.toml"), "").unwrap();
        fs::write(dir.path().join("T-not-a-number.toml"), "").unwrap();
        fs::write(dir.path().join("not-a-task.toml"), "").unwrap();

        assert_eq!(next_id(dir.path()).unwrap(), "T-0008");
        assert_eq!(
            fs::read_to_string(dir.path().join(".next_id")).unwrap(),
            "8"
        );
    }
}
