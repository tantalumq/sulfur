use std::path::{Component, Path, PathBuf};

use walkdir::WalkDir;

use crate::{Error, Result};

#[allow(clippy::missing_errors_doc)]
pub fn extraction_path(source: &Path, target: &Path) -> Result<PathBuf> {
    let source = normalize_path(source);
    let target = normalize_path(target);

    if !source.exists() || !source.is_file() || source.extension().is_none_or(|ex| ex != "slf") {
        return Err(Error::Path(format!(
            "Invalid source destination at path: {}",
            source.display()
        )));
    }

    let extraction_path = if target.is_file() {
        return Err(Error::Path(format!(
            "Archive can't be unpacked into file at path: {}",
            target.display(),
        )));
    } else {
        let source_stem = source.file_stem().ok_or(Error::Path(format!(
            "Failed to get file stem from path: {}",
            source.display()
        )))?;
        target.join(source_stem)
    };

    Ok(extraction_path)
}

#[allow(clippy::missing_errors_doc)]
pub fn archive_path(source: &Path, target: &Path) -> Result<PathBuf> {
    let source = normalize_path(source);
    let target = normalize_path(target);

    if !source.exists() || (!source.is_file() && !source.is_dir()) {
        return Err(Error::Path(format!(
            "Invalid source destination at path: {}",
            source.display()
        )));
    }

    let target = if target == Path::new(".") {
        archive_name(&source)?
    } else {
        target
    };

    let resolved_path = if target.extension().is_some_and(|ex| ex == "slf") {
        target
    } else {
        target.with_extension("slf")
    };

    if resolved_path.is_dir() {
        return Err(Error::Path(format!(
            "target path is an existing directory: {}",
            resolved_path.display()
        )));
    }

    Ok(resolved_path)
}

fn archive_name(source: &Path) -> Result<PathBuf> {
    Ok(if source.is_file() {
        PathBuf::from(source.file_stem().ok_or(Error::Path(format!(
            "Failed to get file stem from path: {}",
            source.display()
        )))?)
    } else {
        PathBuf::from(source.file_name().ok_or(Error::Path(format!(
            "Failed to get directory name from path: {}",
            source.display()
        )))?)
    })
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(p) => {
                normalized.clear();
                normalized.push(Component::Prefix(p));
            }
            Component::RootDir => {
                normalized.retain(|c| matches!(c, Component::Prefix(_)));
                normalized.push(Component::RootDir);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = normalized.last() {
                    if let Component::Normal(_) = last {
                        normalized.pop();
                    }
                } else {
                    normalized.push(component);
                }
            }
            Component::Normal(_) => normalized.push(component),
        }
    }

    if normalized.is_empty() {
        PathBuf::from(".")
    } else {
        normalized.iter().collect()
    }
}

pub fn safe_join(base: &Path, untrusted: &Path) -> Result<PathBuf> {
    let mut sanitized = PathBuf::new();

    for component in untrusted.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                return Err(Error::Path(format!(
                    "absolute path detected {}",
                    untrusted.display()
                )));
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(Error::Path(format!(
                    "path traversal detected: {}",
                    untrusted.display()
                )));
            }
            Component::Normal(c) => sanitized.push(c),
        }
    }

    if sanitized.as_os_str().is_empty() {
        return Err(Error::Path(format!(
            "Path sanitized to nothing: {}",
            untrusted.display()
        )));
    }

    let result = base.join(sanitized);

    Ok(result)
}

pub fn collect_files(source: &Path) -> Result<Vec<(PathBuf, String)>> {
    let mut file_paths = Vec::new();

    for entry in WalkDir::new(source) {
        let entry = entry?;
        let path = entry.into_path();

        if path.is_symlink() {
            continue;
        }

        let relative_name = if source.is_file() {
            path.file_name()
        } else {
            path.strip_prefix(source).ok().map(Path::as_os_str)
        }
        .and_then(|s| s.to_str())
        .map(|s| s.replace('\\', "/"))
        .ok_or(Error::Path(format!(
            "can't get relative file name from {}",
            path.display(),
        )))?;

        if path.is_file() {
            file_paths.push((path, relative_name));
        }
    }

    Ok(file_paths)
}

#[must_use]
pub fn to_readable_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes > TB {
        let hundredths = (bytes * 100 + (TB / 2)) / TB;
        format!("{}.{:02} TB", hundredths / 100, hundredths % 100)
    } else if bytes > GB {
        let hundredths = (bytes * 100 + (GB / 2)) / GB;
        format!("{}.{:02} GB", hundredths / 100, hundredths % 100)
    } else if bytes > MB {
        let hundredths = (bytes * 100 + (MB / 2)) / MB;
        format!("{}.{:02} MB", hundredths / 100, hundredths % 100)
    } else if bytes > KB {
        let hundredths = (bytes * 100 + (KB / 2)) / KB;
        format!("{}.{:02} KB", hundredths / 100, hundredths % 100)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};
    use tempfile::tempdir;

    #[test]
    fn test_archive_path_to_file() -> Result<()> {
        let dir = tempdir()?;
        let source = dir.path().join("a");
        fs::File::create(&source)?;

        let target = dir.path().join("c.slf");

        assert_eq!(&archive_path(&source, &target)?, &dir.path().join("c.slf"));
        Ok(())
    }

    #[test]
    fn test_archive_path_to_file_with_dir() -> Result<()> {
        let dir = tempdir()?;
        let source = dir.path().join("a");
        fs::File::create(&source)?;

        let target = dir.path().join("c/b");

        assert_eq!(
            &archive_path(&source, &target)?,
            &dir.path().join("c/b.slf")
        );
        Ok(())
    }

    #[test]
    fn test_extraction_path_from_archive() -> Result<()> {
        let dir = tempdir()?;
        let source = dir.path().join("a.slf");
        fs::File::create(&source)?;

        let target = dir.path().join("b");

        assert_eq!(
            &extraction_path(&source, &target)?,
            &dir.path().join("b/a/")
        );
        Ok(())
    }

    #[test]
    fn test_extraction_path_from_archive_invalid_extension() -> Result<()> {
        let dir = tempdir()?;
        let source = dir.path().join("a.notslf");
        fs::File::create(&source)?;

        let target = dir.path().join("b");

        assert!(extraction_path(&source, &target).is_err());
        Ok(())
    }

    #[test]
    fn test_safe_join_path_traversal() {
        let base = Path::new("/base");
        let untrusted = Path::new("../../etc/passwd");
        assert!(safe_join(base, untrusted).is_err());
    }

    #[test]
    fn test_safe_join_allows_normal() -> Result<()> {
        let base = Path::new("/base");
        let untrusted = Path::new("images/photo.jpeg");
        assert_eq!(
            safe_join(base, untrusted)?,
            PathBuf::from("/base/images/photo.jpeg")
        );
        Ok(())
    }

    #[test]
    fn test_normalize_path_separators() {
        assert_eq!(normalize_path(Path::new("a//b")), PathBuf::from("a/b"));
        assert_eq!(normalize_path(Path::new("//a//b")), PathBuf::from("/a/b"));
    }

    #[test]
    fn test_normalize_path_current_dir() {
        assert_eq!(normalize_path(Path::new("./a/./b")), PathBuf::from("a/b"));
        assert_eq!(normalize_path(Path::new("//a//b")), PathBuf::from("/a/b"));
    }

    #[test]
    fn test_normalize_path_parent_dir() {
        assert_eq!(normalize_path(Path::new("a/b/../c")), PathBuf::from("a/c"));
        assert_eq!(normalize_path(Path::new("../a")), PathBuf::from("../a"));
    }
}
