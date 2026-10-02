use super::boundary::{Entry, duplicate, open_directory, require_child, validate_relative};
use super::{MAX_COPY_BYTES, MAX_TREE_DEPTH, MAX_TREE_ENTRIES, WalkEntry, WalkResult, Workspace};
use rustix::fd::OwnedFd;
use rustix::fs::{self, AtFlags, FileType, Mode, RenameFlags};
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;

impl Workspace {
    pub fn create_directory(&self, path: &Path, recursive: bool) -> Result<(), String> {
        let path = validate_relative(path)?;
        require_child(&path)?;
        self.check_policy(&path, true)?;
        self.check_existing_ancestors(&path)?;
        if !recursive {
            return self.create_one(&path);
        }
        let mut prefix = PathBuf::new();
        for component in path.components() {
            prefix.push(component);
            self.check_policy(&prefix, true)?;
            if self.exists(&prefix)? {
                self.directory(&prefix)?;
            } else {
                self.create_one(&prefix)?;
            }
        }
        Ok(())
    }

    fn create_one(&self, path: &Path) -> Result<(), String> {
        self.check_policy(path, true)?;
        let entry = self.entry(path)?;
        fs::mkdirat(&entry.parent, &entry.name, Mode::from_bits_truncate(0o755))
            .map_err(|e| format!("Cannot create directory '{}': {e}", path.display()))
    }

    pub fn create_parent_directory(&self, path: &Path) -> Result<(), String> {
        let path = validate_relative(path)?;
        require_child(&path)?;
        self.check_policy(&path, false)?;
        let parent = path.parent().unwrap_or(Path::new(""));
        if parent.as_os_str().is_empty() {
            return Ok(());
        }
        self.create_directory(parent, true)
    }

    pub fn walk(
        &self,
        path: &Path,
        max_depth: usize,
        max_entries: usize,
    ) -> Result<WalkResult, String> {
        self.directory(path)?;
        let mut result = WalkResult {
            entries: Vec::new(),
            truncated: false,
        };
        self.scan(
            path,
            max_depth.min(MAX_TREE_DEPTH),
            max_entries.min(MAX_TREE_ENTRIES),
            false,
            &mut result,
        )?;
        Ok(result)
    }

    fn scan(
        &self,
        path: &Path,
        depth: usize,
        limit: usize,
        strict: bool,
        result: &mut WalkResult,
    ) -> Result<(), String> {
        let fd = self.directory(path)?;
        for name in directory_names(&fd)? {
            if result.entries.len() >= limit {
                result.truncated = true;
                break;
            }
            let child = path.join(name);
            match self.scan_entry(&child, depth, limit, strict, result) {
                Ok(()) => {}
                Err(error) if strict => return Err(error),
                Err(_) => continue,
            }
        }
        Ok(())
    }

    fn scan_entry(
        &self,
        child: &Path,
        depth: usize,
        limit: usize,
        strict: bool,
        result: &mut WalkResult,
    ) -> Result<(), String> {
        let metadata = self.metadata(child)?;
        let is_dir = metadata.is_dir();
        result.entries.push(WalkEntry {
            path: child.to_path_buf(),
            is_dir,
            size: metadata.len(),
        });
        if is_dir && depth > 1 {
            self.scan(child, depth - 1, limit, strict, result)?;
        } else if is_dir && strict && !directory_names(&self.directory(child)?)?.is_empty() {
            return Err("Directory exceeds traversal depth safety limit".into());
        }
        Ok(())
    }

    fn inventory(&self, path: &Path) -> Result<Vec<WalkEntry>, String> {
        let mut result = WalkResult {
            entries: Vec::new(),
            truncated: false,
        };
        if self.metadata(path)?.is_dir() {
            self.scan(path, MAX_TREE_DEPTH, MAX_TREE_ENTRIES, true, &mut result)?;
        }
        if result.truncated {
            return Err("Directory exceeds entry-count safety limit".into());
        }
        Ok(result.entries)
    }

    pub fn copy(
        &self,
        source: &Path,
        destination: &Path,
        overwrite: bool,
        recursive: bool,
    ) -> Result<(), String> {
        self.validate_transfer(source, destination)?;
        let source_metadata = self.metadata(source)?;
        let destination_entry = self.entry(destination)?;
        if !source_metadata.is_dir() {
            return self
                .copy_file(source, &destination_entry, overwrite, MAX_COPY_BYTES)
                .map(|_| ());
        }
        if !recursive {
            return Err("Directory copy requires recursive: true".into());
        }
        let entries = self.inventory(source)?;
        self.validate_copy_destination(source, destination, &entries)?;
        if destination_entry.stat()?.is_some() {
            return Err("Directory destination already exists; choose a new destination".into());
        }
        self.copy_directory(source, &destination_entry, &entries)
    }

