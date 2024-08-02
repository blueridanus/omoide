use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Clone, Debug, Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Option<Commands>,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Commands {
    /// 日本語を練習してください
    Practice,
    /// Manage the install and training data
    Manage(ManageArgs),
    /// Stats on our data or performance
    Stats(StatsArgs),
    /// Analyse a sentence
    Analyse(AnalysisArgs),
    /// Find example sentences using a given word
    Examples(ExampleArgs),
}

#[derive(Clone, Debug, Args)]
pub struct ManageArgs {
    /// Download a bunch of data to form our own corpus for practicing against
    #[clap(long)]
    pub download: bool,
}

#[derive(Clone, Debug, Args)]
pub struct StatsArgs {
    /// Generate statistics for the words present in the subtitles
    #[clap(long)]
    pub word_stats: bool,
    /// Directory with subtitle files
    #[clap(long, short = 'd')]
    pub subtitles_dir: PathBuf,
}

#[derive(Clone, Debug, Args)]
pub struct ExampleArgs {
    /// Word to find example usage of in subs
    #[clap(long, short)]
    pub word: String,
    /// Directory with subtitle files
    #[clap(long, short = 'd')]
    pub subtitles_dir: PathBuf,
    /// Limit the maximum number of retrieved examples
    #[clap(long)]
    pub max: Option<usize>,
}

#[derive(Clone, Debug, Args)]
#[group(required = true, multiple = false)]
pub struct AnalysisArgs {
    /// An input sentence to analyse
    #[clap(long, short)]
    pub sentence: Vec<String>,
    /// Analyze all sentences in a file
    #[clap(long, short = 'f')]
    pub srt_file: Option<PathBuf>,
}
