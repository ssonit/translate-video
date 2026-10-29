//! Commit a complete file beside its destination; never truncate the last checkpoint.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(crate) fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let name = path.file_name().ok_or("Đường dẫn file không hợp lệ.")?;
    let (temporary, mut file) = loop {
        let mut name = name.to_os_string();
        name.push(format!(
            ".{}.{}.partial",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let candidate = parent.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (Temporary(candidate), file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let written = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file); // Windows must release the handle before rename or cleanup.
    written.map_err(|e| e.to_string())?;
    // Same-directory rename replaces the destination without a delete-then-rename gap.
    fs::rename(&temporary.0, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "atomic-checkpoint-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn replacing_checkpoint_keeps_other_extensions_and_old_temporary_files() {
        let root = scratch();
        let json = root.join("line.json");
        let wav = root.join("line.wav");
        fs::write(root.join("line.tmp"), b"unrelated").unwrap();
        fs::write(root.join("line.json.earlier.partial"), b"interrupted").unwrap();
        write(&json, b"old").unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| write(&json, b"new json").unwrap());
            scope.spawn(|| write(&wav, b"new wav").unwrap());
        });
        assert_eq!(fs::read(&json).unwrap(), b"new json");
        assert_eq!(fs::read(&wav).unwrap(), b"new wav");
        assert_eq!(fs::read(root.join("line.tmp")).unwrap(), b"unrelated");
        assert_eq!(
            fs::read(root.join("line.json.earlier.partial")).unwrap(),
            b"interrupted"
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 4);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_commit_cleans_its_temporary_and_preserves_destination() {
        let root = scratch();
        let destination = root.join("occupied.json");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("keep"), b"keep").unwrap();
        assert!(write(&destination, b"replacement").is_err());
        assert_eq!(fs::read(destination.join("keep")).unwrap(), b"keep");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
