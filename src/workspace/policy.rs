use super::{Workspace, boundary};
use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use rustix::fd::OwnedFd;
use rustix::fs::{self, AtFlags, Mode, OFlags};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

struct IgnoreRules {
    base: PathBuf,
    matcher: Gitignore,
}

impl Workspace {
    pub(super) fn check_policy(&self, path: &Path, is_dir: bool) -> Result<(), String> {
        check_protected(path)?;
        // Check actual entry names so filesystem case/Unicode aliases cannot bypass rules.
        let path = self.actual_path(path)?;
        check_protected(&path)?;
        if !path.as_os_str().is_empty() && self.is_ignored(&path, is_dir)? {
            return Err(format!(
                "Access denied: '{}' is ignored by .gitignore",
                path.display()
            ));
        }
        Ok(())
    }

    fn policy_directory(&self, path: &Path) -> Result<Option<OwnedFd>, String> {
        let mut fd = boundary::duplicate(&self.fd)?;
        for component in path.components() {
            match fs::openat(
                &fd,
                component.as_os_str(),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(next) => fd = next,
                Err(
                    rustix::io::Errno::NOENT | rustix::io::Errno::NOTDIR | rustix::io::Errno::LOOP,
                ) => return Ok(None),
                Err(error) => return Err(format!("Cannot inspect ignore policy: {error}")),
            }
        }
        Ok(Some(fd))
    }

    fn ignore_lines(&self, path: &Path) -> Result<Option<String>, String> {
        let Some(fd) = self.policy_directory(path.parent().unwrap_or(Path::new("")))? else {
            return Ok(None);
        };
        let name = path.file_name().unwrap();
        let file = match fs::openat(
            &fd,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => File::from(fd),
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(error) => return Err(format!("Cannot safely read ignore policy: {error}")),
        };
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Ignore policy must be a regular file".into());
        }
        let mut content = String::new();
        file.take(1024 * 1024 + 1)
            .read_to_string(&mut content)
            .map_err(|e| e.to_string())?;
        if content.len() > 1024 * 1024 {
            return Err("Ignore policy exceeds safety limit".into());
        }
        Ok(Some(content))
    }

    fn is_ignored(&self, path: &Path, is_dir: bool) -> Result<bool, String> {
        let target_dir = if is_dir {
            path
        } else {
            path.parent().unwrap_or(Path::new(""))
        };
        let rules = self.ignore_rules(target_dir)?;
        let mut prefix = PathBuf::new();
        // A descendant negation cannot reopen an ignored parent directory.
        for component in path.components() {
            prefix.push(component);
            if self.ignored_at(&rules, &prefix, is_dir || prefix != path) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn ignore_rules(&self, target_dir: &Path) -> Result<Vec<IgnoreRules>, String> {
        let repo = self.repository_root(target_dir)?;
        let mut rules = Vec::new();
        // Repository excludes have lower priority than .gitignore files.
        self.add_ignore(&mut rules, &repo, &repo.join(".git/info/exclude"))?;
        let mut chain: Vec<_> = target_dir.ancestors().collect();
        chain.reverse();
        for dir in chain {
            self.add_ignore(&mut rules, dir, &dir.join(".gitignore"))?;
        }
        Ok(rules)
    }

    fn ignored_at(&self, rules: &[IgnoreRules], path: &Path, is_dir: bool) -> bool {
        let mut ignored = false;
        for rule in rules {
            if !path.starts_with(&rule.base) || path == rule.base {
                continue;
            }
            match rule.matcher.matched(self.root.join(path), is_dir) {
                Match::Ignore(_) => ignored = true,
                Match::Whitelist(_) => ignored = false,
                Match::None => {}
            }
        }
        ignored
    }

    fn repository_root(&self, target_dir: &Path) -> Result<PathBuf, String> {
        for dir in target_dir.ancestors() {
            if let Some(fd) = self.policy_directory(dir)?
                && fs::statat(&fd, ".git", AtFlags::SYMLINK_NOFOLLOW).is_ok()
            {
                return Ok(dir.to_path_buf());
            }
        }
        Ok(PathBuf::new())
    }

    fn add_ignore(
        &self,
        rules: &mut Vec<IgnoreRules>,
        base: &Path,
        path: &Path,
    ) -> Result<(), String> {
        if let Some(content) = self.ignore_lines(path)? {
            let mut builder = GitignoreBuilder::new(self.root.join(base));
            for line in content.trim_start_matches('\u{feff}').lines() {
                builder
                    .add_line(Some(self.root.join(path)), line)
                    .map_err(|e| format!("Invalid ignore policy: {e}"))?;
            }
            let matcher = builder
                .build()
                .map_err(|e| format!("Invalid ignore policy: {e}"))?;
            rules.push(IgnoreRules {
                base: base.to_path_buf(),
                matcher,
            });
        }
        Ok(())
    }
}

fn check_protected(path: &Path) -> Result<(), String> {
    if crate::security::is_denylisted(path)
        || path.components().any(|component| {
            component
                .as_os_str()
                .to_string_lossy()
                .starts_with(".mcp-tmp-")
        })
    {
        return Err(format!(
            "Access denied: '{}' matches security denylist",
            path.display()
        ));
    }
    Ok(())
}
