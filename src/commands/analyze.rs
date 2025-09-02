use crate::cli::Cli;
use crate::proto::exec_log_entry::{self as compact, Type as CompactEntryType};
use crate::proto::{ExecLogEntry, SpawnExec};
use crate::{AppError, AppResult};
use prost::Message;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;
use zstd::stream::decode_all;

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

#[derive(Default)]
struct ExecutionTimings {
    count: u64,
    total_duration: Duration,
}

#[derive(Default)]
struct MnemonicExecutionStats {
    remote: ExecutionTimings,
    local: ExecutionTimings,
}

/// An enum to hold different types of compact log entries for reconstruction.
enum StoredEntry {
    File(compact::File),
    Directory(compact::Directory),
}

pub fn run_analyze(args: Cli) -> AppResult<()> {
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

    // --- Optional Reports ---
    if args.cache_metrics {
        print_cache_performance_report(&spawns);
    }
    if args.phase_timings {
        print_phase_timings_report(&spawns, args.top_n);
    }
    if args.input_analysis {
        print_input_analysis_report(&spawns, args.top_n);
    }
    if args.retries {
        print_retries_and_failures_report(&spawns);
    }

    // --- NEW REPORTS ---
    if args.aggregate_phases {
        print_aggregate_phases_report(&spawns);
    }
    if args.output_analysis {
        print_output_analysis_report(&spawns, args.top_n);
    }
    if args.memory_analysis {
        print_memory_analysis_report(&spawns, args.top_n);
    }
    if args.execution_comparison {
        print_execution_comparison_report(&spawns);
    }
    if args.queue_analysis {
        print_queue_analysis_report(&spawns, args.top_n);
    }

    Ok(())
}

