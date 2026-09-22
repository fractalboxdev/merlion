//! File handling for a hostile working tree (specs/integrations.md#file-handling).

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// A hint larger than the core's 1 MiB input limit is ignored (`I022`).
pub const MAX_HINT_BYTES: u64 = 1 << 20;

/// Checks a path the CLI is about to write and returns the path to write to.
///
/// Without `follow`, the path must not be a symbolic link and its resolved directory must
/// lie inside `cwd`. With `follow`, a symbolic link is resolved and its target written.
pub fn guard_target(path: &Path, cwd: &Path, follow: bool) -> Result<PathBuf, String> {
    guard(path, cwd, follow, false)
}

/// Checks a hint path before reading it; the same rules as [`guard_target`], and the file
/// must exist.
pub fn guard_hint(path: &Path, cwd: &Path, follow: bool) -> Result<PathBuf, String> {
    guard(path, cwd, follow, true)
}

fn guard(path: &Path, cwd: &Path, follow: bool, must_exist: bool) -> Result<PathBuf, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if !follow {
                return Err("is a symbolic link (pass --follow-symlinks to allow)".into());
            }
            return path
                .canonicalize()
                .map_err(|e| format!("cannot resolve symbolic link: {e}"));
        }
        Ok(meta) if meta.is_dir() => return Err("is a directory".into()),
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound && !must_exist => {}
        Err(e) => return Err(e.to_string()),
    }
    let name = path
        .file_name()
        .ok_or_else(|| "does not name a file".to_string())?;
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    if !must_exist && fs::symlink_metadata(parent).is_err() {
        create_output_dir(parent, cwd, follow)?;
    }
    let dir = parent
        .canonicalize()
        .map_err(|e| format!("directory is not usable: {e}"))?;
    if !follow {
        let root = cwd
            .canonicalize()
            .map_err(|e| format!("working directory is not usable: {e}"))?;
        if !dir.starts_with(&root) {
            return Err(
                "resolves outside the working directory (pass --follow-symlinks to allow)".into(),
            );
        }
    }
    Ok(dir.join(name))
}

/// Creates the missing directories of an output path, like `mkdir -p`, under the same
/// rules as the write itself: the nearest existing ancestor resolves (symbolic links
/// included) inside `cwd` unless `follow`, and the missing part is plain names only, so
/// `..` can never climb back out of the resolved ancestor.
fn create_output_dir(dir: &Path, cwd: &Path, follow: bool) -> Result<(), String> {
    use std::path::Component;

    let mut existing = dir;
    let mut missing = Vec::new();
    while fs::symlink_metadata(existing).is_err() {
        let name = existing
            .file_name()
            .ok_or_else(|| "directory is not usable".to_string())?;
        missing.push(name);
        existing = match existing.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
    }
    let tail: PathBuf = missing.iter().rev().collect();
    if !tail.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err("directory is not usable: missing part contains `..`".into());
    }
    let base = existing
        .canonicalize()
        .map_err(|e| format!("directory is not usable: {e}"))?;
    if !base.is_dir() {
        return Err("directory is not usable: not a directory".into());
    }
    if !follow {
        let root = cwd
            .canonicalize()
            .map_err(|e| format!("working directory is not usable: {e}"))?;
        if !base.starts_with(&root) {
            return Err(
                "resolves outside the working directory (pass --follow-symlinks to allow)".into(),
            );
        }
    }
    fs::create_dir_all(base.join(tail)).map_err(|e| format!("cannot create directory: {e}"))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Hint {
    Text(String),
    TooLarge,
    NotUtf8,
}

