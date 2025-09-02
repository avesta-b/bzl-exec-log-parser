use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "bzl-exec-log-analyzer")]
#[command(about = "Analyzes Bazel execution logs to extract performance metrics")]
#[command(version)]
pub struct Cli {
    /// Path to the Bazel execution log file (auto-detects format)
    #[arg(help = "Path to the Bazel execution log file")]
    pub file: PathBuf,

    /// Number of slowest actions to display in the report
    #[arg(short, long, default_value_t = 10)]
    pub top_n: usize,

    /// Calculate and display remote cache performance metrics
    #[arg(long, default_value_t = true)]
    pub cache_metrics: bool,

    /// Display a detailed breakdown of action phase timings for slowest actions
    #[arg(long)]
    pub phase_timings: bool,

    /// Display a report on actions with the largest input sizes
    #[arg(long)]
    pub input_analysis: bool,

    /// Display a report on actions that failed or were retried
    #[arg(long)]
    pub retries: bool,

    /// Display an aggregate summary of time spent in each execution phase
    #[arg(long)]
    pub aggregate_phases: bool,

    /// Display a report on actions with the largest output sizes
    #[arg(long)]
    pub output_analysis: bool,

    /// Display a report on actions with the highest memory usage relative to their limit
    #[arg(long)]
    pub memory_analysis: bool,

    /// Display a comparison of remote vs. local execution times by mnemonic
    #[arg(long)]
    pub execution_comparison: bool,

    /// Display a report on actions with the longest queue times
    #[arg(long)]
    pub queue_analysis: bool,
}