    fn validate_transfer(&self, source: &Path, destination: &Path) -> Result<(), String> {
        require_child(source)?;
        require_child(destination)?;
        let source = self.actual_path(source)?;
        let destination = self.actual_path(destination)?;
        if destination.starts_with(&source) || source.starts_with(&destination) {
            return Err(
                "Source and destination must be distinct and cannot contain one another".into(),
            );
        }
        Ok(())
    }

    fn validate_copy_destination(
        &self,
        source: &Path,
        destination: &Path,
        entries: &[WalkEntry],
    ) -> Result<(), String> {
        entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .try_fold(0u64, |total, entry| {
                total
                    .checked_add(entry.size)
                    .filter(|size| *size <= MAX_COPY_BYTES as u64)
                    .ok_or_else(|| "Directory copy exceeds safety limit".to_string())
            })?;
        self.check_policy(destination, true)?;
        for entry in entries {
            let child = destination.join(entry.path.strip_prefix(source).unwrap());
            self.check_policy(&child, entry.is_dir)?;
        }
        Ok(())
    }

    pub fn move_path(
        &self,
        source: &Path,
        destination: &Path,
        overwrite: bool,
    ) -> Result<(), String> {
        self.validate_transfer(source, destination)?;
        let source_entry = self.entry(source)?;
        let metadata = source_entry.metadata()?;
        let entries = self.inventory(source)?;
        self.check_policy(destination, metadata.is_dir())?;
        for entry in entries {
            self.check_policy(
                &destination.join(entry.path.strip_prefix(source).unwrap()),
                entry.is_dir,
            )?;
        }
        let destination_entry = self.entry(destination)?;
        check_move_destination(&metadata, &destination_entry)?;
        rename(&source_entry, &destination_entry, overwrite)
    }

    pub fn delete(&self, path: &Path, recursive: bool) -> Result<(), String> {
        require_child(path)?;
        let entry = self.entry(path)?;
        let metadata = entry.metadata()?;
        if !metadata.is_dir() {
            return unlink(&entry, false);
        }
        if !recursive {
            return unlink(&entry, true);
        }
        let entries = self.inventory(path)?;
        let mut removed = 0;
        for child in entries.iter().rev() {
            let result = self
                .entry(&child.path)
                .and_then(|entry| unlink(&entry, child.is_dir));
            result.map_err(|error| {
                format!("Deletion failed after removing {removed} entries: {error}")
            })?;
            removed += 1;
        }
        unlink(&entry, true)
            .map_err(|error| format!("Deletion failed after removing {removed} entries: {error}"))
    }

    pub fn delete_checked(&self, path: &Path, expected_hash: &str) -> Result<(), String> {
        let entry = self.entry(path)?;
        self.check_hash(&entry, Some(expected_hash))?;
        if !entry.metadata()?.is_file() {
            return Err("Expected a regular file".into());
        }
        unlink(&entry, false)
    }
}

fn directory_names(fd: &OwnedFd) -> Result<Vec<OsString>, String> {
    let mut names = Vec::new();
    let directory = fs::Dir::read_from(fd).map_err(|e| e.to_string())?;
    for entry in directory {
        let entry = entry.map_err(|e| e.to_string())?;
        let bytes = entry.file_name().to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        if names.len() >= MAX_TREE_ENTRIES {
            return Err("Directory exceeds entry-count safety limit".into());
        }
        names.push(std::ffi::OsStr::from_bytes(bytes).to_os_string());
    }
    names.sort();
    Ok(names)
}

fn unlink(entry: &Entry, directory: bool) -> Result<(), String> {
    let flags = if directory {
        AtFlags::REMOVEDIR
    } else {
        AtFlags::empty()
    };
    fs::unlinkat(&entry.parent, &entry.name, flags)
        .map_err(|e| format!("Cannot delete '{}': {e}", entry.path.display()))
}

fn check_move_destination(source: &std::fs::Metadata, destination: &Entry) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let Some(stat) = destination.stat()? else {
        return Ok(());
    };
    if FileType::from_raw_mode(stat.st_mode) == FileType::Directory {
        return Err("Existing directory destinations cannot be overwritten".into());
    }
    if source.dev() == stat.st_dev as u64 && source.ino() == stat.st_ino {
        return Err(
            "Source and destination are hard links to the same file; rename would not move it"
                .into(),
        );
    }
    Ok(())
}

