use super::{
    DEFAULT_MAX_FILE_SIZE, DEFAULT_MAX_LIST_ENTRIES, MAX_ALLOWED_FILE_LIMIT,
    MAX_ALLOWED_LIST_ENTRIES, ServerConfig,
};
use crate::protocol::ToolCallResult;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

mod search;
pub(super) use search::tool_search_files;

pub(super) fn tool_read_file(config: &ServerConfig, args: &serde_json::Value) -> ToolCallResult {
    match read_file(config, args) {
        Ok(output) => ToolCallResult::ok(output),
        Err(error) => ToolCallResult::error(error),
    }
}

fn read_file(config: &ServerConfig, args: &serde_json::Value) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(|value| value.as_str())
        .ok_or("Missing required parameter: 'path'")?;
    let relative = config.workspace.legacy_relative(path)?;
    if config.workspace.metadata(&relative)?.is_dir() {
        return Err(format!(
            "Path '{}' is a directory. Use list_directory to view its contents.",
            path
        ));
    }
    read_slice(config.workspace.open_file(&relative)?, args)
}

fn read_slice(file: File, args: &serde_json::Value) -> Result<String, String> {
    let total = file
        .metadata()
        .map_err(|error| format!("Cannot read file metadata: {}", error))?
        .len();
    let offset = args
        .get("offset")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    if (total == 0 && offset > 0) || (total > 0 && offset >= total) {
        return Ok(format!(
            "[Offset {} exceeds file length of {} bytes]",
            offset, total
        ));
    }
    let limit = bounded_argument(args, "limit", DEFAULT_MAX_FILE_SIZE, MAX_ALLOWED_FILE_LIMIT);
    let bytes = read_bytes(file, offset, limit)?;
    Ok(format_file_chunk(&bytes, offset, total))
}

fn read_bytes(mut file: File, offset: u64, limit: usize) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| format!("Seek error: {}", error))?;
    let mut buffer = Vec::new();
    file.take(limit as u64)
        .read_to_end(&mut buffer)
        .map_err(|error| format!("Read error: {}", error))?;
    Ok(buffer)
}

pub(super) fn bounded_argument(
    args: &serde_json::Value,
    key: &str,
    default: usize,
    maximum: usize,
) -> usize {
    args.get(key)
        .and_then(|value| value.as_u64())
        .unwrap_or(default as u64)
        .clamp(1, maximum as u64) as usize
}

fn format_file_chunk(buffer: &[u8], offset: u64, total: u64) -> String {
    if buffer.contains(&0) {
        return format!(
            "[Binary file detected (size: {} bytes). Raw binary content is omitted.]",
            total
        );
    }
    let mut output = String::from_utf8_lossy(buffer).into_owned();
    let end = offset + buffer.len() as u64;
    if end < total {
        output.push_str(&format!(
            "\n\n[File truncated: showing bytes {}-{} of {}. Use offset={} to read next chunk]",
            offset, end, total, end
        ));
    }
    output
}

pub(super) fn tool_list_directory(
    config: &ServerConfig,
    args: &serde_json::Value,
) -> ToolCallResult {
    match list_directory(config, args) {
        Ok(output) => ToolCallResult::ok(output),
        Err(error) => ToolCallResult::error(error),
    }
}

pub(super) fn directory_path(
    config: &ServerConfig,
    args: &serde_json::Value,
) -> Result<PathBuf, String> {
    let path = args
        .get("path")
        .and_then(|value| value.as_str())
        .unwrap_or(".");
    let relative = config.workspace.legacy_relative(path)?;
    if !config.workspace.metadata(&relative)?.is_dir() {
        return Err(format!("Path '{}' is not a directory.", path));
    }
    Ok(relative)
}

fn list_directory(config: &ServerConfig, args: &serde_json::Value) -> Result<String, String> {
    let target = directory_path(config, args)?;
    let depth = bounded_argument(args, "depth", 1, 5);
    let limit = bounded_argument(
        args,
        "max_entries",
        DEFAULT_MAX_LIST_ENTRIES,
        MAX_ALLOWED_LIST_ENTRIES,
    );
    let walk = config.workspace.walk(&target, depth, limit)?;
    let mut lines: Vec<String> = walk
        .entries
        .iter()
        .map(|entry| {
            let relative = entry.path.strip_prefix(&target).unwrap_or(&entry.path);
            listing_line(relative, entry.is_dir, entry.size)
        })
        .collect();
    lines.sort();
    Ok(format_listing(lines, walk.truncated, limit))
}

fn listing_line(path: &Path, is_dir: bool, size: u64) -> String {
    if is_dir {
        format!("[DIR]  {}/", path.display())
    } else {
        format!("[FILE] {} ({} bytes)", path.display(), size)
    }
}

fn format_listing(lines: Vec<String>, truncated: bool, limit: usize) -> String {
    if lines.is_empty() {
        return "Directory is empty or all contents are ignored/denylisted.".to_string();
    }
    let truncated = truncated || lines.len() >= limit;
    let mut output = lines.join("\n");
    if truncated {
        output.push_str(&format!(
            "\n\n[Directory listing truncated at {} entries. Use deeper path or increase max_entries]", limit
        ));
    }
    output
}
