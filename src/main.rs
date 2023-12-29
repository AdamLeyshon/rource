#![deny(
    rust_2018_idioms,
    unused_must_use,
    clippy::nursery,
    clippy::pedantic,
    clippy::perf,
    clippy::correctness,
    clippy::dbg_macro,
    clippy::else_if_without_else,
    clippy::empty_drop,
    clippy::empty_structs_with_brackets,
    clippy::expect_used,
    clippy::if_then_some_else_none,
    clippy::multiple_inherent_impl,
    clippy::panic,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::same_name_method,
    clippy::string_to_string,
    clippy::todo,
    clippy::try_err,
    clippy::unimplemented,
    clippy::unnecessary_self_imports,
    clippy::unreachable,
    clippy::unwrap_used,
    clippy::wildcard_enum_match_arm
)]

use anyhow::{anyhow, Context};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use clap::Parser;
use csv::QuoteStyle;
use git2::{Commit, Delta, DiffDelta, Repository};
use log::{error, warn};
use serde::Serialize;
use simple_logger::SimpleLogger;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    SimpleLogger::new().init()?;
    let args = ClapArguments::parse();
    let root = PathBuf::from(&*shellexpand::tilde(&args.path));
    let aliases = validate_aliases(&args.alias)?;
    let repositories = discover_repositories(&root, args.recursive, &args.include, &args.exclude)?;
    let repositories = validate_repositories(repositories);
    let logs = repositories
        .iter()
        .map(|r| read_git_log(&root, r))
        .collect::<anyhow::Result<Vec<_>>>()?;
    write_gource_log(logs.into_iter().flatten().collect(), args.output, &aliases)
}

#[derive(Debug, Serialize)]
enum GourceActionType {
    A,
    M,
    D,
}

#[derive(Debug, Serialize)]
struct GourceLogFormat {
    timestamp: DateTime<Utc>,
    username: String,
    r#type: GourceActionType,
    file: String,
}

impl GourceLogFormat {
    fn try_from_delta(
        root_path: &PathBuf,
        repo: &Repository,
        commit: &Commit<'_>,
        delta: &'_ DiffDelta<'_>,
    ) -> anyhow::Result<Option<Self>> {
        // Using the root path, determine the relative path to the repository
        let relative = repo
            .path()
            .strip_prefix(root_path)
            .map_err(|_| anyhow!("Unable to determine relative path for {:?}", repo.path()))?
            .parent()
            .ok_or_else(|| anyhow!("Git repo has no parent path? {:?}", repo.path()))?;

        let timestamp = Utc.from_utc_datetime(
            &NaiveDateTime::from_timestamp_opt(commit.time().seconds(), 0)
                .ok_or_else(|| anyhow!("Unable to parse timestamp log for {:?}", commit))?,
        );
        let username = commit
            .author()
            .name()
            .ok_or_else(|| anyhow!("Unable to parse git log for {:?}", commit))?
            .to_string();
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
        let file = format!(
            "root/{}/{}",
            relative
                .to_str()
                .ok_or_else(|| anyhow!("Unable to parse git path for {:?}", relative))?,
            path
        );

        Ok(Some(Self {
            timestamp,
            username,
            r#type,
            file,
        }))
    }
}

#[derive(Parser)]
#[command(author = "Adam Leyshon", version = "0.0.1", about, long_about = None)]
struct ClapArguments {
    #[arg(short, long, help = "The path to the git repository/repositories")]
    path: String,

    #[arg(
        short,
        long,
        help = "Recursively search for repositories, by default all repositories in <PATH> will be included"
    )]
    recursive: bool,

    #[arg(
        requires = "recursive",
        short,
        long,
        help = "Used with recursive, only process these repositories, cannot be used with --exclude"
    )]
    include: Vec<String>,

    #[arg(
        requires = "recursive",
        conflicts_with = "include",
        short,
        long,
        help = "Used with recursive, exclude these repositories from processing, cannot be used with --include"
    )]
    exclude: Vec<String>,

    #[arg(short, long, help = "Output file, defaults to stdout")]
    output: Option<String>,

    #[arg(
        long,
        help = "Add an alias for a user, format it <GIT_USERNAME>::<GOURCE_USERNAME>, you can specify this option multiple times"
    )]
    alias: Vec<String>,
}

fn validate_aliases(aliases: &[String]) -> anyhow::Result<HashMap<String, String>> {
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
fn discover_repositories(
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
fn validate_repositories(mut repositories: Vec<PathBuf>) -> Vec<PathBuf> {
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

/// Read the git log for a repository and parse into our struct
fn read_git_log(root_path: &PathBuf, path: &PathBuf) -> anyhow::Result<Vec<GourceLogFormat>> {
    let mut logs: Vec<GourceLogFormat> = Vec::new();
    let repo = Repository::open(path)?;
    let mut revwalk = repo.revwalk()?;
    revwalk
        .push_head()
        .context(format!("Processing {path:?}"))?;
    revwalk.set_sorting(git2::Sort::TIME)?;

    for revision in revwalk {
        let Ok(revision) = revision else {
            error!("Failed to read revision: {:?}", revision);
            continue;
        };

        let commit = repo.find_commit(revision)?;
        logs.append(&mut compute_diff(root_path, &repo, &commit)?);
    }

    Ok(logs)
}

/// Compute the diff between two trees and return a list of changes
fn compute_diff(
    root_path: &PathBuf,
    repo: &Repository,
    commit: &Commit<'_>,
) -> anyhow::Result<Vec<GourceLogFormat>> {
    let a = if commit.parents().len() == 1 {
        let parent = commit.parent(0)?;
        Some(parent.tree()?)
    } else {
        None
    };

    let b = commit.tree()?;
    let diff = repo.diff_tree_to_tree(a.as_ref(), Some(&b), None)?;
    Ok(diff
        .deltas()
        .filter_map(|d| {
            GourceLogFormat::try_from_delta(root_path, repo, commit, &d).unwrap_or_else(|e| {
                error!("{e}");
                None
            })
        })
        .collect::<Vec<_>>())
}

/// Write out the changes we've accumulated to the target
fn write_gource_log(
    mut logs: Vec<GourceLogFormat>,
    output_file: Option<String>,
    aliases: &HashMap<String, String>,
) -> anyhow::Result<()> {
    // Sort the logs by timestamp, we'll use unstable to save on memory
    logs.sort_unstable_by_key(|log| log.timestamp);

    // Apply any aliases
    for log in &mut logs {
        if let Some(alias) = aliases.get(&log.username) {
            log.username = alias.to_string();
        }
    }

    // Set the output stream
    let output_stream: Box<dyn Write> = match output_file {
        Some(path) => Box::new(std::fs::File::create(path)?),
        None => Box::new(std::io::stdout()),
    };

    // Use CSV to write the logs using Serde
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .delimiter(b'|')
        .quote_style(QuoteStyle::Necessary)
        .from_writer(output_stream);
    for log in logs {
        writer.serialize(log)?;
    }
    writer.flush()?;

    Ok(())
}
