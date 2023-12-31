use crate::consts::DEFAULT_PROGRESS_STYLE;
use crate::structs::GourceLogFormat;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::io::{BufWriter, Read, Seek, Write};
use std::path::PathBuf;
use std::{fs, io};

#[derive(Serialize, Deserialize)]
pub struct DiskGourceLog {
    pub size: u16,
    pub data: Vec<u8>,
}

pub fn log_to_bytes(log: &GourceLogFormat) -> anyhow::Result<DiskGourceLog> {
    let data = serde_cbor::ser::to_vec_packed(&log)?;
    Ok(DiskGourceLog {
        size: u16::try_from(data.len())?,
        data,
    })
}

pub fn batch_log_write<T>(writer: &mut BufWriter<T>, logs: Vec<DiskGourceLog>) -> anyhow::Result<()>
where
    T: Write,
{
    for log in logs {
        log_write(writer, &log)?;
    }
    Ok(())
}

pub fn log_write<T>(writer: &mut BufWriter<T>, log: &DiskGourceLog) -> anyhow::Result<()>
where
    T: Write,
{
    writer.write_all(&log.size.to_le_bytes())?;
    writer.write_all(&log.data)?;
    Ok(())
}

pub struct DiskLogReader {
    reader: io::BufReader<fs::File>,
    progress_bar: ProgressBar,
}

impl DiskLogReader {
    pub fn new(filename: &PathBuf, multi_progress: &MultiProgress) -> anyhow::Result<Self> {
        let input_reader = io::BufReader::new(fs::File::open(filename)?);
        let progress_bar = multi_progress.add(
            ProgressBar::new(fs::metadata(filename)?.len())
                .with_style(ProgressStyle::with_template(DEFAULT_PROGRESS_STYLE)?),
        );
        progress_bar.set_prefix("Log Data");
        progress_bar.set_message("Building Chunks");
        Ok(Self {
            reader: input_reader,
            progress_bar,
        })
    }

    pub fn record_count(&mut self) -> anyhow::Result<u64> {
        let mut counter = 0;
        loop {
            let mut size_bytes = [0u8; 2];
            if self.reader.read_exact(&mut size_bytes).is_err() {
                // When we hit EOF, reset the reader and return the counter
                self.reader.seek(io::SeekFrom::Start(0))?;
                return Ok(counter);
            };
            // Figure out the size of the object and skip over it
            let data_size = u16::from_le_bytes(size_bytes) as usize;
            #[allow(clippy::cast_possible_wrap)]
            // Reason: Unlikely that we'd ever have an object this big
            self.reader.seek_relative(data_size as i64)?;
            counter += 1;
        }
    }
}

impl Drop for DiskLogReader {
    fn drop(&mut self) {
        self.progress_bar.finish_with_message("Done");
    }
}

impl Iterator for DiskLogReader {
    type Item = Result<GourceLogFormat, io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut size_bytes = [0u8; 2];
        self.reader.read_exact(&mut size_bytes).ok()?;
        let data_size = u16::from_le_bytes(size_bytes) as usize;
        let mut data = vec![0u8; data_size];
        self.reader.read_exact(&mut data).ok()?;
        self.progress_bar.inc((data_size + 2) as u64);
        Some(Ok(serde_cbor::de::from_slice(&data).ok()?))
    }
}

pub fn serialize_logs(changes: &[GourceLogFormat]) -> anyhow::Result<Vec<DiskGourceLog>> {
    use rayon::prelude::*;
    changes
        .par_iter()
        .map(log_to_bytes)
        .collect::<anyhow::Result<Vec<_>>>()
}
