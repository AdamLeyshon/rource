use crate::serde::{batch_log_write, serialize_logs};
use crate::structs::GourceLogFormat;
use anyhow::Context;
use git2::{Commit, Oid, Repository};
use log::error;

use crate::consts::{DEFAULT_PROGRESS_STYLE, DEFAULT_SPINNER_STYLE, DEFAULT_SPINNER_TICK_STYLE};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use std::{fs, io};

/// Read the git log for a repository and parse into our struct
pub fn read_git_log(
    root_path: &PathBuf,
    path: &PathBuf,
    locked_output_writer: Option<&Mutex<io::BufWriter<fs::File>>>,
    progress_bar: &MultiProgress,
    max_changeset_size: Option<usize>,
) -> anyhow::Result<Vec<GourceLogFormat>> {
    let logs: Vec<GourceLogFormat> = Vec::new();

    let repo_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Failed to decode path for repo"))?
        .to_str()
        .unwrap_or("Non-UTF8 repo path")
        .to_string();

    // Create a temporary progress bar while we open the repository
    let sub_bar = progress_bar.add(ProgressBar::new_spinner().with_style(
        ProgressStyle::with_template(DEFAULT_SPINNER_STYLE)?.tick_chars(DEFAULT_SPINNER_TICK_STYLE),
    ));
    sub_bar.set_prefix(format!("Initialising Repository {repo_name}"));
    sub_bar.enable_steady_tick(Duration::from_millis(100));

    // Open the repository
    let repo = Repository::open(path)?;

    // Reset the progress bar
    progress_bar.remove(&sub_bar);

    // Create a new progress bar for processing commits
    let commit_count = get_commit_count(&repo)?;
    let sub_bar = progress_bar.add(
        ProgressBar::new(commit_count as u64)
            .with_style(ProgressStyle::with_template(DEFAULT_PROGRESS_STYLE)?),
    );

    sub_bar.set_prefix(format!("Processing {repo_name}"));
    sub_bar.set_message("Reading commit: ");

    let mut revwalk = repo.revwalk()?;
    revwalk
        .push_head()
        .context(format!("Processing {repo_name}"))?;
    revwalk.set_sorting(git2::Sort::TIME)?;

    let log_lock = Mutex::new(logs);
    let commits = revwalk.collect::<Vec<Result<Oid, _>>>();

    commits.par_iter().for_each(|revision| {
        let Ok(repo) = Repository::open(path) else {
            error!("Failed to open repository: {:?}", path);
            return;
        };

        sub_bar.inc(1);

        let Ok(revision) = revision else {
            error!("Failed to read revision: {:?}", revision);
            return;
        };

        let Ok(commit) = &repo.find_commit(*revision) else {
            error!("Failed to find commit: {:?}", revision);
            return;
        };

        let Ok(mut changes) = compute_diff(root_path, &repo, commit, max_changeset_size) else {
            error!("Failed to compute diff for commit: {:?}", revision);
            return;
        };

        if changes.is_empty() {
            return;
        }

        if let Some(writer) = locked_output_writer.as_ref() {
            let Ok(changes) = serialize_logs(&changes[..]) else {
                error!("Failed to serialize logs for commit: {:?}", revision);
                return;
            };
            let Ok(mut writer) = writer.lock() else {
                error!("Failed to lock writer for commit: {:?}", revision);
                return;
            };
            if let Err(e) = batch_log_write(&mut writer, changes) {
                error!("Failed to write logs for commit: {:?} - {:?}", revision, e);
            }
        } else {
            let Ok(mut logs) = log_lock.lock() else {
                error!("Failed to lock writer for commit: {:?}", revision);
                return;
            };
            logs.append(&mut changes);
        }
    });

    if let Some(writer) = locked_output_writer {
        let mut writer = writer
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock writer for buffer flush - {:?}", e))?;
        writer.flush()?;
    }

    sub_bar.finish_with_message("Finished");

    Ok(log_lock.into_inner()?)
}

fn get_commit_count(repo: &Repository) -> anyhow::Result<usize> {
    let mut revwalk = repo.revwalk()?;
    revwalk
        .push_head()
        .context(format!("Processing {:?}", repo.path()))?;

    Ok(revwalk.count())
}

/// Compute the diff between two trees and return a list of changes
fn compute_diff(
    root_path: &PathBuf,
    repo: &Repository,
    commit: &Commit<'_>,
    limit: Option<usize>,
) -> anyhow::Result<Vec<GourceLogFormat>> {
    let a = if commit.parents().len() == 1 {
        let parent = commit.parent(0)?;
        Some(parent.tree()?)
    } else {
        None
    };

    let b = commit.tree()?;
    let diff = repo.diff_tree_to_tree(a.as_ref(), Some(&b), None)?;
    let iter = diff.deltas().filter_map(|d| {
        GourceLogFormat::try_from_delta(root_path, repo, commit, &d).unwrap_or_else(|e| {
            error!("{e}");
            None
        })
    });

    if let Some(limit) = limit {
        let c: Vec<GourceLogFormat> = iter.take(limit + 1).collect();
        if c.len() > limit {
            return Ok(vec![]);
        }
        Ok(c)
    } else {
        Ok(iter.collect())
    }
}
