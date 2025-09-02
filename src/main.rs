mod proto;

use anyhow::Result;
use clap::Parser;
use prost::Message;
use proto::SpawnExec;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(clap::ValueEnum, Clone, Debug)]
enum LogFormat {
    Binary,
    Json,
}

#[derive(Parser)]
#[command(name = "bzl-exec-log-analyzer")]
#[command(about = "Analyzes a Bazel execution log to extract performance metrics.")]
#[command(version)]
struct Args {
    /// Path to the Bazel execution log file.
    /// Can be generated with --execution_log_json_file=<path> (for JSON)
    /// or --execution_log_binary_file=<path> (for binary protobuf).
    #[arg(help = "Path to the Bazel execution log file")]
    file: PathBuf,

    /// Number of slowest actions to display in the report
    #[arg(short, long, default_value_t = 10)]
    top_n: usize,

    /// Calculate and display remote cache performance metrics
    #[arg(long)]
    cache_metrics: bool,

    /// Specify the format of the log file. Tries to auto-detect from extension if not provided.
    #[arg(short, long, value_enum)]
    format: Option<LogFormat>,
}

/// Helper to convert prost's Duration to std's Duration
fn to_std_duration(prost_duration: &prost_types::Duration) -> Duration {
    Duration::new(
        prost_duration.seconds.try_into().unwrap_or(0),
        prost_duration.nanos.try_into().unwrap_or(0),
    )
}

#[derive(Default)]
struct MnemonicMetrics {
    count: u64,
    cache_hits: u64,
    total_duration: Duration,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // 1. Read the file
    let content = fs::read(&args.file)?;

    // Determine format from flag or file extension
    let format = args.format.clone().unwrap_or_else(|| {
        if args.file.extension() == Some(std::ffi::OsStr::new("json")) {
            LogFormat::Json
        } else {
            LogFormat::Binary
        }
    });

    // 2. Parse the file based on format
    let spawns: Vec<SpawnExec> = match format {
        LogFormat::Json => {
            // The JSON log is a stream of JSON objects, not a single array.
            // We use a streaming deserializer to handle this efficiently.
            let deserializer = serde_json::Deserializer::from_slice(&content);
            let iterator = deserializer.into_iter::<SpawnExec>();
            let mut decoded_spawns = Vec::new();

            for item in iterator {
                match item {
                    Ok(spawn) => decoded_spawns.push(spawn),
                    Err(e) => {
                        // Provide a more helpful error if parsing fails mid-stream
                        return Err(anyhow::anyhow!(
                            "Failed to parse JSON object in stream at line {}: {}. Ensure the file is a valid JSON stream.",
                            e.line(), e
                        ));
                    }
                }
            }
            decoded_spawns
        }
        LogFormat::Binary => {
            // Parse as length-delimited protobuf binary format
            let mut decoded_spawns = Vec::new();
            let mut cursor = content.as_slice();
            
            while !cursor.is_empty() {
                match SpawnExec::decode_length_delimited(&mut cursor) {
                    Ok(spawn) => decoded_spawns.push(spawn),
                    Err(_) => break, // End of valid data
                }
            }
            decoded_spawns
        }
    };

    if spawns.is_empty() {
        println!("Execution log is empty or could not be parsed. No metrics to report.");
        return Ok(());
    }

    // --- Print Main Report ---
    print_main_report(&spawns, &args);

    // --- Optionally, print cache metrics report ---
    if args.cache_metrics {
        print_cache_performance_report(&spawns);
    }

    Ok(())
}

