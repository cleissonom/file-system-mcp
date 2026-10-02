# Public repository checklist

Complete this review before making a repository public or uploading a source archive. These commands inspect the local project; they do not publish it.

## Review the publication contents

From the repository root, inspect Git's current candidates and any tracked files that match ignore rules:

```bash
git status --short
git ls-files --cached --others --exclude-standard
git ls-files --cached --ignored --exclude-standard
cargo test --locked --test publication_policy
```

The third command should produce no private artifacts. Ignore rules do not remove already tracked files or erase their historical contents. Review staged and unstaged diffs without sharing secret values in screenshots, logs, or issue reports. Make sure new files are covered too: a clean tracked diff does not inspect untracked source.

Keep these local:

- Real `.env` files and other credential configuration. Only `.env.example` is intended for publication, with placeholders.
- `.data/`, SQLite databases, WAL/SHM/journal companions, lock files, and telemetry exports.
- Private keys, certificate containers, personal workspace paths, and build artifacts.

Keep source SQL migrations, `Cargo.lock`, static dashboard assets, tests, docs, and the license. Review files before adding them; do not force-add ignored configuration or volumes. Also review Git remotes for embedded credentials and your intended public commit author identity. Do not paste authentication-bearing remote URLs or private identity details into reports.

## Scan for credentials

Install Gitleaks 8.30.1 separately and scan all local commit history with redacted output:

```bash
gitleaks git . --redact --no-banner --log-opts='--all'
```

This scans commits, including local branches. It does **not** inspect untracked files or uncommitted working-tree changes, and a repository with no commits has no history to scan. Before the first commit, also scan a clean source copy containing only the files you intend to publish:

```bash
gitleaks dir /path/to/clean-source-copy --redact --no-banner
```

Build that copy from the reviewed candidate list; omit local configuration, databases, `.git`, and build outputs. Do not run a directory scan of private local artifacts and then treat those findings as committed exposure. Gitleaks can miss secrets and can flag synthetic fixtures: inspect findings privately, distinguish fixture strings from actual credentials, and avoid broad exclusions that hide source or tests.

If a real secret reached source history, revoke or rotate it through its issuer and remove it from the publication contents and history before publishing. Deleting a current file is insufficient to remove historical exposure. Keep actual values out of scanner reports, discussions, and PR descriptions.

The CI workflow scans committed history with a pinned Gitleaks release and checksum verification. It also runs the Rust and JavaScript checks on macOS/Linux. Review its results after pushing to a private repository; workflow configuration alone is not evidence that hosted checks have passed.

## Verify the project

Follow all commands in [CONTRIBUTING.md](../CONTRIBUTING.md). Confirm the [README](../README.md) quick start with a disposable workspace. Check that `.env.example` contains placeholders, the [MIT license](../LICENSE) uses the intended attribution, and public docs contain no private paths, invented contact addresses, or unsupported setup instructions.

Document compatibility or migration changes. Observer schema upgrades happen on startup, so back up a real volume before running a new binary. Public source archives must include migration SQL and `observer-ui/`, which the compiler embeds.

## Prepare GitHub and complete public setup

If useful, use a private repository first to review source and run hosted checks before changing visibility. Review available secret-scanning and push-protection settings, access permissions, and the repository description.

GitHub private vulnerability reporting is available for public repositories. Immediately after changing visibility to public, enable **private vulnerability reporting** in the security settings and verify **Security → Report a vulnerability** before inviting reports. See [GitHub's repository configuration guide](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository) and [SECURITY.md](../SECURITY.md). Documentation does not enable this feature by itself.

These are maintainer actions: this checklist does not create a GitHub repository, push commits, or change visibility.
