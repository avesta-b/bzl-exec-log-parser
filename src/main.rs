// src/main.rs

mod proto;

use anyhow::{anyhow, Result};
use clap::Parser;
use prost::Message;
use proto::exec_log_entry::{self as compact, Type as CompactEntryType};
use proto::{ExecLogEntry, SpawnExec};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use zstd::stream::decode_all;

#[derive(Parser)]
#[command(name = "bzl-exec-log-analyzer")]
#[command(about = "Analyzes Bazel execution logs to extract performance metrics")]
#[command(version)]
struct Args {
    /// Path to the Bazel execution log file (auto-detects format)
    #[arg(help = "Path to the Bazel execution log file")]
    file: PathBuf,

    /// Number of slowest actions to display in the report
    #[arg(short, long, default_value_t = 10)]
    top_n: usize,

    /// Calculate and display remote cache performance metrics
    #[arg(long, default_value_t = true)]
    cache_metrics: bool,
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

/// An enum to hold different types of compact log entries for reconstruction.
enum StoredEntry {
    File(compact::File),
    Directory(compact::Directory),
}

fn main() -> Result<()> {
    let args = Args::parse();

    let spawns = parse_log_file(&args.file)?;

    if spawns.is_empty() {
        println!("Execution log is empty or contains no spawn actions. No metrics to report.");
        return Ok(());
    }
    println!(
        "Successfully parsed and reconstructed {} spawn entries from the log.",
        spawns.len()
    );

    // --- Print Main Report ---
    print_main_report(&spawns, &args);

    // --- Optionally, print cache metrics report ---
    if args.cache_metrics {
        print_cache_performance_report(&spawns);
    }

    Ok(())
}

/// Parses the log file, auto-detecting the format (compact or verbose).
fn parse_log_file(path: &Path) -> Result<Vec<SpawnExec>> {
    let raw_bytes = fs::read(path)?;

    // 1. Try parsing as a zstd-compressed compact log first.
    if let Ok(decompressed) = decode_all(raw_bytes.as_slice()) {
        if let Ok(spawns) = parse_compact_log(&decompressed) {
            println!("Detected zstd-compressed compact log format.");
            return Ok(spawns);
        }
    }

    // 2. Fallback to parsing as an uncompressed verbose log.
    println!("Could not parse as compact log. Falling back to verbose log format.");
    parse_verbose_log(&raw_bytes)
}

/// Parses the verbose execution log format (length-delimited SpawnExec protos).
fn parse_verbose_log(content: &[u8]) -> Result<Vec<SpawnExec>> {
    let mut decoded_spawns = Vec::new();
    let mut cursor = content;

    while !cursor.is_empty() {
        match SpawnExec::decode_length_delimited(&mut cursor) {
            Ok(spawn) => decoded_spawns.push(spawn),
            Err(e) => {
                return Err(anyhow!("Failed to parse verbose protobuf message: {}. The log file might be corrupt or in the wrong format.", e));
            }
        }
    }
    Ok(decoded_spawns)
}

/// Parses the compact execution log format and reconstructs SpawnExec messages.
fn parse_compact_log(content: &[u8]) -> Result<Vec<SpawnExec>> {
    let mut cursor = content;
    let mut stored_entries: HashMap<u32, StoredEntry> = HashMap::new();
    let mut reconstructed_spawns = Vec::new();

    while !cursor.is_empty() {
        let entry = ExecLogEntry::decode_length_delimited(&mut cursor)?;
        let id = entry.id;

        match entry.r#type {
            Some(CompactEntryType::Spawn(s)) => {
                let spawn_exec = reconstruct_spawn_exec(s, &stored_entries);
                reconstructed_spawns.push(spawn_exec);
            }
            Some(CompactEntryType::File(f)) if id != 0 => {
                stored_entries.insert(id, StoredEntry::File(f));
            }
            Some(CompactEntryType::Directory(d)) if id != 0 => {
                stored_entries.insert(id, StoredEntry::Directory(d));
            }
            // Ignore other entry types for now as they are not needed for the analysis.
            _ => {}
        }
    }
    Ok(reconstructed_spawns)
}

