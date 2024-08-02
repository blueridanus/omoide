//! Handle parsing subtitle files and getting the stuff we want from them.
use anyhow::Context;
use srtlib::{Subtitles, Timestamp};
use std::fs;
use std::path::Path;
use std::time::Duration;

/// A subtitle to be shown, contains the start/end time it's shown for and the content shown on
/// screen.
#[derive(Clone, Debug)]
pub struct SubtitleChunk {
    pub start: Duration,
    pub end: Duration,
    pub content: String,
}

fn timestamp_to_duration(t: &Timestamp) -> Duration {
    let (hours, minutes, seconds, milliseconds) = t.get();
    let milliseconds =
        milliseconds as u64 + 1000 * (seconds as u64 + 60 * (minutes as u64 + 60 * hours as u64));
    Duration::from_millis(milliseconds)
}

pub fn parse_subtitle_file(path: impl AsRef<Path>) -> anyhow::Result<Vec<SubtitleChunk>> {
    let string = fs::read_to_string(path.as_ref())
        .with_context(|| format!("Failed to file at '{}',", path.as_ref().display()))?;
    parse_subtitle_content(string)
}

pub fn parse_subtitle_content(content: String) -> anyhow::Result<Vec<SubtitleChunk>> {
    let file = Subtitles::parse_from_str(content)?;

    Ok(file
        .to_vec()
        .iter()
        .filter(|x| !x.text.is_empty())
        .map(|x| SubtitleChunk {
            start: timestamp_to_duration(&x.start_time),
            end: timestamp_to_duration(&x.end_time),
            content: x.text.clone(),
        })
        .collect())
}
