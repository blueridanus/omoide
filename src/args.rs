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
    Analyse(AnalyseArgs),
}

#[derive(Clone, Debug, Args)]
pub struct ManageArgs {
    /// Download a bunch of data to form our own corpus for practicing against
    #[clap(long)]
    pub download: bool,
}

#[derive(Clone, Debug, Args)]
pub struct AnalyseArgs {
    /// Generate statistics for the words present in the subtitles
    #[clap(long)]
    pub word_stats: bool,
}
