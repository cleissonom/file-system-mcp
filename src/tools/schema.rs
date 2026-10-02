use super::*;

pub fn get_available_tools() -> Vec<Tool> {
    let mut tools = vec![
        Tool {
            name: "read_file",
            annotations: crate::protocol::ToolAnnotations {
                read_only_hint: true,
                destructive_hint: false,
                open_world_hint: false,
            },
            description: "Read file contents safely from the workspace. Subject to gitignore rules, hard denylist for secrets/credentials (.env*, *.pem, *.key, .git/, .ssh, .aws, bastion.sh), and read size limits.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file from workspace root."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Optional byte offset to start reading from (default: 0)."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Optional maximum number of bytes to read (default: 25000, max: 100000)."
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "list_directory",
            annotations: crate::protocol::ToolAnnotations {
                read_only_hint: true,
                destructive_hint: false,
                open_world_hint: false,
            },
            description: "List directory contents safely from the workspace. Automatically excludes gitignored files and denylisted secrets across all repositories.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to directory from workspace root (default: '.' for root)."
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Maximum recursion depth (default: 1 for immediate children, max: 5)."
                    },
                    "max_entries": {
                        "type": "integer",
                        "description": "Maximum number of entries to return (default: 200, max: 1000)."
                    }
                }
            }),
        },
        Tool {
            name: "search_files",
            annotations: crate::protocol::ToolAnnotations {
                read_only_hint: true,
                destructive_hint: false,
                open_world_hint: false,
            },
            description: "Search file contents in the workspace using text or regular expressions, strictly respecting gitignore and security denylists.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The text pattern or regular expression to search for in file contents."
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional relative subdirectory to restrict the search to (default: '.')."
                    },
                    "is_regex": {
                        "type": "boolean",
                        "description": "Whether the query is a regular expression (default: false)."
                    },
                    "file_pattern": {
                        "type": "string",
                        "description": "Optional glob pattern to filter files by filename (e.g. '*.rs', '*.py', '*.json')."
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum matching lines to return (default: 50, max: 200)."
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "write_plan_file",
            annotations: crate::protocol::ToolAnnotations {
                read_only_hint: false,
                destructive_hint: true,
                open_world_hint: false,
            },
            description: "Saves an architectural or engineering implementation plan markdown file into the dedicated plans/ directory.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "filename": {
                        "type": "string",
                        "description": "Filename or relative subpath inside the plans/ directory (e.g. 'parser-scalability.plan.md', must end with .md or .markdown)."
                    },
                    "content": {
                        "type": "string",
                        "description": "Markdown content of the plan (max 500 KB)."
                    },
                    "overwrite": {
                        "type": "boolean",
                        "description": "Whether to overwrite an existing plan file if it already exists (default: false)."
                    }
                },
                "required": ["filename", "content"]
            }),
        },
        Tool {
            name: "write_patch_file",
            annotations: crate::protocol::ToolAnnotations {
                read_only_hint: false,
                destructive_hint: true,
                open_world_hint: false,
            },
            description: "Saves a unified diff / patch file into the dedicated patches/ directory under <workspace_root>/patches/.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "filename": {
                        "type": "string",
                        "description": "Filename or relative subpath inside patches/ (e.g. 'user-service-account-split.patch', must end with .patch or .diff)."
                    },
                    "content": {
                        "type": "string",
                        "description": "Unified diff / patch content (max 1 MB)."
                    },
                    "overwrite": {
                        "type": "boolean",
                        "description": "Whether to overwrite an existing patch file if it already exists (default: false)."
                    }
                },
                "required": ["filename", "content"]
            }),
        },
        Tool {
            name: "validate_patch",
            annotations: crate::protocol::ToolAnnotations {
                read_only_hint: true,
                destructive_hint: false,
                open_world_hint: false,
            },
            description: "Validate a stored or inline unified text diff using the same workspace-confined preflight as apply_patch, without modifying files. Rename, mode-only, binary, and symlink diffs are rejected.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "patch_filename": {
                        "type": "string",
                        "description": "Filename of the patch inside patches/ (e.g. 'user-service-account-split.patch') or relative path."
                    },
                    "patch_content": {
                        "type": "string",
                        "description": "Optional inline patch/diff content to validate directly without reading from a file."
                    },
                    "target_dir": {
                        "type": "string",
                        "description": "Relative path to target repository or directory in workspace where patch applies (default: '.')."
                    },
                    "expected_hashes": {
                        "type": "object",
                        "additionalProperties": {"type": "string", "pattern": "^[a-fA-F0-9]{64}$"},
                        "description": "Optional SHA-256 hashes keyed by existing patch targets relative to target_dir."
                    }
                },
                "anyOf": [
                    {"required": ["patch_filename"]},
                    {"required": ["patch_content"]}
                ]
            }),
        },
    ];
    tools.extend(super::workspace_schema::get_workspace_tools());
    tools
}
