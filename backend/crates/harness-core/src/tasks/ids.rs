//! Per-thread monotonic `T-XXXX` id counter persisted at `tasks/.next_id`.

use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use fs2::FileExt;

use crate::Error;

pub fn next_id(tasks_dir: &Path) -> Result<String, Error> {
    fs::create_dir_all(tasks_dir)?;
    let path = tasks_dir.join(".next_id");
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    f.lock_exclusive()?;

    let mut buf = String::new();
    f.read_to_string(&mut buf)?;
    let n: u32 = buf.trim().parse().unwrap_or(0) + 1;

    f.seek(SeekFrom::Start(0))?;
    f.set_len(0)?;
    f.write_all(n.to_string().as_bytes())?;
    f.sync_all()?;

    FileExt::unlock(&f)?;
    Ok(format!("T-{:04}", n))
}