fn print_main_report(spawns: &[SpawnExec], args: &Args) {
    // 2. Analyze the spawns
    let total_actions = spawns.len();
    let cache_hits = spawns.iter().filter(|s| s.cache_hit).count();

    let mut slowest_actions: Vec<&SpawnExec> = spawns.iter().collect();
    slowest_actions.sort_by_key(|s| {
        s.metrics
            .as_ref()
            .and_then(|m| m.total_time.as_ref())
            .map(to_std_duration)
            .unwrap_or_default()
    });
    slowest_actions.reverse(); // Now from slowest to fastest

    let mut mnemonic_metrics: HashMap<String, MnemonicMetrics> = HashMap::new();
    for spawn in spawns {
        let metrics = mnemonic_metrics
            .entry(spawn.mnemonic.clone())
            .or_default();
        metrics.count += 1;
        if spawn.cache_hit {
            metrics.cache_hits += 1;
        }
        if let Some(m) = spawn.metrics.as_ref().and_then(|m| m.total_time.as_ref()) {
            metrics.total_duration += to_std_duration(m);
        }
    }

    // 3. Print the report
    println!("========================================");
    println!(" Bazel Execution Log Analysis Report");
    println!("========================================");
    println!("Log file: {}\n", args.file.display());

    println!("--- Overall Summary ---");
    println!("Total Actions: {}", total_actions);
    println!("Cache Hits: {} ({:.2}%)", cache_hits, (cache_hits as f64 / total_actions as f64) * 100.0);
    println!();


    println!("--- Top {} Slowest Actions ---", args.top_n);
    println!("{:<10} | {:<25} | {}", "Time", "Mnemonic", "Target");
    println!("---------------------------------------------------------------------------------");
    for spawn in slowest_actions.iter().take(args.top_n) {
        let duration = spawn.metrics.as_ref()
            .and_then(|m| m.total_time.as_ref())
            .map(to_std_duration)
            .unwrap_or_default();
        
        println!(
            "{:<10.3}s | {:<25} | {}",
            duration.as_secs_f64(),
            spawn.mnemonic,
            spawn.target_label
        );
    }
    println!();


    println!("--- Analysis by Mnemonic ---");
    println!("{:<25} | {:>10} | {:>10} | {:>10} | {:>10}", "Mnemonic", "Count", "Cache Hits", "Total Time", "Avg Time");
    println!("---------------------------------------------------------------------------------");

    let mut sorted_mnemonics: Vec<_> = mnemonic_metrics.iter().collect();
    sorted_mnemonics.sort_by_key(|(_, metrics)| metrics.total_duration);
    sorted_mnemonics.reverse();

    for (mnemonic, metrics) in sorted_mnemonics {
        let avg_time = if metrics.count > 0 {
            metrics.total_duration.as_secs_f64() / metrics.count as f64
        } else {
            0.0
        };

        println!(
            "{:<25} | {:>10} | {:>10.1}% | {:>10.2}s | {:>10.3}s",
            mnemonic,
            metrics.count,
            (metrics.cache_hits as f64 / metrics.count as f64) * 100.0,
            metrics.total_duration.as_secs_f64(),
            avg_time
        );
    }
    println!();
}

fn print_cache_performance_report(spawns: &[SpawnExec]) {
    let mut total_bytes_downloaded: i64 = 0;
    let mut total_fetch_time = Duration::ZERO;
    let mut remote_cache_hit_count = 0;

    for spawn in spawns {
        // Filter for spawns that were served by the remote cache
        if spawn.runner == "remote cache hit" {
            remote_cache_hit_count += 1;

            // Sum the size of all output files for this spawn
            let bytes_for_spawn: i64 = spawn.actual_outputs.iter()
                .filter_map(|file| file.digest.as_ref())
                .map(|digest| digest.size_bytes)
                .sum();
            total_bytes_downloaded += bytes_for_spawn;

            // Add the time spent fetching remote outputs
            if let Some(fetch_duration) = spawn.metrics.as_ref().and_then(|m| m.fetch_time.as_ref()) {
                total_fetch_time += to_std_duration(fetch_duration);
            }
        }
    }

    println!("--- Remote Cache Performance ---");

    if remote_cache_hit_count == 0 {
        println!("No remote cache hits found in the log.");
        println!();
        return;
    }

    let total_mb_downloaded = total_bytes_downloaded as f64 / 1_000_000.0;
    let total_fetch_seconds = total_fetch_time.as_secs_f64();

    println!("Remote Cache Hits Count: {}", remote_cache_hit_count);
    println!("Total Data Downloaded: {:.2} MB", total_mb_downloaded);
    println!("Total Time Fetching from Cache: {:.2}s", total_fetch_seconds);

    if total_fetch_seconds > 0.001 {
        let download_rate_mbps = total_mb_downloaded / total_fetch_seconds;
        println!("Average Download Rate: {:.2} MB/s", download_rate_mbps);
    } else {
        println!("Average Download Rate: N/A (total fetch time is negligible)");
    }
    println!();
}
