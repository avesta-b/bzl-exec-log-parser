#!/bin/bash

# Script to generate Go language bindings for protobuf files with JSON support
# Uses protoc-gen-go for standard Go bindings and protoc-gen-go-json for JSON marshaling

set -euo pipefail

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
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if protoc is installed
if ! command -v protoc &> /dev/null; then
    print_error "protoc is not installed. Please install Protocol Buffers compiler."
    print_error "On macOS: brew install protobuf"
    exit 1
fi

# Check if Go is installed
if ! command -v go &> /dev/null; then
    print_error "Go is not installed. Please install Go."
    exit 1
fi

# Check if Go environment is properly set up
if ! go env GOROOT &> /dev/null; then
    print_error "Go environment is not properly configured."
    print_error "Try reinstalling Go or fixing your Go installation."
    exit 1
fi

print_status "Checking and installing required Go plugins..."

# Install protoc-gen-go if not present
if ! command -v protoc-gen-go &> /dev/null; then
    print_status "Installing protoc-gen-go..."
    go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
else
    print_status "protoc-gen-go is already installed"
fi

# Install protoc-gen-go-json if not present
if ! command -v protoc-gen-go-json &> /dev/null; then
    print_status "Installing protoc-gen-go-json..."
    go install github.com/mitchellh/protoc-gen-go-json@latest
else
    print_status "protoc-gen-go-json is already installed"
fi

# Ensure GOPATH/bin is in PATH
if [[ ":$PATH:" != *":$(go env GOPATH)/bin:"* ]]; then
    export PATH="$PATH:$(go env GOPATH)/bin"
    print_warning "Added $(go env GOPATH)/bin to PATH for this session"
fi

# Create output directory for generated Go files
OUTPUT_DIR="pkg/proto"
mkdir -p "$OUTPUT_DIR"

print_status "Generating Go bindings for protobuf files..."

# Array of proto files with their subdirectories
PROTO_CONFIGS=(
    "spawn:spawn/spawn.proto"
)

# Create base output directory for generated Go files
BASE_OUTPUT_DIR="pkg/proto"
rm -rf "$BASE_OUTPUT_DIR"
mkdir -p "$BASE_OUTPUT_DIR"

print_status "Generating Go bindings for protobuf files..."

# Generate Go bindings for each proto file in its own package
for config in "${PROTO_CONFIGS[@]}"; do
    package_name="${config%%:*}"
    proto_path="${config##*:}"
    
    if [[ -f "protos/$proto_path" ]]; then
        print_status "Processing $proto_path into package $package_name..."
        
        # Create output directory
        mkdir -p "$BASE_OUTPUT_DIR/$package_name"
        
        # Generate bindings for this specific proto file
        protoc \
            --proto_path=protos \
            --go_out="$BASE_OUTPUT_DIR/$package_name" \
            --go_opt=paths=source_relative \
            --go-json_out="$BASE_OUTPUT_DIR/$package_name" \
            --go-json_opt=paths=source_relative \
            "protos/$proto_path"
        
        # Move files from nested directory to package root if they exist
        if [[ -d "$BASE_OUTPUT_DIR/$package_name/${proto_path%/*}" ]]; then
            mv "$BASE_OUTPUT_DIR/$package_name/${proto_path%/*}/"* "$BASE_OUTPUT_DIR/$package_name/"
            rmdir "$BASE_OUTPUT_DIR/$package_name/${proto_path%/*}"
        fi
        
        if [[ $? -eq 0 ]]; then
            print_status "✓ Successfully generated bindings for $package_name"
        else
            print_warning "⚠ Failed to generate bindings for $package_name"
        fi
    else
        print_warning "Proto file not found: protos/$proto_path"
    fi
done

print_status "Code generation completed successfully!"
print_status "Generated files are located in: $BASE_OUTPUT_DIR"

# Display generated files
print_status "Generated files:"
find "$BASE_OUTPUT_DIR" -name "*.go" | while read -r file; do
    echo "  - $file"
done

print_status "To use these bindings in your Go code, import them as:"
echo "  import \"github.com/avesta-b/bzl-exec-log-parser/pkg/proto/<package_name>\""
echo ""
echo "Available packages:"
echo "  - github.com/avesta-b/bzl-exec-log-parser/pkg/proto/spawn"
