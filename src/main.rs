use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use anyhow::{anyhow, Context};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use clap::Parser;
use csv::QuoteStyle;
use git2::{Commit, Delta, DiffDelta, Repository};
use log::{error, warn};
use serde::Serialize;
use simple_logger::SimpleLogger;

fn main() -> anyhow::Result<()> {
    SimpleLogger::new().init()?;
    let args = ClapArguments::parse();
    let root = PathBuf::from(&*shellexpand::tilde(&args.path));
    let aliases = validate_aliases(&args.alias)?;
    let repositories = discover_repositories(&root, args.recursive, &args.include, &args.exclude)?;
    let repositories = validate_repositories(repositories);
    let logs = repositories.iter().map(|r| read_git_log(&root, r)).collect::<anyhow::Result<Vec<_>>>()?;
    write_gource_log(logs.into_iter().flatten().collect(), args.output, aliases)
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
    fn try_from_delta(root_path: &PathBuf, repo: &Repository, commit: &Commit, delta: DiffDelta) -> anyhow::Result<Option<Self>> {

        // Using the root path, determine the relative path to the repository
        let relative = repo.path().strip_prefix(root_path).map_err(|_| anyhow!("Unable to determine relative path for {:?}", repo.path()))?.parent().ok_or_else(|| anyhow!("Git repo has no parent path? {:?}", repo.path()))?;

        let timestamp = Utc.from_utc_datetime(&NaiveDateTime::from_timestamp_opt(commit.time().seconds(), 0).ok_or_else(|| anyhow!("Unable to parse timestamp log for {:?}", commit))?);
        let username = commit.author().name().ok_or_else(|| anyhow!("Unable to parse git log for {:?}", commit))?.to_string();
        let r#type = match delta.status() {
            Delta::Added => GourceActionType::A,
            Delta::Deleted => GourceActionType::D,
            Delta::Modified => GourceActionType::M,
            Delta::Renamed => GourceActionType::M,
            Delta::Copied => GourceActionType::M,
            Delta::Typechange => GourceActionType::M,
            // These don't change the tree so they're NOPs
            Delta::Untracked |
            Delta::Unmodified |
            Delta::Unreadable |
            Delta::Conflicted |
            Delta::Ignored => { return Ok(None); }
        };
        let path = delta.new_file().path().ok_or_else(|| anyhow!("Unable to parse git log for {:?}", commit))?.to_str().ok_or_else(|| anyhow!("Unable to parse git log for {:?}", commit))?.to_string();
        let file = format!("root/{}/{}", relative.to_str().ok_or_else(|| anyhow!("Unable to parse git path for {:?}", relative))?, path);

        Ok(Some(GourceLogFormat {
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

    #[arg(short, long, help = "Recursively search for repositories, by default all repositories in <PATH> will be included")]
    recursive: bool,

    #[arg(requires = "recursive", short, long, help = "Used with recursive, only process these repositories, cannot be used with --exclude")]
    include: Vec<String>,

    #[arg(requires = "recursive", conflicts_with = "include", short, long,
    help = "Used with recursive, exclude these repositories from processing, cannot be used with --include")]
    exclude: Vec<String>,

    #[arg(short, long, help = "Output file, defaults to stdout")]
    output: Option<String>,

    #[arg(long, help = "Add an alias for a user, format it <GIT_USERNAME>::<GOURCE_USERNAME>, you can specify this option multiple times")]
    alias: Vec<String>,
}

fn validate_aliases(aliases: &[String]) -> anyhow::Result<HashMap<String, String>> {
    let mut validated_aliases: HashMap<String, String> = HashMap::with_capacity(aliases.len());
    for alias in aliases {
        let parts = alias.split("::").collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(anyhow!("Invalid alias format, expected <GIT_USERNAME>::<GOURCE_USERNAME>"));
        }
        validated_aliases.insert(parts[0].to_string(), parts[1].to_string());
    }
    Ok(validated_aliases)
}

fn discover_repositories(root: &PathBuf, recursive: bool, include: &[String], exclude: &[String]) -> anyhow::Result<Vec<PathBuf>> {
    let mut repositories = Vec::new();

    for entry in root.read_dir()?.collect::<Result<Vec<_>, _>>()? {
        // Is this a directory?
        if !entry.file_type()?.is_dir() {
            // Skip non-directories
            continue;
        }

        // Is this potentially a git repository?
        if entry.file_name().to_str().ok_or_else(|| anyhow!("Unable to read path {:?}", entry))? == ".git" {
            // Remember the path to this repository
            repositories.push(root.clone());
            continue;
        } else {
            if recursive {
                let mut sub_repositories = discover_repositories(&entry.path(), recursive, include, exclude)?;
                repositories.append(&mut sub_repositories);
            }
        }
    }


    Ok(repositories)
}

/// Take a list of repositories and validate them, returning the list repositories with the invalid ones removed
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
    revwalk.push_head().context(format!("Processing {:?}", path))?;
    revwalk.set_sorting(git2::Sort::TIME)?;

    for revision in revwalk {
        let Ok(revision) = revision else {
            error!("Failed to read revision: {:?}", revision);
            continue;
        };

        let commit = repo.find_commit(revision)?;
        logs.append(&mut compute_diff(root_path, &repo, commit)?);
    }

    Ok(logs)
}

/// Compute the diff between two trees and return a list of changes
fn compute_diff(root_path: &PathBuf, repo: &Repository, commit: Commit) -> anyhow::Result<Vec<GourceLogFormat>> {
    let a = if commit.parents().len() == 1 {
        let parent = commit.parent(0)?;
        Some(parent.tree()?)
    } else {
        None
    };

    let b = commit.tree()?;
    let diff = repo.diff_tree_to_tree(a.as_ref(), Some(&b), None)?;
    Ok(diff.deltas().filter_map(|d| GourceLogFormat::try_from_delta(root_path, repo, &commit, d).unwrap_or_else(|e| {
        error!("{e}");
        None
    })).collect::<Vec<_>>())
}

fn write_gource_log(mut logs: Vec<GourceLogFormat>, output_file: Option<String>, aliases: HashMap<String, String>) -> anyhow::Result<()> {
    // First we need to sort the logs by timestamp, we'll use unstable to save on memory
    logs.sort_unstable_by_key(|log| log.timestamp);

    // Now we need to apply the aliases
    logs.iter_mut().for_each(|log| {
        if let Some(alias) = aliases.get(&log.username) {
            log.username = alias.to_string();
        }
    });

    // Choose the output stream
    let output_stream: Box<dyn Write> = match output_file {
        Some(path) => {
            Box::new(std::fs::File::create(path)?)
        }
        None => {
            Box::new(std::io::stdout())
        }
    };

    // Use CSV to write the logs using Serde
    let mut writer = csv::WriterBuilder::new().has_headers(false).delimiter(b'|').quote_style(QuoteStyle::Necessary).from_writer(output_stream);
    for log in logs {
        writer.serialize(log)?;
    }
    writer.flush()?;

    Ok(())
}