/// Reads at most `MAX_HINT_BYTES` of a hint file; a larger file is `TooLarge` even if it
/// grows between the size check and the read.
pub fn read_hint(path: &Path) -> io::Result<Hint> {
    let f = fs::File::open(path)?;
    if f.metadata()?.len() > MAX_HINT_BYTES {
        return Ok(Hint::TooLarge);
    }
    let mut buf = Vec::new();
    f.take(MAX_HINT_BYTES + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_HINT_BYTES {
        return Ok(Hint::TooLarge);
    }
    Ok(String::from_utf8(buf).map_or(Hint::NotUtf8, Hint::Text))
}

/// Writes `bytes` to a temporary file in `target`'s directory, flushes it to disk and
/// renames it over `target`. The rename replaces a symbolic link instead of writing
/// through it, and a crash leaves either the old file or the new one.
pub fn atomic_write(target: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let dir = match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    // `create_new` (O_EXCL) never opens an existing file or follows a planted link.
    let mut attempts = 0;
    let (tmp, mut file) = loop {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut tmp_name = std::ffi::OsString::from(".");
        tmp_name.push(name);
        tmp_name.push(format!(".{}-{n}.merlion-tmp", std::process::id()));
        let tmp = dir.join(tmp_name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(f) => break (tmp, f),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempts < 64 => {
                attempts += 1;
            }
            Err(e) => return Err(e),
        }
    };
    let result = (|| {
        file.write_all(bytes)?;
        // Keep the permissions of a regular file being replaced.
        if let Ok(meta) = fs::symlink_metadata(target) {
            if meta.is_file() {
                file.set_permissions(meta.permissions())?;
            }
        }
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
        return result;
    }
    // Persist the rename itself; not every platform can open a directory, so this is
    // best effort.
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

#[cfg(test)]
pub mod testdir {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    /// A fresh, canonical directory under the system temp dir.
    pub fn fresh(tag: &str) -> PathBuf {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d =
            std::env::temp_dir().join(format!("merlion-cli-unit-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("create temp dir");
        d.canonicalize().expect("canonical temp dir")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn atomic_write_creates_and_replaces_without_leftovers() {
        let d = testdir::fresh("aw");
        let t = d.join("out.svg");
        atomic_write(&t, b"one").unwrap();
        assert_eq!(fs::read(&t).unwrap(), b"one");
        atomic_write(&t, b"two").unwrap();
        assert_eq!(fs::read(&t).unwrap(), b"two");
        assert_eq!(entries(&d), vec!["out.svg"]);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_replaces_a_link_instead_of_writing_through() {
        let d = testdir::fresh("awlink");
        let victim = d.join("victim");
        fs::write(&victim, b"keep").unwrap();
        let link = d.join("out.svg");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        atomic_write(&link, b"new").unwrap();
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        assert!(!fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(&link).unwrap(), b"new");
    }

    #[test]
    fn guard_accepts_paths_inside_cwd() {
        let d = testdir::fresh("gin");
        fs::create_dir(d.join("sub")).unwrap();
        let got = guard_target(&d.join("sub/a.svg"), &d, false).unwrap();
        assert_eq!(got, d.join("sub").join("a.svg"));
    }

    #[test]
    fn guard_refuses_paths_outside_cwd_unless_following() {
        let d = testdir::fresh("gout");
        fs::create_dir(d.join("cwd")).unwrap();
        let outside = d.join("x.svg");
        let cwd = d.join("cwd");
        assert!(guard_target(&outside, &cwd, false)
            .unwrap_err()
            .contains("outside"));
        assert!(guard_target(&cwd.join("../x.svg"), &cwd, false).is_err());
        assert_eq!(guard_target(&outside, &cwd, true).unwrap(), outside);
    }

    #[test]
    fn guard_creates_missing_output_directories_inside_cwd() {
        let d = testdir::fresh("gmk");
        let got = guard_target(&d.join("out/nested/a.svg"), &d, false).unwrap();
        assert_eq!(got, d.join("out").join("nested").join("a.svg"));
        assert!(d.join("out/nested").is_dir());
        // A relative path resolves against the process cwd, which the caller passes.
        assert!(guard_target(&d, &d, false).is_err());
    }

    #[test]
    fn guard_never_creates_directories_outside_cwd() {
        let d = testdir::fresh("gmkout");
        fs::create_dir(d.join("cwd")).unwrap();
        let cwd = d.join("cwd");
        assert!(guard_target(&cwd.join("new/../../escape/a.svg"), &cwd, false).is_err());
        assert!(!d.join("escape").exists());
        assert!(!cwd.join("new").exists());
        assert!(guard_target(&d.join("elsewhere/a.svg"), &cwd, false).is_err());
        assert!(!d.join("elsewhere").exists());
    }

    #[cfg(unix)]
    #[test]
    fn guard_never_creates_directories_through_a_symlink() {
        let d = testdir::fresh("gmklink");
        fs::create_dir_all(d.join("cwd")).unwrap();
        fs::create_dir_all(d.join("outside")).unwrap();
        let cwd = d.join("cwd");
        std::os::unix::fs::symlink(d.join("outside"), cwd.join("link")).unwrap();
        assert!(guard_target(&cwd.join("link/new/a.svg"), &cwd, false).is_err());
        assert!(!d.join("outside/new").exists());
    }

    #[test]
    fn guard_hint_never_creates_directories() {
        let d = testdir::fresh("ghmk");
        assert!(guard_hint(&d.join("nope/h.svg"), &d, false).is_err());
        assert!(!d.join("nope").exists());
    }

    #[cfg(unix)]
    #[test]
    fn guard_refuses_symlinks_unless_following() {
        let d = testdir::fresh("glink");
        let real = d.join("real.svg");
        fs::write(&real, b"x").unwrap();
        let link = d.join("link.svg");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(guard_target(&link, &d, false)
            .unwrap_err()
            .contains("symbolic link"));
        assert!(guard_hint(&link, &d, false)
            .unwrap_err()
            .contains("symbolic link"));
        assert_eq!(guard_target(&link, &d, true).unwrap(), real);
        assert_eq!(guard_hint(&link, &d, true).unwrap(), real);
    }

    #[test]
    fn hint_rules() {
        let d = testdir::fresh("hint");
        let h = d.join("h.svg");
        fs::write(&h, "<svg/>").unwrap();
        assert_eq!(guard_hint(&h, &d, false).unwrap(), h);
        assert_eq!(read_hint(&h).unwrap(), Hint::Text("<svg/>".into()));
        fs::write(&h, [0xff, 0xfe]).unwrap();
        assert_eq!(read_hint(&h).unwrap(), Hint::NotUtf8);
        fs::write(&h, vec![b'a'; MAX_HINT_BYTES as usize + 1]).unwrap();
        assert_eq!(read_hint(&h).unwrap(), Hint::TooLarge);
        assert!(guard_hint(&d.join("missing.svg"), &d, false).is_err());
    }
}
