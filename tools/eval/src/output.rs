use std::{
    fs, io,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

/// Prepare output beside its destination so committing uses a same-filesystem rename.
pub struct Output {
    destination: PathBuf,
    file: NamedTempFile,
}

fn validate(path: &Path) -> io::Result<Option<fs::Metadata>> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(metadata)),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a file", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

impl Output {
    pub fn new(path: &Path) -> io::Result<Self> {
        validate(path)?;
        // Resolve output symlinks to their target paths.
        if path.is_symlink() {
            let target = fs::read_link(path)?;
            let target = if target.is_absolute() {
                target
            } else {
                path.parent().unwrap_or(Path::new(".")).join(target)
            };
            return Self::new(&target);
        }
        let destination = if path.is_file() {
            path.canonicalize()?
        } else {
            path.to_owned()
        };
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let mut builder = tempfile::Builder::new();
        builder.prefix(".cinekey-eval-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(fs::Permissions::from_mode(0o666));
        }
        let file = builder.tempfile_in(parent)?;
        Ok(Self { destination, file })
    }

    pub fn file(&mut self) -> &mut fs::File {
        self.file.as_file_mut()
    }
}

pub fn commit(outputs: Vec<Output>) -> io::Result<()> {
    commit_with(outputs, |file, destination| {
        file.persist(destination)
            .map(|_| ())
            .map_err(|error| error.error)
    })
}

fn commit_with(
    outputs: Vec<Output>,
    mut install: impl FnMut(NamedTempFile, &Path) -> io::Result<()>,
) -> io::Result<()> {
    // Validate and retain the previous files before changing either report.
    let mut backups = Vec::with_capacity(outputs.len());
    for output in &outputs {
        let previous = if let Some(metadata) = validate(&output.destination)? {
            output
                .file
                .as_file()
                .set_permissions(metadata.permissions())?;
            let backup = NamedTempFile::new_in(output.file.path().parent().unwrap())?;
            fs::copy(&output.destination, backup.path())?;
            Some(backup)
        } else {
            None
        };
        output.file.as_file().sync_all()?;
        backups.push((output.destination.clone(), previous));
    }
    for (index, output) in outputs.into_iter().enumerate() {
        if let Err(error) = install(output.file, &output.destination) {
            let mut failures = Vec::new();
            for (destination, backup) in backups.into_iter().take(index).rev() {
                let restored = match backup {
                    Some(file) => match file.persist(&destination) {
                        Ok(_) => Ok(()),
                        Err(error) => {
                            let path = error.file.into_temp_path().keep()?;
                            Err(io::Error::other(format!(
                                "{}; previous output retained at {}",
                                error.error,
                                path.display()
                            )))
                        }
                    },
                    None => fs::remove_file(&destination),
                };
                if let Err(error) = restored {
                    failures.push(error.to_string());
                }
            }
            return if failures.is_empty() {
                Err(error)
            } else {
                Err(io::Error::other(format!(
                    "{error}; restoring previous outputs failed: {}",
                    failures.join("; ")
                )))
            };
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[cfg(unix)]
    #[test]
    fn committing_preserves_output_symlinks_and_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.json");
        let link = directory.path().join("summary.json");
        fs::write(&target, b"previous").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        symlink(&target, &link).unwrap();
        let mut output = Output::new(&link).unwrap();
        output.file().write_all(b"updated").unwrap();
        commit(vec![output]).unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&link).unwrap(), b"updated");
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
        let missing = directory.path().join("new.json");
        let missing_link = directory.path().join("new-link.json");
        symlink("new.json", &missing_link).unwrap();
        let mut output = Output::new(&missing_link).unwrap();
        output.file().write_all(b"new output").unwrap();
        commit(vec![output]).unwrap();
        assert!(
            fs::symlink_metadata(&missing_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&missing).unwrap(), b"new output");
    }

    #[test]
    fn failed_second_commit_restores_existing_and_removes_new_outputs() {
        for existing in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let first = directory.path().join("summary.json");
            let second = directory.path().join("mismatches.jsonl");
            if existing {
                fs::write(&first, b"previous summary").unwrap();
            }
            fs::write(&second, b"previous mismatches").unwrap();
            let mut outputs = vec![Output::new(&first).unwrap(), Output::new(&second).unwrap()];
            outputs[0].file().write_all(b"new summary").unwrap();
            outputs[1].file().write_all(b"new mismatches").unwrap();
            let mut count = 0;
            let result = commit_with(outputs, |file, destination| {
                count += 1;
                if count == 2 {
                    return Err(io::Error::other("injected commit failure"));
                }
                file.persist(destination)
                    .map(|_| ())
                    .map_err(|error| error.error)
            });
            assert!(result.is_err());
            assert_eq!(first.exists(), existing);
            if existing {
                assert_eq!(fs::read(&first).unwrap(), b"previous summary");
            }
            assert_eq!(fs::read(&second).unwrap(), b"previous mismatches");
            assert_eq!(
                fs::read_dir(directory.path()).unwrap().count(),
                1 + usize::from(existing)
            );
        }
    }
}
