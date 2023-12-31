use anyhow::{anyhow, bail};
use deepsize::DeepSizeOf;
use git2::{Commit, Delta, DiffDelta, Repository};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, DeepSizeOf)]
pub enum GourceActionType {
    A,
    M,
    D,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, DeepSizeOf)]
pub struct GourceLogFormat {
    pub timestamp: i64,
    pub username: String,
    pub r#type: GourceActionType,
    pub file: String,
}

impl PartialOrd for GourceLogFormat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GourceLogFormat {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.timestamp
            .cmp(&other.timestamp)
            .then(self.file.cmp(&other.file))
            .then(self.r#type.cmp(&other.r#type))
            .then(self.username.cmp(&other.username))
    }
}

impl GourceLogFormat {
    pub fn try_from_delta(
        root_path: &PathBuf,
        repo: &Repository,
        commit: &Commit<'_>,
        delta: &'_ DiffDelta<'_>,
    ) -> anyhow::Result<Option<Self>> {
        // Using the root path, determine the relative path to the repository
        let relative = repo
            .path()
            .strip_prefix(root_path)
            .map_err(|e| {
                anyhow!(
                    "Unable to determine relative path for {:?}: {e}",
                    repo.path()
                )
            })?
            .parent()
            .ok_or_else(|| anyhow!("Git repo has no parent path? {:?}", repo.path()))?;

        let username = commit
            .author()
            .name()
            .ok_or_else(|| anyhow!("Unable to parse git log for {:?}", commit))?
            .replace('|', "#");

        let r#type = match delta.status() {
            Delta::Added => GourceActionType::A,
            Delta::Deleted => GourceActionType::D,
            Delta::Modified | Delta::Renamed | Delta::Copied | Delta::Typechange => {
                GourceActionType::M
            }
            // These don't change the tree so they're NOPs
            Delta::Untracked
            | Delta::Unmodified
            | Delta::Unreadable
            | Delta::Conflicted
            | Delta::Ignored => {
                return Ok(None);
            }
        };

        let path = delta
            .new_file()
            .path()
            .ok_or_else(|| anyhow!("Unable to parse git log for {:?}", commit))?
            .to_str()
            .ok_or_else(|| anyhow!("Unable to parse git log for {:?}", commit))?
            .to_string();

        let file = if relative.as_os_str() == "" {
            path
        } else {
            format!(
                "{}/{}",
                relative
                    .to_str()
                    .ok_or_else(|| anyhow!("Unable to parse git path for {:?}", relative))?,
                path
            )
        };

        Ok(Some(Self {
            timestamp: commit.time().seconds(),
            username,
            r#type,
            file,
        }))
    }
}

pub struct GourceLogConfig {
    pub output_file: Option<String>,
    pub aliases: HashMap<String, String>,
    pub merge_sort_config: Option<MergeSortConfig>,
}

pub struct MergeSortConfig {
    pub chunk_size: u64,
    pub tmp_location: PathBuf,
}

impl MergeSortConfig {
    pub fn new(chunk_size: Option<u64>, tmp_location: Option<String>) -> anyhow::Result<Self> {
        let tmp_location = tmp_location.map_or_else(
            || {
                let random_bytes = rand::thread_rng()
                    .sample_iter(&rand::distributions::Alphanumeric)
                    .take(5)
                    .collect::<Vec<u8>>();
                // Reason: Infallible conversion of ASCII characters to UTF-8
                #[allow(clippy::unwrap_used)]
                let random_chars = String::from_utf8(random_bytes).unwrap();
                Path::new(&format!("./rource-temp-{random_chars}/")).to_path_buf()
            },
            |user_path| Path::new(&*shellexpand::tilde(&user_path)).to_path_buf(),
        );

        if tmp_location.parent().ok_or_else(|| anyhow!("Temporary directory has no parent (refusing to use '/' !) or the path was invalid"))?.exists() {
            // Try to create the directory, but don't fail if it already exists
            fs::create_dir_all(&tmp_location)?;
        } else {
            bail!("Path provided for temporary directory does not exist: {:?}", tmp_location);
        }

        let chunk_size = chunk_size.unwrap_or(4096);
        if chunk_size < 50 {
            bail!("Chunk size must be at least 64 MB, try --help for more information");
        }

        Ok(Self {
            chunk_size,
            tmp_location,
        })
    }
}
