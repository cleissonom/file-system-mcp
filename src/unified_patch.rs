//! Text unified diffs with optional ordinary Git framing and index metadata.
//!
//! Paths use unquoted UTF-8 names, optional tab-separated timestamps, and standard
//! a/ and b/ prefixes. Renames, copies, binary patches, symlinks, and mode changes
//! are rejected. Regular-file creation/deletion modes are validated as metadata;
//! applying a patch only produces text, leaving filesystem policy to the caller.

use std::collections::HashSet;
use std::path::PathBuf;

pub const MAX_PATCH_BYTES: usize = 1024 * 1024;
pub const MAX_PATCH_FILES: usize = 100;
const NO_NEWLINE: &str = "\\ No newline at end of file";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Create,
    Modify,
    Delete,
}

#[derive(Debug)]
pub struct PatchChange {
    pub path: PathBuf,
    pub kind: ChangeKind,
    patch_content: String,
}

impl PatchChange {
    pub fn apply_to(&self, before: &str) -> Result<String, String> {
        if self.kind == ChangeKind::Create && !before.is_empty() {
            return Err("A file creation patch requires an empty base".into());
        }
        let patch = diffy::Patch::from_str(&self.patch_content).map_err(|e| e.to_string())?;
        let after =
            diffy::apply(before, &patch).map_err(|e| format!("Patch does not match: {e}"))?;
        if self.kind == ChangeKind::Delete && !after.is_empty() {
            return Err("A file deletion patch must remove all file content".into());
        }
        Ok(after)
    }
}

pub fn parse(content: &str) -> Result<Vec<PatchChange>, String> {
    if content.len() > MAX_PATCH_BYTES || content.contains('\0') {
        return Err("Patch exceeds the 1 MiB limit or contains binary NUL data".into());
    }
    let lines: Vec<_> = content.split_inclusive('\n').collect();
    let (mut cursor, mut changes, mut paths) = (0, Vec::new(), HashSet::new());
    while cursor < lines.len() {
        if changes.len() == MAX_PATCH_FILES {
            return Err("Patch exceeds the 100-file limit".into());
        }
        let change = take_change(&lines, &mut cursor)?;
        if !paths.insert(change.path.clone()) {
            return Err("Patch contains duplicate file targets".into());
        }
        changes.push(change);
    }
    if changes.is_empty() {
        Err("Patch must contain at least one file change".into())
    } else {
        Ok(changes)
    }
}

#[derive(Default)]
struct Metadata<'a> {
    framing: Option<&'a str>,
    operation: Option<ChangeKind>,
}

fn take_change(lines: &[&str], cursor: &mut usize) -> Result<PatchChange, String> {
    let metadata = take_metadata(lines, cursor)?;
    let start = *cursor;
    let old = take_filename(lines, cursor, "--- ")?;
    let new = take_filename(lines, cursor, "+++ ")?;
    let (path, kind) = target(old, new)?;
    validate_metadata(&metadata, &path, kind)?;
    take_hunks(lines, cursor)?;
    let patch_content = lines[start..*cursor].concat();
    let patch = diffy::Patch::from_str(&patch_content).map_err(|e| e.to_string())?;
    if !patch
        .hunks()
        .iter()
        .flat_map(|hunk| hunk.lines())
        .any(is_change)
    {
        return Err("Patch must contain an insertion or deletion".into());
    }
    Ok(PatchChange {
        path,
        kind,
        patch_content,
    })
}

fn is_change(line: &diffy::Line<'_, str>) -> bool {
    matches!(line, diffy::Line::Insert(_) | diffy::Line::Delete(_))
}

fn take_metadata<'a>(lines: &[&'a str], cursor: &mut usize) -> Result<Metadata<'a>, String> {
    let mut metadata = Metadata::default();
    if let Some(framing) = lines
        .get(*cursor)
        .filter(|line| line.starts_with("diff --git "))
    {
        metadata.framing = Some(without_newline(framing));
        *cursor += 1;
    }
    while let Some(line) = lines.get(*cursor).filter(|line| !line.starts_with("--- ")) {
        if metadata.framing.is_none() {
            return Err("Expected a unified diff file header".into());
        }
        parse_metadata_line(without_newline(line), &mut metadata)?;
        *cursor += 1;
    }
    Ok(metadata)
}

fn parse_metadata_line(line: &str, metadata: &mut Metadata<'_>) -> Result<(), String> {
    if let Some(index) = line.strip_prefix("index ") {
        return validate_index(index);
    }
    for (prefix, kind) in [
        ("new file mode ", ChangeKind::Create),
        ("deleted file mode ", ChangeKind::Delete),
    ] {
        if let Some(mode) = line.strip_prefix(prefix) {
            if !regular_mode(mode) || metadata.operation.replace(kind).is_some() {
                return Err("Invalid or unsupported file mode metadata".into());
            }
            return Ok(());
        }
    }
    Err("Unsupported Git metadata; only text file changes are accepted".into())
}

fn validate_index(index: &str) -> Result<(), String> {
    let fields: Vec<_> = index.split(' ').collect();
    let Some((old, new)) = fields[0].split_once("..") else {
        return Err("Invalid Git index metadata".into());
    };
    let valid_hash =
        |hash: &str| !hash.is_empty() && hash.bytes().all(|byte| byte.is_ascii_hexdigit());
    if fields.len() > 2
        || !valid_hash(old)
        || !valid_hash(new)
        || (fields.len() == 2 && !regular_mode(fields[1]))
    {
        return Err("Invalid Git index or unsupported file mode metadata".into());
    }
    Ok(())
}

fn regular_mode(mode: &str) -> bool {
    matches!(mode, "100644" | "100755")
}