fn rename(source: &Entry, destination: &Entry, overwrite: bool) -> Result<(), String> {
    if overwrite {
        fs::renameat(
            &source.parent,
            &source.name,
            &destination.parent,
            &destination.name,
        )
    } else {
        fs::renameat_with(
            &source.parent,
            &source.name,
            &destination.parent,
            &destination.name,
            RenameFlags::NOREPLACE,
        )
    }
    .map_err(|e| format!("Cannot move '{}': {e}", source.path.display()))
}

impl Workspace {
    fn copy_directory(
        &self,
        source: &Path,
        destination: &Entry,
        entries: &[WalkEntry],
    ) -> Result<(), String> {
        let temporary = super::atomic::temporary_name();
        fs::mkdirat(
            &destination.parent,
            &temporary,
            Mode::from_bits_truncate(0o700),
        )
        .map_err(|e| e.to_string())?;
        let fd = match open_directory(&destination.parent, Path::new(&temporary)) {
            Ok(fd) => fd,
            Err(error) => {
                let _ = fs::unlinkat(&destination.parent, &temporary, AtFlags::REMOVEDIR);
                return Err(error);
            }
        };
        let copied = self
            .populate_copy(source, &fd, entries)
            .and_then(|()| self.preserve_directory_modes(source, &fd, entries));
        if let Err(error) = copied {
            cleanup_directory(&fd);
            let _ = fs::unlinkat(&destination.parent, &temporary, AtFlags::REMOVEDIR);
            return Err(error);
        }
        let published = fs::renameat_with(
            &destination.parent,
            &temporary,
            &destination.parent,
            &destination.name,
            RenameFlags::NOREPLACE,
        );
        if let Err(error) = published {
            cleanup_directory(&fd);
            let _ = fs::unlinkat(&destination.parent, &temporary, AtFlags::REMOVEDIR);
            return Err(format!("Cannot publish directory copy: {error}"));
        }
        Ok(())
    }

    fn populate_copy(
        &self,
        source: &Path,
        directory: &OwnedFd,
        entries: &[WalkEntry],
    ) -> Result<(), String> {
        let mut remaining = MAX_COPY_BYTES;
        for child in entries {
            let relative = child.path.strip_prefix(source).unwrap();
            let entry = raw_entry(directory, relative)?;
            if child.is_dir {
                fs::mkdirat(&entry.parent, &entry.name, Mode::from_bits_truncate(0o755))
                    .map_err(|e| e.to_string())?;
            } else {
                remaining -= self.copy_file(&child.path, &entry, false, remaining)?;
            }
        }
        Ok(())
    }

    fn preserve_directory_modes(
        &self,
        source: &Path,
        directory: &OwnedFd,
        entries: &[WalkEntry],
    ) -> Result<(), String> {
        for child in entries.iter().rev().filter(|entry| entry.is_dir) {
            let entry = raw_entry(directory, child.path.strip_prefix(source).unwrap())?;
            let fd = open_directory(&entry.parent, Path::new(&entry.name))?;
            self.copy_directory_mode(&child.path, &fd)?;
        }
        self.copy_directory_mode(source, directory)
    }

    fn copy_directory_mode(&self, source: &Path, destination: &OwnedFd) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let mode = self.metadata(source)?.permissions().mode() & 0o777;
        fs::fchmod(destination, Mode::from_bits_truncate(mode as _)).map_err(|e| e.to_string())
    }
}

fn raw_entry(directory: &OwnedFd, path: &Path) -> Result<Entry, String> {
    let mut parent = duplicate(directory)?;
    for component in path.parent().unwrap_or(Path::new("")).components() {
        parent = open_directory(&parent, Path::new(component.as_os_str()))?;
    }
    Ok(Entry {
        parent,
        name: path.file_name().unwrap().to_os_string(),
        path: path.to_path_buf(),
    })
}

fn cleanup_directory(directory: &OwnedFd) {
    let _ = fs::fchmod(directory, Mode::from_bits_truncate(0o700));
    let Ok(names) = directory_names(directory) else {
        return;
    };
    for name in names {
        if let Ok(fd) = open_directory(directory, Path::new(&name)) {
            cleanup_directory(&fd);
            let _ = fs::unlinkat(directory, &name, AtFlags::REMOVEDIR);
        } else {
            let _ = fs::unlinkat(directory, &name, AtFlags::empty());
        }
    }
}
