#!/bin/bash

# Bash script to compile Rust bindings for spawn.proto
# This script sets up protobuf compilation for Rust

set -e  # Exit on any error

echo "🚀 Setting up Rust protobuf bindings for spawn.proto"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if protoc is installed
check_protoc() {
    if ! command -v protoc &> /dev/null; then
        print_error "protoc (Protocol Buffer Compiler) is not installed"
        echo "Please install it using one of these methods:"
        echo "  macOS: brew install protobuf"
        echo "  Ubuntu/Debian: sudo apt-get install protobuf-compiler"
        echo "  Other: https://grpc.io/docs/protoc-installation/"
        exit 1
    else
        print_status "protoc found: $(protoc --version)"
    fi
}

# Update Cargo.toml with required dependencies
update_cargo_toml() {
    print_status "Updating Cargo.toml with protobuf dependencies..."
    
    # Backup original Cargo.toml
    cp Cargo.toml Cargo.toml.backup
    
    # Add dependencies to Cargo.toml
    cat >> Cargo.toml << 'EOF'

# Protobuf dependencies
prost = "0.12"
prost-types = "0.12"

[build-dependencies]
prost-build = "0.12"
EOF
    
    print_status "Dependencies added to Cargo.toml"
}

# Create build script
create_build_script() {
    print_status "Creating build.rs script..."
    
    cat > build.rs << 'EOF'
use std::io::Result;

fn main() -> Result<()> {
    let mut config = prost_build::Config::new();
    
    // Configure the output directory
    config.out_dir("src/proto");
    
    // Compile the proto file
    config.compile_protos(&["spawn.proto"], &["."])?;
    
    println!("cargo:rerun-if-changed=spawn.proto");
    
    Ok(())
}
EOF
    
    print_status "build.rs created"
}

# Create proto module directory
create_proto_module() {
    print_status "Creating proto module structure..."
    
    mkdir -p src/proto
    
    # Create mod.rs for the proto module
    cat > src/proto/mod.rs << 'EOF'
// Auto-generated protobuf bindings
// This module contains the generated Rust structs from spawn.proto

pub mod tools {
    pub mod protos {
        include!(concat!(env!("OUT_DIR"), "/tools.protos.rs"));
    }
}

// Re-export commonly used types for convenience
pub use tools::protos::*;
EOF
    
    print_status "Proto module structure created"
}

# Update main.rs to include the proto module
update_main_rs() {
    print_status "Updating main.rs to include proto module..."
    
    # Backup original main.rs
    cp src/main.rs src/main.rs.backup
    
    cat > src/main.rs << 'EOF'
mod proto;

use proto::*;

fn main() {
    println!("Hello, world!");
    println!("Protobuf bindings are ready to use!");
    
    // Example: Create a new Digest message
    let digest = Digest {
        hash: "abc123".to_string(),
        size_bytes: 1024,
        hash_function_name: "SHA256".to_string(),
    };
    
    println!("Created digest: {:?}", digest);
}
EOF
    
    print_status "main.rs updated"
}

# Build the project
build_project() {
    print_status "Building the project..."
    
    if cargo build; then
        print_status "✅ Build successful! Rust protobuf bindings are ready."
    else
        print_error "❌ Build failed. Please check the error messages above."
        exit 1
    fi
}

# Clean up function
cleanup_on_error() {
    print_warning "Cleaning up due to error..."
    if [ -f Cargo.toml.backup ]; then
        mv Cargo.toml.backup Cargo.toml
        print_status "Restored original Cargo.toml"
    fi
    if [ -f src/main.rs.backup ]; then
        mv src/main.rs.backup src/main.rs
        print_status "Restored original main.rs"
    fi
}

# Set up error handling
trap cleanup_on_error ERR

# Main execution
main() {
    print_status "Starting protobuf compilation process..."
    
    # Check prerequisites
    check_protoc
    
    # Perform setup steps
    update_cargo_toml
    create_build_script
    create_proto_module
    update_main_rs
    
    # Build the project
    build_project
    
    # Clean up backup files on success
    rm -f Cargo.toml.backup src/main.rs.backup
    
    print_status "🎉 Complete! Your Rust protobuf bindings are ready."
    echo ""
    echo "Next steps:"
    echo "  1. Run: cargo run"
    echo "  2. Import types in your code: use crate::proto::*;"
    echo "  3. Use the generated structs like Digest, File, SpawnExec, etc."
    echo ""
    echo "Available message types from spawn.proto:"
    echo "  - Digest"
    echo "  - File"
    echo "  - SpawnExec"
    echo "  - ExecLogEntry"
    echo "  - And more..."
}

# Run the main function
main