/// Parses the log file, auto-detecting the format (compact or verbose).
fn parse_log_file(path: &Path) -> AppResult<Vec<SpawnExec>> {
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
fn parse_verbose_log(content: &[u8]) -> AppResult<Vec<SpawnExec>> {
    let mut decoded_spawns = Vec::new();
    let mut cursor = content;

    while !cursor.is_empty() {
        match SpawnExec::decode_length_delimited(&mut cursor) {
            Ok(spawn) => decoded_spawns.push(spawn),
            Err(e) => {
                return Err(AppError::LogParsing(format!("Failed to parse verbose protobuf message: {}. The log file might be corrupt or in the wrong format.", e)));
            }
        }
    }
    Ok(decoded_spawns)
}

/// Parses the compact execution log format and reconstructs SpawnExec messages.
fn parse_compact_log(content: &[u8]) -> AppResult<Vec<SpawnExec>> {
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
                        actual_outputs.push(crate::proto::File {
                            path: f.path.clone(),
                            digest: f.digest.clone(),
                            symlink_target_path: String::new(),
                            is_tool: false,
                        });
                    }
                    StoredEntry::Directory(d) => {
                        // The verbose format represents directories as a single File entry with a path.
                        // We will omit the digest as it's not directly available/needed for metrics.
                        actual_outputs.push(crate::proto::File {
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

// --- ANALYSIS AND REPORTING FUNCTIONS ---

fn print_main_report(spawns: &[SpawnExec], args: &Cli) {
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

fn print_phase_timings_report(spawns: &[SpawnExec], top_n: usize) {
    println!("--- Top {} Slowest Actions (Phase Timings) ---", top_n);
    println!("Note: This report excludes cache hits as phase timings are most relevant for executed actions.");

    let mut non_cache_hits: Vec<&SpawnExec> = spawns.iter().filter(|s| !s.cache_hit).collect();
    non_cache_hits.sort_by_key(|s| {
        s.metrics
            .as_ref()
            .and_then(|m| m.total_time.as_ref())
            .map(to_std_duration)
            .unwrap_or_default()
    });
    non_cache_hits.reverse();

    if non_cache_hits.is_empty() {
        println!("No executed actions found (all were cache hits).");
        println!();
        return;
    }

    // Calculate column widths based on actual data
    let actions_to_display = non_cache_hits.iter().take(top_n);
    
    let total_width = actions_to_display.clone()
        .map(|s| {
            let total = s.metrics.as_ref()
                .and_then(|m| m.total_time.as_ref())
                .map(to_std_duration)
                .unwrap_or_default();
            format!("{:.2}s", total.as_secs_f64()).len()
        })
        .max()
        .unwrap_or(5)
        .max(5); // "Total" header

    let queue_width = actions_to_display.clone()
        .map(|s| {
            let queue = s.metrics.as_ref()
                .and_then(|m| m.queue_time.as_ref())
                .map(to_std_duration)
                .unwrap_or_default();
            format!("{:.2}s", queue.as_secs_f64()).len()
        })
        .max()
        .unwrap_or(5)
        .max(5); // "Queue" header

    let setup_width = actions_to_display.clone()
        .map(|s| {
            let setup = s.metrics.as_ref()
                .and_then(|m| m.setup_time.as_ref())
                .map(to_std_duration)
                .unwrap_or_default();
            format!("{:.2}s", setup.as_secs_f64()).len()
        })
        .max()
        .unwrap_or(5)
        .max(5); // "Setup" header

    let upload_width = actions_to_display.clone()
        .map(|s| {
            let upload = s.metrics.as_ref()
                .and_then(|m| m.upload_time.as_ref())
                .map(to_std_duration)
                .unwrap_or_default();
            format!("{:.2}s", upload.as_secs_f64()).len()
        })
        .max()
        .unwrap_or(6)
        .max(6); // "Upload" header

    let execute_width = actions_to_display.clone()
        .map(|s| {
            let execution = s.metrics.as_ref()
                .and_then(|m| m.execution_wall_time.as_ref())
                .map(to_std_duration)
                .unwrap_or_default();
            format!("{:.2}s", execution.as_secs_f64()).len()
        })
        .max()
        .unwrap_or(7)
        .max(7); // "Execute" header

    let fetch_width = actions_to_display.clone()
        .map(|s| {
            let fetch = s.metrics.as_ref()
                .and_then(|m| m.fetch_time.as_ref())
                .map(to_std_duration)
                .unwrap_or_default();
            format!("{:.2}s", fetch.as_secs_f64()).len()
        })
        .max()
        .unwrap_or(5)
        .max(5); // "Fetch" header

    // Print header
    println!(
        "{:>width1$} | {:>width2$} | {:>width3$} | {:>width4$} | {:>width5$} | {:>width6$} | {}",
        "Total", "Queue", "Setup", "Upload", "Execute", "Fetch", "Target",
        width1 = total_width,
        width2 = queue_width,
        width3 = setup_width,
        width4 = upload_width,
        width5 = execute_width,
        width6 = fetch_width
    );
    
    // Print separator line
    let separator_width = total_width + queue_width + setup_width + upload_width + execute_width + fetch_width + 18 + 6; // separators + "Target"
    println!("{}", "-".repeat(separator_width));

    for spawn in non_cache_hits.iter().take(top_n) {
        if let Some(metrics) = spawn.metrics.as_ref() {
            let total = metrics.total_time.as_ref().map(to_std_duration).unwrap_or_default();
            let queue = metrics.queue_time.as_ref().map(to_std_duration).unwrap_or_default();
            let setup = metrics.setup_time.as_ref().map(to_std_duration).unwrap_or_default();
            let upload = metrics.upload_time.as_ref().map(to_std_duration).unwrap_or_default();
            let execution = metrics.execution_wall_time.as_ref().map(to_std_duration).unwrap_or_default();
            let fetch = metrics.fetch_time.as_ref().map(to_std_duration).unwrap_or_default();

            // Calculate overhead for display
            let overhead_pct = if total.as_secs_f64() > 0.0 {
                (total - execution).as_secs_f64() / total.as_secs_f64() * 100.0
            } else {
                0.0
            };

            println!(
                "{:>width1$.2}s | {:>width2$.2}s | {:>width3$.2}s | {:>width4$.2}s | {:>width5$.2}s | {:>width6$.2}s | {}",
                total.as_secs_f64(),
                queue.as_secs_f64(),
                setup.as_secs_f64(),
                upload.as_secs_f64(),
                execution.as_secs_f64(),
                fetch.as_secs_f64(),
                spawn.target_label,
                width1 = total_width - 1, // -1 for 's' suffix
                width2 = queue_width - 1,
                width3 = setup_width - 1,
                width4 = upload_width - 1,
                width5 = execute_width - 1,
                width6 = fetch_width - 1
            );
            println!("  └ Overhead: {:.1}%", overhead_pct);
        }
    }
    println!();
}

fn print_input_analysis_report(spawns: &[SpawnExec], top_n: usize) {
    println!("--- Top {} Actions by Input Size ---", top_n);

    let mut sorted_by_size = spawns.to_vec();
    sorted_by_size.sort_by_key(|s| s.metrics.as_ref().map_or(0, |m| m.input_bytes));
    sorted_by_size.reverse();

    // Filter out actions with no input data
    let actions_with_inputs: Vec<_> = sorted_by_size
        .iter()
        .filter(|s| s.metrics.as_ref().map_or(false, |m| m.input_bytes > 0))
        .collect();

    if actions_with_inputs.is_empty() {
        println!("No actions with input size data found in the log.");
        println!();
        return;
    }

    // Calculate column widths based on actual data
    let actions_to_display = actions_with_inputs.iter().take(top_n);
    
    let size_width = actions_to_display.clone()
        .map(|s| {
            let size_mb = s.metrics.as_ref().unwrap().input_bytes as f64 / 1_048_576.0;
            format!("{:.2}MB", size_mb).len()
        })
        .max()
        .unwrap_or(10)
        .max(10); // "Input Size" header

    let files_width = actions_to_display.clone()
        .map(|s| s.metrics.as_ref().unwrap().input_files.to_string().len())
        .max()
        .unwrap_or(11)
        .max(11); // "Input Files" header

    // Print header
    println!(
        "{:>width1$} | {:>width2$} | {}",
        "Input Size", "Input Files", "Target",
        width1 = size_width,
        width2 = files_width
    );
    
    // Print separator line
    let separator_width = size_width + files_width + 6 + 6; // separators + "Target"
    println!("{}", "-".repeat(separator_width));

    for spawn in actions_with_inputs.iter().take(top_n) {
        if let Some(metrics) = spawn.metrics.as_ref() {
            println!(
                "{:>width1$.2}MB | {:>width2$} | {}",
                metrics.input_bytes as f64 / 1_048_576.0,
                metrics.input_files,
                spawn.target_label,
                width1 = size_width - 2, // -2 for "MB" suffix
                width2 = files_width
            );
        }
    }
    println!();
}

fn print_retries_and_failures_report(spawns: &[SpawnExec]) {
    println!("--- Actions with Failures or Retries ---");

    let problematic_spawns: Vec<_> = spawns
        .iter()
        .filter(|s| {
            !s.status.is_empty() || s.metrics.as_ref().map_or(false, |m| {
                m.retry_time.as_ref().map_or(false, |d| d.seconds > 0 || d.nanos > 0)
            })
        })
        .collect();

    if problematic_spawns.is_empty() {
        println!("No actions with failures or retries found.");
    } else {
        for spawn in problematic_spawns {
            let retry_duration = spawn
                .metrics
                .as_ref()
                .and_then(|m| m.retry_time.as_ref())
                .map(to_std_duration)
                .unwrap_or_default();
            
            println!("Target: {}", spawn.target_label);
            if !spawn.status.is_empty() {
                println!("  └ Status: {} (Exit Code: {})", spawn.status, spawn.exit_code);
            }
            if !retry_duration.is_zero() {
                println!("  └ Time in Retries: {:.3}s", retry_duration.as_secs_f64());
            }
        }
    }
    println!();
}

fn print_aggregate_phases_report(spawns: &[SpawnExec]) {
    println!("--- Aggregate Phase Timings (Executed Actions) ---");
    
    let mut total_time = Duration::ZERO;
    let mut total_queue = Duration::ZERO;
    let mut total_setup = Duration::ZERO;
    let mut total_upload = Duration::ZERO;
    let mut total_execution = Duration::ZERO;
    let mut total_fetch = Duration::ZERO;
    let mut total_retry = Duration::ZERO;
    
    let mut executed_count = 0;
    
    for spawn in spawns {
        if !spawn.cache_hit {
            executed_count += 1;
            if let Some(metrics) = spawn.metrics.as_ref() {
                if let Some(d) = metrics.total_time.as_ref() {
                    total_time += to_std_duration(d);
                }
                if let Some(d) = metrics.queue_time.as_ref() {
                    total_queue += to_std_duration(d);
                }
                if let Some(d) = metrics.setup_time.as_ref() {
                    total_setup += to_std_duration(d);
                }
                if let Some(d) = metrics.upload_time.as_ref() {
                    total_upload += to_std_duration(d);
                }
                if let Some(d) = metrics.execution_wall_time.as_ref() {
                    total_execution += to_std_duration(d);
                }
                if let Some(d) = metrics.fetch_time.as_ref() {
                    total_fetch += to_std_duration(d);
                }
                if let Some(d) = metrics.retry_time.as_ref() {
                    total_retry += to_std_duration(d);
                }
            }
        }
    }
    
    if executed_count == 0 {
        println!("No executed actions found (all were cache hits).");
        println!();
        return;
    }
    
    let total_seconds = total_time.as_secs_f64();
    
    println!("Executed Actions: {}", executed_count);
    println!("Total Execution Time: {:.2}s", total_seconds);
    println!();
    
    println!("{:<15} | {:>10} | {:>8}", "Phase", "Time", "% of Total");
    println!("----------------------------------------");
    
    let phases = [
        ("Queue", total_queue),
        ("Setup", total_setup),
        ("Upload", total_upload),
        ("Execution", total_execution),
        ("Fetch", total_fetch),
        ("Retry", total_retry),
    ];
    
    for (name, duration) in phases {
        let seconds = duration.as_secs_f64();
        let percentage = if total_seconds > 0.0 {
            (seconds / total_seconds) * 100.0
        } else {
            0.0
        };
        println!("{:<15} | {:>10.2}s | {:>7.1}%", name, seconds, percentage);
    }
    println!();
}

fn print_output_analysis_report(spawns: &[SpawnExec], top_n: usize) {
    println!("--- Top {} Actions by Output Size ---", top_n);
    
    let mut size_data: Vec<(i64, &SpawnExec)> = Vec::new();
    
    for spawn in spawns {
        let total_output_size: i64 = spawn
            .actual_outputs
            .iter()
            .filter_map(|file| file.digest.as_ref())
            .map(|digest| digest.size_bytes)
            .sum();
        
        if total_output_size > 0 {
            size_data.push((total_output_size, spawn));
        }
    }
    
    if size_data.is_empty() {
        println!("No actions with output size data found in the log.");
        println!();
        return;
    }
    
    size_data.sort_by_key(|(size, _)| *size);
    size_data.reverse();
    
    // Calculate column widths based on actual data
    let actions_to_display = size_data.iter().take(top_n);
    
    let size_width = actions_to_display.clone()
        .map(|(size, _)| {
            let size_mb = *size as f64 / 1_048_576.0;
            format!("{:.2}MB", size_mb).len()
        })
        .max()
        .unwrap_or(11)
        .max(11); // "Output Size" header
    
    let files_width = actions_to_display.clone()
        .map(|(_, spawn)| spawn.actual_outputs.len().to_string().len())
        .max()
        .unwrap_or(12)
        .max(12); // "Output Files" header
    
    // Print header
    println!(
        "{:>width1$} | {:>width2$} | {}",
        "Output Size", "Output Files", "Target",
        width1 = size_width,
        width2 = files_width
    );
    
    // Print separator line
    let separator_width = size_width + files_width + 6 + 6; // separators + "Target"
    println!("{}", "-".repeat(separator_width));
    
    for (size, spawn) in size_data.iter().take(top_n) {
        println!(
            "{:>width1$.2}MB | {:>width2$} | {}",
            *size as f64 / 1_048_576.0,
            spawn.actual_outputs.len(),
            spawn.target_label,
            width1 = size_width - 2, // -2 for "MB" suffix
            width2 = files_width
        );
    }
    println!();
}

fn print_memory_analysis_report(spawns: &[SpawnExec], top_n: usize) {
    println!("--- Top {} Actions by Memory Usage vs. Limit ---", top_n);
    
    let mut memory_data: Vec<(f64, &SpawnExec)> = Vec::new();
    
    for spawn in spawns {
        if let Some(metrics) = spawn.metrics.as_ref() {
            if metrics.memory_bytes_limit > 0 {
                let usage_ratio = metrics.memory_estimate_bytes as f64 / metrics.memory_bytes_limit as f64;
                memory_data.push((usage_ratio, spawn));
            }
        }
    }
    
    if memory_data.is_empty() {
        println!("No actions with memory limit data found in the log.");
        println!();
        return;
    }
    
    memory_data.sort_by(|(a, _), (b, _)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    
    // Calculate column widths based on actual data
    let actions_to_display = memory_data.iter().take(top_n);
    
    let estimate_width = actions_to_display.clone()
        .map(|(_, spawn)| {
            let estimate_mb = spawn.metrics.as_ref().unwrap().memory_estimate_bytes as f64 / 1_048_576.0;
            format!("{:.1}MB", estimate_mb).len()
        })
        .max()
        .unwrap_or(12)
        .max(12); // "Memory Used" header
    
    let limit_width = actions_to_display.clone()
        .map(|(_, spawn)| {
            let limit_mb = spawn.metrics.as_ref().unwrap().memory_bytes_limit as f64 / 1_048_576.0;
            format!("{:.1}MB", limit_mb).len()
        })
        .max()
        .unwrap_or(13)
        .max(13); // "Memory Limit" header
    
    let usage_width = 7; // "Usage %" header
    
    // Print header
    println!(
        "{:>width1$} | {:>width2$} | {:>width3$} | {}",
        "Memory Used", "Memory Limit", "Usage %", "Target",
        width1 = estimate_width,
        width2 = limit_width,
        width3 = usage_width
    );
    
    // Print separator line
    let separator_width = estimate_width + limit_width + usage_width + 6 + 9; // separators + "Target"
    println!("{}", "-".repeat(separator_width));
    
    for (ratio, spawn) in memory_data.iter().take(top_n) {
        let metrics = spawn.metrics.as_ref().unwrap();
        let estimate_mb = metrics.memory_estimate_bytes as f64 / 1_048_576.0;
        let limit_mb = metrics.memory_bytes_limit as f64 / 1_048_576.0;
        let usage_pct = ratio * 100.0;
        
        println!(
            "{:>width1$.1}MB | {:>width2$.1}MB | {:>width3$.1}% | {}",
            estimate_mb,
            limit_mb,
            usage_pct,
            spawn.target_label,
            width1 = estimate_width - 2, // -2 for "MB" suffix
            width2 = limit_width - 2,    // -2 for "MB" suffix
            width3 = usage_width - 1     // -1 for "%" suffix
        );
    }
    println!();
}

fn print_execution_comparison_report(spawns: &[SpawnExec]) {
    println!("--- Remote vs. Local Execution Time Comparison ---");
    
    let mut mnemonic_stats: HashMap<String, MnemonicExecutionStats> = HashMap::new();
    
    for spawn in spawns {
        if !spawn.cache_hit {
            if let Some(metrics) = spawn.metrics.as_ref() {
                if let Some(execution_time) = metrics.execution_wall_time.as_ref() {
                    let duration = to_std_duration(execution_time);
                    let stats = mnemonic_stats.entry(spawn.mnemonic.clone()).or_default();
                    
                    if spawn.runner.contains("remote") {
                        stats.remote.count += 1;
                        stats.remote.total_duration += duration;
                    } else if spawn.runner.contains("sandbox") || spawn.runner.contains("local") {
                        stats.local.count += 1;
                        stats.local.total_duration += duration;
                    }
                }
            }
        }
    }
    
    // Filter for mnemonics that have both remote and local executions
    let comparable_mnemonics: Vec<_> = mnemonic_stats
        .iter()
        .filter(|(_, stats)| stats.remote.count > 0 && stats.local.count > 0)
        .collect();
    
    if comparable_mnemonics.is_empty() {
        println!("No mnemonics found with both remote and local executions.");
        println!();
        return;
    }
    
    // Calculate column widths
    let mnemonic_width = comparable_mnemonics
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(8)
        .max(8); // "Mnemonic" header
    
    let count_width = 8; // "Remote/Local" headers
    let time_width = 10; // "Avg Time" headers
    
    // Print header
    println!(
        "{:<width1$} | {:>width2$} | {:>width3$} | {:>width2$} | {:>width3$} | {:>12}",
        "Mnemonic", "Remote", "Avg Time", "Local", "Avg Time", "Difference",
        width1 = mnemonic_width,
        width2 = count_width,
        width3 = time_width
    );
    
    // Print separator line
    let separator_width = mnemonic_width + count_width * 2 + time_width * 2 + 12 + 15; // separators
    println!("{}", "-".repeat(separator_width));
    
    let mut sorted_mnemonics = comparable_mnemonics;
    sorted_mnemonics.sort_by(|(a, _), (b, _)| a.cmp(b));
    
    for (mnemonic, stats) in sorted_mnemonics {
        let remote_avg = if stats.remote.count > 0 {
            stats.remote.total_duration.as_secs_f64() / stats.remote.count as f64
        } else {
            0.0
        };
        
        let local_avg = if stats.local.count > 0 {
            stats.local.total_duration.as_secs_f64() / stats.local.count as f64
        } else {
            0.0
        };
        
        let difference_ratio = if local_avg > 0.0 {
            remote_avg / local_avg
        } else {
            0.0
        };
        
        let difference_text = if difference_ratio > 1.0 {
            format!("{:.1}x slower", difference_ratio)
        } else if difference_ratio > 0.0 && difference_ratio < 1.0 {
            format!("{:.1}x faster", 1.0 / difference_ratio)
        } else {
            "N/A".to_string()
        };
        
        println!(
            "{:<width1$} | {:>width2$} | {:>width3$.3}s | {:>width2$} | {:>width3$.3}s | {:>12}",
            mnemonic,
            stats.remote.count,
            remote_avg,
            stats.local.count,
            local_avg,
            difference_text,
            width1 = mnemonic_width,
            width2 = count_width,
            width3 = time_width - 1 // -1 for 's' suffix
        );
    }
    println!();
}

fn print_queue_analysis_report(spawns: &[SpawnExec], top_n: usize) {
    println!("--- Top {} Actions by Queue Time ---", top_n);
    
    let mut non_cache_hits: Vec<&SpawnExec> = spawns.iter().filter(|s| !s.cache_hit).collect();
    
    if non_cache_hits.is_empty() {
        println!("No executed actions found (all were cache hits).");
        println!();
        return;
    }
    
    non_cache_hits.sort_by_key(|s| {
        s.metrics
            .as_ref()
            .and_then(|m| m.queue_time.as_ref())
            .map(to_std_duration)
            .unwrap_or_default()
    });
    non_cache_hits.reverse();
    
    // Calculate column widths based on actual data
    let actions_to_display = non_cache_hits.iter().take(top_n);
    
    let queue_width = actions_to_display.clone()
        .map(|s| {
            let queue_time = s.metrics.as_ref()
                .and_then(|m| m.queue_time.as_ref())
                .map(to_std_duration)
                .unwrap_or_default();
            format!("{:.2}s", queue_time.as_secs_f64()).len()
        })
        .max()
        .unwrap_or(10)
        .max(10); // "Queue Time" header
    
    let total_width = actions_to_display.clone()
        .map(|s| {
            let total_time = s.metrics.as_ref()
                .and_then(|m| m.total_time.as_ref())
                .map(to_std_duration)
                .unwrap_or_default();
            format!("{:.2}s", total_time.as_secs_f64()).len()
        })
        .max()
        .unwrap_or(10)
        .max(10); // "Total Time" header
    
    // Print header
    println!(
        "{:>width1$} | {:>width2$} | {}",
        "Queue Time", "Total Time", "Target",
        width1 = queue_width,
        width2 = total_width
    );
    
    // Print separator line
    let separator_width = queue_width + total_width + 6 + 6; // separators + "Target"
    println!("{}", "-".repeat(separator_width));
    
    for spawn in non_cache_hits.iter().take(top_n) {
        if let Some(metrics) = spawn.metrics.as_ref() {
            let queue_time = metrics.queue_time.as_ref().map(to_std_duration).unwrap_or_default();
            let total_time = metrics.total_time.as_ref().map(to_std_duration).unwrap_or_default();
            
            println!(
                "{:>width1$.2}s | {:>width2$.2}s | {}",
                queue_time.as_secs_f64(),
                total_time.as_secs_f64(),
                spawn.target_label,
                width1 = queue_width - 1, // -1 for 's' suffix
                width2 = total_width - 1  // -1 for 's' suffix
            );
        }
    }
    println!();
}