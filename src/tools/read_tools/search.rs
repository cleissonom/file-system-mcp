use super::{bounded_argument, directory_path};
use crate::protocol::ToolCallResult;
use crate::tools::{DEFAULT_MAX_SEARCH_RESULTS, MAX_ALLOWED_SEARCH_RESULTS, ServerConfig};
use crate::workspace::WalkEntry;
use globset::{Glob, GlobMatcher};
use regex::{Regex, RegexBuilder};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

pub(in crate::tools) fn tool_search_files(
    config: &ServerConfig,
    args: &serde_json::Value,
) -> ToolCallResult {
    match search_files(config, args) {
        Ok(output) => ToolCallResult::ok(output),
        Err(error) => ToolCallResult::error(error),
    }
}

struct Search {
    matcher: Regex,
    glob: Option<GlobMatcher>,
    limit: usize,
}

fn search_files(config: &ServerConfig, args: &serde_json::Value) -> Result<String, String> {
    let search = Search {
        matcher: query_matcher(args)?,
        glob: file_matcher(args)?,
        limit: bounded_argument(
            args,
            "max_results",
            DEFAULT_MAX_SEARCH_RESULTS,
            MAX_ALLOWED_SEARCH_RESULTS,
        ),
    };
    let target = directory_path(config, args)?;
    let walk = config.workspace.walk(&target, usize::MAX, usize::MAX)?;
    let matches = find_matches(config, &walk.entries, &target, &search);
    Ok(format_matches(matches, walk.truncated, search.limit))
}

fn find_matches(
    config: &ServerConfig,
    entries: &[WalkEntry],
    target: &Path,
    search: &Search,
) -> Vec<String> {
    let mut matches = Vec::new();
    for entry in entries {
        if !entry.is_dir
            && entry.size <= 5 * 1024 * 1024
            && matches_file(config, &entry.path, target, search.glob.as_ref())
        {
            search_file(config, &entry.path, search, &mut matches);
        }
        if matches.len() >= search.limit {
            break;
        }
    }
    matches
}

fn query_matcher(args: &serde_json::Value) -> Result<Regex, String> {
    let query = args
        .get("query")
        .and_then(|value| value.as_str())
        .filter(|query| !query.trim().is_empty())
        .ok_or("Missing or empty required parameter: 'query'")?;
    let pattern = if args
        .get("is_regex")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        query.to_string()
    } else {
        regex::escape(query)
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
        .map_err(|error| format!("Invalid regex query: {}", error))
}

fn file_matcher(args: &serde_json::Value) -> Result<Option<GlobMatcher>, String> {
    args.get("file_pattern")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(|pattern| {
            Glob::new(pattern)
                .map(|glob| glob.compile_matcher())
                .map_err(|error| format!("Invalid file_pattern: {}", error))
        })
        .transpose()
}

fn matches_file(
    config: &ServerConfig,
    path: &Path,
    target: &Path,
    glob: Option<&GlobMatcher>,
) -> bool {
    let Some(glob) = glob else {
        return true;
    };
    let Some(filename) = path.file_name() else {
        return false;
    };
    glob.is_match(filename)
        || glob.is_match(path)
        || glob.is_match(config.workspace.root().join(path))
        || glob.is_match(path.strip_prefix(target).unwrap_or(path))
}

fn search_file(config: &ServerConfig, path: &Path, search: &Search, matches: &mut Vec<String>) {
    let Some(reader) = search_reader(config, path) else {
        return;
    };
    // Bound reads even if another editor grows the file after its metadata check.
    for (index, line) in reader.take(5 * 1024 * 1024).lines().enumerate() {
        let Ok(line) = line else {
            break;
        };
        if search.matcher.is_match(&line) {
            matches.push(format!(
                "{}:{}: {}",
                path.display(),
                index + 1,
                clipped_line(&line)
            ));
            if matches.len() >= search.limit {
                break;
            }
        }
    }
}

fn search_reader(config: &ServerConfig, path: &Path) -> Option<BufReader<std::fs::File>> {
    let file = config.workspace.open_file(path).ok()?;
    if file.metadata().ok()?.len() > 5 * 1024 * 1024 {
        return None;
    }
    let mut reader = BufReader::new(file);
    text_reader(&mut reader).then_some(reader)
}

fn text_reader(reader: &mut BufReader<std::fs::File>) -> bool {
    let mut head = [0u8; 1024];
    let Ok(length) = reader.read(&mut head) else {
        return false;
    };
    !head[..length].contains(&0) && reader.seek(SeekFrom::Start(0)).is_ok()
}

fn clipped_line(line: &str) -> String {
    if line.len() <= 200 {
        return line.to_string();
    }
    let mut end = 200;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &line[..end])
}

fn format_matches(matches: Vec<String>, tree_truncated: bool, limit: usize) -> String {
    let at_limit = matches.len() >= limit;
    let mut output = if matches.is_empty() {
        "No matching lines found.".to_string()
    } else {
        matches.join("\n")
    };
    if at_limit {
        output.push_str(&format!(
            "\n\n[Search results truncated at {} matches. Narrow your query or path]",
            limit
        ));
    } else if tree_truncated {
        output.push_str("\n\n[Search traversal truncated by workspace limits. Narrow your path]");
    }
    output
}
