//! dowel-craft core: reading Bazis-Mebelshchik models, parts list, sheet cutting.

pub mod b3d;
pub mod contour;
pub mod design;
pub mod drawing;
pub mod export;
pub mod fasteners;
pub mod geom;
pub mod job;
pub mod library;
pub mod i18n;
pub mod live;
pub mod mesh;
pub mod nest;
pub mod obj;
pub mod pdf;
pub mod panel;
pub mod raskroy;
pub mod report;
pub mod view;

/// Writes a file whole or not at all: the data goes to a temporary file beside it that then replaces the old one,
/// so a crash or a full disk mid-save leaves the previous file intact.
pub fn write(path: impl AsRef<std::path::Path>, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    // Through a symlink to the file it points at, keeping the link.
    let path = std::fs::canonicalize(path.as_ref()).unwrap_or_else(|_| path.as_ref().to_path_buf());
    // /dev/null, /dev/stdout, a pipe: written straight, never replaced.
    if std::fs::metadata(&path).is_ok_and(|m| !m.is_file()) || path.starts_with("/dev") {
        return std::fs::write(&path, data);
    }
    let name = path.file_name().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no file name"))?;
    let tmp = path.with_file_name(format!(".{}.{}-{}.tmp", name.to_string_lossy(), std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let mut f = match std::fs::File::create(&tmp) {
        Ok(f) => f,
        // A writable file in a folder we may not add to (or a name too long for the extra suffix): written in place as before.
        Err(_) => return std::fs::write(&path, data),
    };
    let res = (|| {
        f.write_all(data.as_ref())?;
        f.sync_all()?;
        if let Ok(m) = std::fs::metadata(&path) {
            f.set_permissions(m.permissions())?;
        }
        drop(f);
        std::fs::rename(&tmp, &path)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

#[cfg(test)]
mod tests {
    /// The file is replaced whole, and no temporary file is left beside it.
    #[test]
    fn write_replaces_the_file() {
        let dir = std::env::temp_dir().join(format!("dowel-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.dcraft");
        super::write(&f, "first, longer").unwrap();
        super::write(&f, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "second");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        assert!(super::write(dir.join("missing/b"), "x").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
