use anyhow::anyhow;
use git2::Repository;
use log::{error, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn validate_aliases(aliases: &[String]) -> anyhow::Result<HashMap<String, String>> {
    let mut validated_aliases: HashMap<String, String> = HashMap::with_capacity(aliases.len());
    for alias in aliases {
        let parts = alias.split("::").collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(anyhow!(
                "Invalid alias format, expected <GIT_USERNAME>::<GOURCE_USERNAME>"
            ));
        }
        validated_aliases.insert(parts[0].to_string(), parts[1].to_string());
    }
    Ok(validated_aliases)
}

/// Try to find potential git repositories in a directory
pub fn discover_repositories(
    root: &Path,
    recursive: bool,
    include: &[String],
    exclude: &[String],
) -> anyhow::Result<Vec<PathBuf>> {
    let mut repositories = Vec::new();

    for entry in root.read_dir()?.collect::<Result<Vec<_>, _>>()? {
        if !entry.file_type()?.is_dir() {
            // Skip non-directories
            continue;
        }

        let entry_name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| anyhow!("Unable to read path {:?}", entry))?
            .to_string();

        // Assuming we're at the parent level before we recurse, check if we should skip this directory
        if !exclude.is_empty() && exclude.contains(&entry_name) {
            // Skip excluded directories
            continue;
        }
        if !include.is_empty() && !include.contains(&entry_name) {
            // Skip non-included directories
            continue;
        }

        // After passing the include/exclude checks,
        // Is this potentially a git repository?
        if entry_name == ".git" {
            // Push this as a potential repository
            repositories.push(root.to_path_buf());

            // Don't recurse into .git directories
            continue;
        }

        if recursive {
            let mut sub_repositories =
                discover_repositories(&entry.path(), recursive, include, exclude)?;
            repositories.append(&mut sub_repositories);
        }
    }

    Ok(repositories)
}

/// Take a list of repository paths and validate them, returning the list repositories with the invalid ones removed
pub fn validate_repositories(mut repositories: Vec<PathBuf>) -> Vec<PathBuf> {
    repositories.retain(|path| {
        let path = PathBuf::from(path);
        match Repository::open(path.as_path()) {
            Ok(r) => {
                if r.head().is_err() {
                    warn!("Skipping repository with no HEAD {:?}", path);
                    return false;
                }
                if r.is_bare() {
                    warn!("Skipping bare repository {:?}", path);
                    return false;
                }
                if r.is_empty().unwrap_or(false) {
                    warn!("Skipping empty repository {:?}", path);
                    return false;
                }
                if r.head_detached().unwrap_or(false) {
                    warn!("Skipping detached head repository {:?}", path);
                    return false;
                }
                true
            }
            Err(e) => {
                error!("Failed to open repository: {}", e);
                false
            }
        }
    });
    repositories
}
