use crate::support::McpClient;
use serde_json::{Value, json};
use std::fs;

mod patches;
mod plans;
mod reads;

pub(crate) fn check_tools(
    client: &mut McpClient,
    temp: &std::path::Path,
    repo_dir: &std::path::Path,
) {
    reads::check_reads(client);
    plans::check_plans(client);
    patches::check_patches(client, temp, repo_dir);
}
