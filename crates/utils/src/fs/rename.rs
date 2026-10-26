use std::fs;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// Renames atomically, allowing brief Windows reader/antivirus locks to drain.
///
/// Only failed Windows access/sharing/lock violations are retried. No content is
/// deleted, permissions changed, or copy fallback attempted. Callers must already
/// own the mutation and keep its source/destination authority for the bounded wait.
/// Returns the number of calls so callers can report transient contention.
pub fn rename_with_retry(from: &Path, to: &Path) -> io::Result<usize> {
    let started = Instant::now();
    let mut attempts = 0;
    loop {
        attempts += 1;
        match fs::rename(from, to) {
            Err(error) if retryable(&error) && started.elapsed() < Duration::from_secs(1) => {
                std::thread::sleep(Duration::from_millis(50));
            }
            result => return result.map(|()| attempts),
        }
    }
}

/// PermissionDenied alone is insufficient: non-Windows permissions must not be retried.
fn retryable(error: &io::Error) -> bool {
    cfg!(windows) && matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Retrying a rename never falls back to deleting or merging an occupied destination.
    #[test]
    fn occupied_directory_preserves_both_trees() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("source.txt"), b"source").unwrap();
        fs::write(destination.join("user.txt"), b"user").unwrap();
        assert!(rename_with_retry(&source, &destination).is_err());
        assert_eq!(
            (
                fs::read(source.join("source.txt")).unwrap(),
                fs::read(destination.join("user.txt")).unwrap()
            ),
            (b"source".to_vec(), b"user".to_vec())
        );
    }

    /// A normal directory rename reports one attempt and moves the complete tree.
    #[test]
    fn uncontended_directory_moves_once() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("file.txt"), b"bytes").unwrap();
        assert_eq!(rename_with_retry(&source, &destination).unwrap(), 1);
        assert!(!source.exists());
        assert_eq!(fs::read(destination.join("file.txt")).unwrap(), b"bytes");
    }
}
