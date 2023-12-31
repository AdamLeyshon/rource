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

mod cli;
mod consts;
mod git_stuff;
mod serde;
mod structs;
mod validation;

use crate::serde::DiskLogReader;
use anyhow::Context;
use clap::Parser;
use cli::ClapArguments;
use csv::QuoteStyle;
use ext_sort::buffer::mem::MemoryLimitedBufferBuilder;
use ext_sort::{ExternalSorter, ExternalSorterBuilder};

use crate::consts::{DEFAULT_PROGRESS_STYLE, DEFAULT_SPINNER_STYLE, DEFAULT_SPINNER_TICK_STYLE};
use crate::structs::{GourceLogConfig, MergeSortConfig};
use consts::TEMPORARY_LOG_FILENAME;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use indicatif_log_bridge::LogWrapper;
use log::warn;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use std::{fs, io};
use structs::GourceLogFormat;

fn main() -> anyhow::Result<()> {
    reset_pipe();
    let args = ClapArguments::parse();

    // Setup logging
    let mut logger = env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("info")
            .filter("ext_sort=warn"),
    );

    // If we're writing to stdout, disable logging
    if args.output.is_none() {
        logger.filter_level(log::LevelFilter::Off);
    }

    let logger = logger.build();

    let multi = MultiProgress::new();
    LogWrapper::new(multi.clone(), logger).try_init()?;

    // Cleanup any previous runs if they exist
    if Path::new(TEMPORARY_LOG_FILENAME).exists() {
        fs::remove_file(TEMPORARY_LOG_FILENAME).context("Failed to remove temp file")?;
    }

    // Parse and validate the arguments, then discover the repositories
    let root = PathBuf::from(&*shellexpand::tilde(&args.path)).canonicalize()?;
    let aliases = validation::validate_aliases(&args.alias)?;
    let repositories =
        validation::discover_repositories(&root, args.recursive, &args.include, &args.exclude)?;
    let repositories = validation::validate_repositories(repositories);

    #[allow(clippy::if_then_some_else_none)]
    // Reason: We can't use ? inside a closure
    let (merge_sort_config, locked_output_writer) = if args.use_merge_sort {
        let config = MergeSortConfig::new(args.sort_chunk_size, args.temp_file_location)?;

        let writer = Mutex::new(io::BufWriter::new(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(config.tmp_location.join(TEMPORARY_LOG_FILENAME))?,
        ));

        (Some(config), Some(writer))
    } else {
        (None, None)
    };

    let logs = repositories
        .par_iter()
        .map(|r| {
            git_stuff::read_git_log(
                &root,
                r,
                locked_output_writer.as_ref(),
                &multi,
                args.max_changeset_size,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let temp_path = merge_sort_config.as_ref().map(|c| c.tmp_location.clone());

    // Do the final sort and write out the log file
    write_gource_log(
        logs.into_iter().flatten().collect(),
        &multi,
        GourceLogConfig {
            output_file: args.output,
            aliases,
            merge_sort_config,
        },
    )?;

    // Cleanup if needed
    let Some(path) = temp_path else { return Ok(()) };
    let temp_file = path.join(TEMPORARY_LOG_FILENAME);
    if !temp_file.exists() {
        return Ok(());
    }
    // Remove the temporary file
    fs::remove_file(temp_file)?;

    // Hopefully that the last file in the directory
    if path.read_dir()?.next().is_none() {
        fs::remove_dir(path)?;
    } else {
        warn!("Temporary directory still contains files, not removing");
    }

    Ok(())
}

#[cfg(unix)]
fn reset_pipe() {
    sigpipe::reset();
}

#[cfg(not(unix))]
fn reset_pipe() {}

pub struct LogSource {
    pub source: Box<dyn Iterator<Item = GourceLogFormat>>,
    pub size_hint: u64,
}

/// Write out the changes we've accumulated to the target
fn write_gource_log(
    mut logs: Vec<GourceLogFormat>,
    progress_bar: &MultiProgress,
    config: GourceLogConfig,
) -> anyhow::Result<()> {
    // Setup the progress bar
    let merge_progress = progress_bar.add(ProgressBar::new_spinner());
    merge_progress.set_style(
        ProgressStyle::with_template(DEFAULT_SPINNER_STYLE)?.tick_chars(DEFAULT_SPINNER_TICK_STYLE),
    );
    merge_progress.set_prefix("Generating output");
    merge_progress.enable_steady_tick(Duration::from_millis(100));
    merge_progress.set_message("Merge and Sort");

    // Do we need to do a merge sort?
    let source = if let Some(ms_config) = config.merge_sort_config {
        let mut reader = DiskLogReader::new(
            &ms_config.tmp_location.join(TEMPORARY_LOG_FILENAME),
            progress_bar,
        )?;
        let records = reader.record_count()?;

        let sorter: ExternalSorter<GourceLogFormat, io::Error, MemoryLimitedBufferBuilder> =
            ExternalSorterBuilder::new()
                .with_tmp_dir(Path::new("./"))
                .with_buffer(MemoryLimitedBufferBuilder::new(
                    ms_config.chunk_size * 1024 * 1024,
                ))
                .build()?;

        LogSource {
            size_hint: records,
            source: Box::new(sorter.sort(reader)?.flatten()),
        }
    } else {
        // Sort in memory
        logs.sort_unstable_by_key(|log| log.timestamp);
        LogSource {
            size_hint: logs.len() as u64,
            source: Box::new(logs.into_iter()),
        }
    };

    merge_progress.set_message("Gourcification");
    write_to_output(source, config.output_file, &config.aliases, progress_bar)?;
    merge_progress.finish_with_message("Done");

    Ok(())
}

fn write_to_output(
    source: LogSource,
    output_file: Option<String>,
    aliases: &HashMap<String, String>,
    multi_progress: &MultiProgress,
) -> anyhow::Result<()> {
    let progress_bar = multi_progress.add(
        ProgressBar::new(source.size_hint)
            .with_style(ProgressStyle::with_template(DEFAULT_PROGRESS_STYLE)?),
    );

    progress_bar.set_prefix("Writing Gource Log");

    // Set the output stream
    let output_stream: Box<dyn Write> = match output_file {
        Some(path) => Box::new(fs::File::create(path)?),
        None => Box::new(io::stdout()),
    };

    // Use CSV to write the logs using Serde
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .delimiter(b'|')
        .quote_style(QuoteStyle::Necessary)
        .from_writer(output_stream);

    for mut log in source.source {
        // Apply any aliases
        progress_bar.inc(1);
        if let Some(alias) = aliases.get(&log.username) {
            log.username = alias.to_string();
        }
        writer.serialize(log)?;
    }
    progress_bar.finish_with_message("Done");
    writer.flush().context("Failed to write output")
}