/// Converts a compact `Spawn` entry into a verbose `SpawnExec` using stored file/dir info.
fn reconstruct_spawn_exec(
    spawn: compact::Spawn,
    stored_entries: &HashMap<u32, StoredEntry>,
) -> SpawnExec {
    let mut actual_outputs = Vec::new();
    for output in spawn.outputs {
        if let Some(compact::output::Type::OutputId(id)) = output.r#type {
            if let Some(entry) = stored_entries.get(&id) {
                match entry {
                    StoredEntry::File(f) => {
                        actual_outputs.push(proto::File {
                            path: f.path.clone(),
                            digest: f.digest.clone(),
                            symlink_target_path: String::new(),
                            is_tool: false,
                        });
                    }
                    StoredEntry::Directory(d) => {
                        // The verbose format represents directories as a single File entry with a path.
                        // We will omit the digest as it's not directly available/needed for metrics.
                        actual_outputs.push(proto::File {
                            path: d.path.clone(),
                            digest: None,
                            symlink_target_path: String::new(),
                            is_tool: false,
                        });
                    }
                }
            }
        }
    }

    SpawnExec {
        command_args: spawn.args,
        environment_variables: spawn.env_vars,
        platform: spawn.platform,
        inputs: vec![],         // Not reconstructed as it's not used in analysis
        listed_outputs: vec![], // Not reconstructed as it's not used in analysis
        remotable: spawn.remotable,
        cacheable: spawn.cacheable,
        timeout_millis: spawn.timeout_millis,
        mnemonic: spawn.mnemonic,
        actual_outputs,
        runner: spawn.runner,
        cache_hit: spawn.cache_hit,
        status: spawn.status,
        exit_code: spawn.exit_code,
        remote_cacheable: spawn.remote_cacheable,
        target_label: spawn.target_label,
        digest: spawn.digest,
        metrics: spawn.metrics,
    }
}

// --- ANALYSIS AND REPORTING FUNCTIONS (UNCHANGED) ---

