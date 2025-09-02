mod proto;

use clap::{Parser, ValueEnum};
use proto::*;
use serde::{Deserialize, Serialize};
use serde_json;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DigestJson {
    hash: String,
    size_bytes: i64,
    hash_function_name: String,
}

#[derive(Clone, Debug, ValueEnum)]
enum OutputFormat {
    /// Output in JSON format (default)
    Json,
    /// Output in Protobuf format
    Protobuf,
}

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::Json
    }
}

#[derive(Parser)]
#[command(name = "bzl-exec-log-parser")]
#[command(about = "A Bazel execution log parser")]
#[command(version)]
struct Args {
    /// Path to the execution log file to analyze
    #[arg(help = "Path to the Bazel execution log file")]
    file: PathBuf,
    
    /// Output format
    #[arg(short = 'f', long = "format", value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,
    
    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();
    
    if args.verbose {
        println!("Analyzing file: {:?}", args.file);
        println!("Output format: {:?}", args.format);
    }
    
    // Check if file exists
    if !args.file.exists() {
        eprintln!("Error: File {:?} does not exist", args.file);
        std::process::exit(1);
    }
    
    // TODO: Add actual file parsing logic here
    if args.verbose {
        println!("File exists and ready for parsing!");
    }
    
    // Example: Create a new Digest message
    let digest = Digest {
        hash: "abc123".to_string(),
        size_bytes: 1024,
        hash_function_name: "SHA256".to_string(),
    };
    
    // Output based on selected format
    match args.format {
        OutputFormat::Json => {
            output_json(&digest, args.verbose);
        }
        OutputFormat::Protobuf => {
            output_protobuf(&digest, args.verbose);
        }
    }
}

fn output_json(digest: &Digest, verbose: bool) {
    // Convert protobuf struct to JSON-serializable struct
    let digest_json = DigestJson {
        hash: digest.hash.clone(),
        size_bytes: digest.size_bytes,
        hash_function_name: digest.hash_function_name.clone(),
    };
    
    match serde_json::to_string_pretty(&digest_json) {
        Ok(json_output) => {
            if verbose {
                println!("JSON output:");
            }
            println!("{}", json_output);
        }
        Err(e) => {
            eprintln!("Error serializing to JSON: {}", e);
            std::process::exit(1);
        }
    }
}

fn output_protobuf(digest: &Digest, verbose: bool) {
    if verbose {
        println!("Protobuf output (debug format):");
        println!("{:?}", digest);
    } else {
        // In a real implementation, you would serialize to binary protobuf format
        // For now, we'll just show the debug representation
        println!("{:?}", digest);
    }
}
