#!/bin/bash

# Script to combine git-tracked .go, .proto, and .html files into one file
# Each file will be prefixed with a comment showing its path

set -euo pipefail

# Output file
OUTPUT_FILE="combined_code.txt"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

# Check if we're in a git repository
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    echo "Error: Not in a git repository"
    exit 1
fi

print_status "Starting file combination..."

# Remove existing output file
rm -f "$OUTPUT_FILE"

# Create header
cat << 'EOF' > "$OUTPUT_FILE"
# Combined Code Files
# Generated on $(date)
# Repository: $(git remote get-url origin 2>/dev/null || echo "Local repository")
# Branch: $(git branch --show-current)
# Commit: $(git rev-parse HEAD)

EOF

# Counter for files processed
file_count=0

# Find all git-tracked files ending in .go, .proto, or .html
git ls-files | grep -E '\.(go|proto|html|rs)$' | sort | while read -r file; do
    if [[ -f "$file" ]]; then
        print_status "Processing: $file"
        
        # Determine comment style based on file extension
        if [[ "$file" =~ \.(go|proto)$ ]]; then
            comment_prefix="//"
        elif [[ "$file" =~ \.html$ ]]; then
            comment_prefix="<!--"
            comment_suffix=" -->"
        fi
        
        # Add file separator and path comment
        echo "" >> "$OUTPUT_FILE"
        echo "=================================================================================" >> "$OUTPUT_FILE"
        if [[ "$file" =~ \.html$ ]]; then
            echo "<!-- $file -->" >> "$OUTPUT_FILE"
        else
            echo "// $file" >> "$OUTPUT_FILE"
        fi
        echo "=================================================================================" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
        
        # Add file contents
        cat "$file" >> "$OUTPUT_FILE"
        
        # Add some spacing after the file
        echo "" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
        
        ((file_count++))
    else
        print_warning "File not found or not readable: $file"
    fi
done

print_status "Combination complete!"
print_status "Combined $file_count files into $OUTPUT_FILE"
print_status "Output file size: $(wc -l < "$OUTPUT_FILE") lines"

# Show some stats
echo ""
echo "File type breakdown:"
echo "  Go files:    $(git ls-files | grep -E '\.go$' | wc -l)"
echo "  Proto files: $(git ls-files | grep -E '\.proto$' | wc -l)"
echo "  HTML files:  $(git ls-files | grep -E '\.html$' | wc -l)"
echo "  Total:       $(git ls-files | grep -E '\.(go|proto|html)$' | wc -l)"