fn print_main_report(spawns: &[SpawnExec], args: &Args) {
    // ... (This function is identical to the original) ...
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
    slowest_actions.reverse();

    let mut mnemonic_metrics: HashMap<String, MnemonicMetrics> = HashMap::new();
    for spawn in spawns {
        let metrics = mnemonic_metrics.entry(spawn.mnemonic.clone()).or_default();
        metrics.count += 1;
        if spawn.cache_hit {
            metrics.cache_hits += 1;
        }
        if let Some(m) = spawn.metrics.as_ref().and_then(|m| m.total_time.as_ref()) {
            metrics.total_duration += to_std_duration(m);
        }
    }

    println!("========================================");
    println!(" Bazel Execution Log Analysis Report");
    println!("========================================");
    println!("Log file: {}\n", args.file.display());
    println!("--- Overall Summary ---");
    println!("Total Actions: {}", total_actions);
    println!(
        "Cache Hits: {} ({:.2}%)",
        cache_hits,
        (cache_hits as f64 / total_actions as f64) * 100.0
    );
    println!();
    println!("--- Top {} Slowest Actions ---", args.top_n);
    println!("{:<10} | {:<25} | {}", "Time", "Mnemonic", "Target");
    println!("---------------------------------------------------------------------------------");
    for spawn in slowest_actions.iter().take(args.top_n) {
        let duration = spawn
            .metrics
            .as_ref()
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

    // Calculate column widths based on actual data
    let mut sorted_mnemonics: Vec<_> = mnemonic_metrics.iter().collect();
    sorted_mnemonics.sort_by_key(|(_, metrics)| metrics.total_duration);
    sorted_mnemonics.reverse();

    let mnemonic_width = sorted_mnemonics
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(8)
        .max(8); // "Mnemonic" header

    let count_width = sorted_mnemonics
        .iter()
        .map(|(_, metrics)| metrics.count.to_string().len())
        .max()
        .unwrap_or(5)
        .max(5); // "Count" header

    let cache_hits_width = sorted_mnemonics
        .iter()
        .map(|(_, metrics)| {
            format!(
                "{:.1}%",
                (metrics.cache_hits as f64 / metrics.count as f64) * 100.0
            )
            .len()
        })
        .max()
        .unwrap_or(10)
        .max(10); // "Cache Hits" header

    let total_time_width = sorted_mnemonics
        .iter()
        .map(|(_, metrics)| format!("{:.2}s", metrics.total_duration.as_secs_f64()).len())
        .max()
        .unwrap_or(10)
        .max(10); // "Total Time" header

    let avg_time_width = sorted_mnemonics
        .iter()
        .map(|(_, metrics)| {
            let avg_time = if metrics.count > 0 {
                metrics.total_duration.as_secs_f64() / metrics.count as f64
            } else {
                0.0
            };
            format!("{:.3}s", avg_time).len()
        })
        .max()
        .unwrap_or(8)
        .max(8); // "Avg Time" header

    // Print header
    println!(
        "{:<width1$} | {:>width2$} | {:>width3$} | {:>width4$} | {:>width5$}",
        "Mnemonic",
        "Count",
        "Cache Hits",
        "Total Time",
        "Avg Time",
        width1 = mnemonic_width,
        width2 = count_width,
        width3 = cache_hits_width,
        width4 = total_time_width,
        width5 = avg_time_width
    );

    // Print separator line
    let separator_width =
        mnemonic_width + count_width + cache_hits_width + total_time_width + avg_time_width + 12; // 12 for " | " separators
    println!("{}", "-".repeat(separator_width));

    // Print data rows
    for (mnemonic, metrics) in sorted_mnemonics {
        let avg_time = if metrics.count > 0 {
            metrics.total_duration.as_secs_f64() / metrics.count as f64
        } else {
            0.0
        };
        println!(
            "{:<width1$} | {:>width2$} | {:>width3$.1}% | {:>width4$.2}s | {:>width5$.3}s",
            mnemonic,
            metrics.count,
            (metrics.cache_hits as f64 / metrics.count as f64) * 100.0,
            metrics.total_duration.as_secs_f64(),
            avg_time,
            width1 = mnemonic_width,
            width2 = count_width,
            width3 = cache_hits_width - 1, // -1 for the % symbol
            width4 = total_time_width - 1, // -1 for the s suffix
            width5 = avg_time_width - 1    // -1 for the s suffix
        );
    }
    println!();
}

fn print_cache_performance_report(spawns: &[SpawnExec]) {
    // ... (This function is identical to the original) ...
    let mut total_bytes_downloaded: i64 = 0;
    let mut total_fetch_time = Duration::ZERO;
    let mut remote_cache_hit_count = 0;

    for spawn in spawns {
        if spawn.runner == "remote cache hit" {
            remote_cache_hit_count += 1;
            let bytes_for_spawn: i64 = spawn
                .actual_outputs
                .iter()
                .filter_map(|file| file.digest.as_ref())
                .map(|digest| digest.size_bytes)
                .sum();
            total_bytes_downloaded += bytes_for_spawn;
            if let Some(fetch_duration) = spawn.metrics.as_ref().and_then(|m| m.fetch_time.as_ref())
            {
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
    println!(
        "Total Time Fetching from Cache: {:.2}s",
        total_fetch_seconds
    );
    if total_fetch_seconds > 0.001 {
        let download_rate_mbps = total_mb_downloaded / total_fetch_seconds;
        println!("Average Download Rate: {:.2} MB/s", download_rate_mbps);
    } else {
        println!("Average Download Rate: N/A (total fetch time is negligible)");
    }
    println!();
}

