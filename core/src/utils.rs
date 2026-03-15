use std::path::{Component, Path, PathBuf};

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
    Ok(if target.extension().is_some_and(|ex| ex == "slf") {
        target
    } else {
        let archive_name = archive_name(&source)?;
        target.join(archive_name).with_extension("slf")
    })
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
                normalized.clear();
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
    normalized.iter().collect()
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
            Component::CurDir => continue,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_safe_join_path_traversal() {
        let base = Path::new("/base");
        let untrusted = Path::new("../../etc/passwd");
        assert!(safe_join(base, untrusted).is_err())
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
        assert_eq!(normalize_path(Path::new("//a//b")), PathBuf::from("/a/b"))
    }

    #[test]
    fn test_normalize_path_current_dir() {
        assert_eq!(normalize_path(Path::new("./a/./b")), PathBuf::from("a/b"));
        assert_eq!(normalize_path(Path::new("//a//b")), PathBuf::from("/a/b"))
    }

    #[test]
    fn test_normalize_path_parent_dir() {
        assert_eq!(normalize_path(Path::new("a/b/../c")), PathBuf::from("a/c"));
        assert_eq!(normalize_path(Path::new("../a")), PathBuf::from("../a"))
    }
}