fn validate_metadata(
    metadata: &Metadata<'_>,
    path: &std::path::Path,
    kind: ChangeKind,
) -> Result<(), String> {
    if metadata
        .operation
        .is_some_and(|operation| operation != kind)
    {
        return Err("File mode metadata does not agree with the file headers".into());
    }
    if let Some(framing) = metadata.framing {
        let name = path.to_str().ok_or("Patch filename must be UTF-8")?;
        if framing != format!("diff --git a/{name} b/{name}") {
            return Err("Git framing must use the same unquoted target as the file headers".into());
        }
    }
    Ok(())
}

fn take_filename<'a>(
    lines: &[&'a str],
    cursor: &mut usize,
    prefix: &str,
) -> Result<&'a str, String> {
    let line = lines
        .get(*cursor)
        .ok_or("Missing unified diff file header")?;
    let filename = without_newline(line)
        .strip_prefix(prefix)
        .ok_or("Invalid unified diff file header")?;
    *cursor += 1;
    Ok(filename.split_once('\t').map_or(filename, |(name, _)| name))
}

fn without_newline(line: &str) -> &str {
    line.strip_suffix('\n').unwrap_or(line)
}

fn target(old: &str, new: &str) -> Result<(PathBuf, ChangeKind), String> {
    match (old, new) {
        ("/dev/null", "/dev/null") => Err("Both patch filenames cannot be /dev/null".into()),
        ("/dev/null", name) => Ok((
            safe_path(name.strip_prefix("b/").unwrap_or(name))?,
            ChangeKind::Create,
        )),
        (name, "/dev/null") => Ok((
            safe_path(name.strip_prefix("a/").unwrap_or(name))?,
            ChangeKind::Delete,
        )),
        (old, new) if old == new => Ok((safe_path(old)?, ChangeKind::Modify)),
        (old, new) => matching_target(old, new),
    }
}

fn matching_target(old: &str, new: &str) -> Result<(PathBuf, ChangeKind), String> {
    let old = safe_path(old.strip_prefix("a/").unwrap_or(old))?;
    let new = safe_path(new.strip_prefix("b/").unwrap_or(new))?;
    if old != new {
        return Err("Renames and mismatched patch filenames are not supported".into());
    }
    Ok((old, ChangeKind::Modify))
}

fn safe_path(name: &str) -> Result<PathBuf, String> {
    let invalid_character = name
        .chars()
        .any(|character| character.is_control() || matches!(character, '\\' | '"'));
    let windows_drive =
        name.as_bytes().get(1) == Some(&b':') && name.as_bytes()[0].is_ascii_alphabetic();
    if name.is_empty() || name.starts_with('/') || windows_drive || invalid_character {
        return Err("Patch filenames must be unquoted relative paths".into());
    }
    if name
        .split('/')
        .any(|component| matches!(component, "" | "." | ".."))
    {
        return Err("Patch filenames cannot contain traversal or empty components".into());
    }
    Ok(PathBuf::from(name))
}

fn take_hunks(lines: &[&str], cursor: &mut usize) -> Result<(), String> {
    let mut count = 0;
    while lines
        .get(*cursor)
        .is_some_and(|line| line.starts_with("@@ "))
    {
        take_hunk(lines, cursor)?;
        count += 1;
    }
    if count == 0 {
        Err("File patch must contain at least one hunk".into())
    } else {
        Ok(())
    }
}

fn take_hunk(lines: &[&str], cursor: &mut usize) -> Result<(), String> {
    let (mut old, mut new) = hunk_counts(lines[*cursor])?;
    *cursor += 1;
    while old != 0 || new != 0 {
        let line = lines
            .get(*cursor)
            .ok_or("Hunk ended before its declared line counts")?;
        let (old_count, new_count) = line_counts(line)?;
        old = old
            .checked_sub(old_count)
            .ok_or("Hunk exceeds its old line count")?;
        new = new
            .checked_sub(new_count)
            .ok_or("Hunk exceeds its new line count")?;
        *cursor += 1;
    }
    while lines
        .get(*cursor)
        .is_some_and(|line| without_newline(line) == NO_NEWLINE)
    {
        *cursor += 1;
    }
    Ok(())
}

fn line_counts(line: &str) -> Result<(usize, usize), String> {
    match line.as_bytes().first() {
        Some(b' ' | b'\n') => Ok((1, 1)),
        Some(b'-') => Ok((1, 0)),
        Some(b'+') => Ok((0, 1)),
        Some(b'\\') if without_newline(line) == NO_NEWLINE => Ok((0, 0)),
        _ => Err("Invalid unified diff hunk body".into()),
    }
}

fn hunk_counts(line: &str) -> Result<(usize, usize), String> {
    let header = line.strip_prefix("@@ ").ok_or("Invalid hunk header")?;
    let (ranges, _) = header.split_once(" @@").ok_or("Unterminated hunk header")?;
    let (old, new) = ranges.split_once(' ').ok_or("Missing hunk ranges")?;
    Ok((range_count(old, '-')?, range_count(new, '+')?))
}

fn range_count(range: &str, prefix: char) -> Result<usize, String> {
    let range = range.strip_prefix(prefix).ok_or("Invalid hunk range")?;
    let (start, count) = range.split_once(',').unwrap_or((range, "1"));
    let start: u32 = start.parse().map_err(|_| "Invalid hunk line number")?;
    let count: u32 = count.parse().map_err(|_| "Invalid hunk line count")?;
    if (start == 0 && count != 0) || start.checked_add(count).is_none() {
        return Err("Invalid or overflowing hunk range".into());
    }
    Ok(count as usize)
}

#[cfg(test)]
#[path = "unified_patch_tests.rs"]
mod tests;
