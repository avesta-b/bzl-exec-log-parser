# Bazel Execution Log Analyzer

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A command-line tool written in Rust to parse and analyze Bazel execution logs. It helps diagnose build performance issues, especially related to remote caching, action scheduling, and resource consumption.

The tool automatically detects and parses both the verbose (`--execution_log_binary_file`) and the zstd-compressed compact (`--experimental_execution_log_compact_file`) log formats.

## Goal

The primary goal of this project is to provide developers with actionable insights into their Bazel builds. By analyzing the rich data within execution logs, this tool helps answer critical questions like:

- How effective is our remote cache?
- What are the slowest actions in the build, and where is their time spent (queueing, setup, execution, upload)?
- Which actions are consuming the most resources (inputs/outputs size, memory)?
- Are there frequent action failures or retries slowing things down?
- How do remote and local execution times compare for the same type of action?

By pinpointing these bottlenecks, teams can optimize their CI/CD pipelines, improve developer productivity, and reduce build costs.

## Features

- **Auto-detects Log Format:** Seamlessly handles both verbose and zstd-compressed compact execution logs.
- **Overall Summary:** Provides a high-level report including total actions, cache hit rate, and a breakdown of time spent by action type (mnemonic).
- **Slowest Actions:** Identifies the top N slowest actions to focus optimization efforts.
- **Remote Cache Metrics:** Calculates total data downloaded from the remote cache and the average download speed.
- **Detailed Phase Timings:** Breaks down the lifecycle of the slowest actions into distinct phases (e.g., `queue`, `setup`, `execution`, `upload`, `fetch`).
- **Resource Analysis:** Reports on actions with the largest input/output sizes and highest memory usage.
- **Failure & Retry Report:** Highlights actions that failed or required retries.
- **Remote vs. Local Comparison:** Compares the average execution time for actions that ran both remotely and locally.
- **Queue Time Analysis:** Pinpoints actions that spent the most time waiting for an available executor.

## Usage

### 1. Generate an Execution Log from Bazel

You can generate an execution log by adding one of the following flags to your `bazel build` or `bazel test` command. The compact format is recommended as it has lower overhead.

**Recommended (Compact Format):**
Bazel will automatically compress the output with `zstd` if the filename ends with `.zstd`.

```bash
bazel build //... --experimental_execution_log_compact_file=/tmp/exec.log.zstd
```

**Legacy (Verbose Format):**

```bash
bazel build //... --execution_log_binary_file=/tmp/exec.log
```

### 2. Run the Analyzer

Use `cargo run --release` to build and run the tool, passing the path to your log file. The arguments after `--` are passed directly to the analyzer.

```bash
cargo run --release -- /tmp/exec.log.zst
```

To see a more detailed report, enable additional analysis flags:

```bash
cargo run --release -- /tmp/exec.log.zst \
    --top-n 15 \
    --phase-timings \
    --input-analysis \
    --memory-analysis
```

### Command-Line Flags

```text
Usage: bzl-exec-log-analyzer <FILE> [OPTIONS]

Arguments:
  <FILE>  Path to the Bazel execution log file

Options:
  -n, --top-n <TOP_N>
          Number of slowest actions to display in the report
          [default: 10]
      --cache-metrics
          Calculate and display remote cache performance metrics
          [default: true]
      --phase-timings
          Display a detailed breakdown of action phase timings for slowest actions
      --input-analysis
          Display a report on actions with the largest input sizes
      --retries
          Display a report on actions that failed or were retried
      --aggregate-phases
          Display an aggregate summary of time spent in each execution phase
      --output-analysis
          Display a report on actions with the largest output sizes
      --memory-analysis
          Display a report on actions with the highest memory usage relative to their limit
      --execution-comparison
          Display a comparison of remote vs. local execution times by mnemonic
      --queue-analysis
          Display a report on actions with the longest queue times
  -h, --help
          Print help
  -V, --version
          Print version
```

## Project Structure

The project is organized into several modules:

- `src/main.rs`: The main binary entry point.
- `src/lib.rs`: The main library entry point, responsible for parsing CLI args and calling the command logic.
- `src/cli.rs`: Defines the command-line interface using `clap`.
- `src/commands/analyze.rs`: Contains the core logic for parsing log files, reconstructing data, performing all analyses, and printing reports. It handles both verbose and compact log formats.
- `src/error.rs`: Defines custom error types for the application.
- `src/proto/`: Contains the protobuf definitions (`spawn.proto`) and the Rust code generated by `prost`.
- `build.rs`: A build script that uses `prost-build` to compile `spawn.proto` into Rust code during the build process.

## License

This project is licensed under the **MIT License**. See the [LICENSE](LICENSE) file for details.
