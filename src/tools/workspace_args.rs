use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmptyArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PathArgs {
    pub path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WriteArgs {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub overwrite: bool,
    pub expected_hash: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EditArgs {
    pub path: String,
    pub edits: Vec<TextEdit>,
    pub expected_hash: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TextEdit {
    pub old_text: String,
    pub new_text: String,
    #[serde(default)]
    pub replace_all: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DirectoryArgs {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CopyArgs {
    pub source: String,
    pub destination: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MoveArgs {
    pub source: String,
    pub destination: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default, rename = "recursive")]
    pub _recursive: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PatchArgs {
    pub patch_content: String,
    #[serde(default = "workspace_root")]
    pub target_dir: String,
    #[serde(default)]
    pub expected_hashes: BTreeMap<String, String>,
}

fn workspace_root() -> String {
    ".".to_string()
}
