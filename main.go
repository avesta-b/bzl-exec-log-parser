package main

import (
	"bufio"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/avesta-b/bzl-exec-log-parser/pkg/proto/spawn"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/encoding/protowire"
	"google.golang.org/protobuf/proto"
)
		}

		var spawnExec spawn.SpawnExec
		if err := proto.Unmarshal(content[offset:offset+int(size)], &spawnExec); err != nil {
			return nil, fmt.Errorf("failed to unmarshal protobuf at offset %d: %v", offset, err)
		}

		spawns = append(spawns, &spawnExec)
		offset += int(size)
	}org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/encoding/protowire"
	"google.golang.org/protobuf/proto"
)

// LogFormat represents the format of the execution log
type LogFormat int

const (
	LogFormatBinary LogFormat = iota
	LogFormatJSON
)

// Args represents command line arguments
type Args struct {
	File         string
	TopN         int
	CacheMetrics bool
	Format       *LogFormat
}

// MnemonicMetrics holds metrics for a specific mnemonic
type MnemonicMetrics struct {
	Count         uint64
	CacheHits     uint64
	TotalDuration time.Duration
}

func main() {
	args := parseArgs()

	// Read the file
	content, err := os.ReadFile(args.File)
	if err != nil {
		log.Fatalf("Failed to read file: %v", err)
	}

	// Determine format from flag or file extension
	format := args.Format
	if format == nil {
		detectedFormat := LogFormatBinary
		if filepath.Ext(args.File) == ".json" {
			detectedFormat = LogFormatJSON
		}
		format = &detectedFormat
	}

	// Parse the file based on format
	var spawns []*spawn.SpawnExec
	switch *format {
	case LogFormatJSON:
		spawns, err = parseJSONLog(content)
	case LogFormatBinary:
		spawns, err = parseBinaryLog(content)
	}

	if err != nil {
		log.Fatalf("Failed to parse execution log: %v", err)
	}

	if len(spawns) == 0 {
		fmt.Println("Execution log is empty or could not be parsed. No metrics to report.")
		return
	}

	// Print main report
	printMainReport(spawns, args)

	// Optionally print cache metrics report
	if args.CacheMetrics {
		printCachePerformanceReport(spawns)
	}
}

func parseArgs() *Args {
	args := &Args{}

	flag.StringVar(&args.File, "file", "", "Path to the Bazel execution log file")
	flag.IntVar(&args.TopN, "top-n", 10, "Number of slowest actions to display in the report")
	flag.BoolVar(&args.CacheMetrics, "cache-metrics", false, "Calculate and display remote cache performance metrics")

	formatStr := flag.String("format", "", "Specify the format of the log file (binary|json). Tries to auto-detect from extension if not provided.")

	flag.Parse()

	// Handle positional argument for file if not provided via flag
	if args.File == "" && flag.NArg() > 0 {
		args.File = flag.Arg(0)
	}

	if args.File == "" {
		fmt.Fprintf(os.Stderr, "Usage: %s [OPTIONS] <file>\n", os.Args[0])
		fmt.Fprintf(os.Stderr, "\nAnalyzes a Bazel execution log to extract performance metrics.\n\n")
		fmt.Fprintf(os.Stderr, "Arguments:\n")
		fmt.Fprintf(os.Stderr, "  <file>    Path to the Bazel execution log file\n\n")
		fmt.Fprintf(os.Stderr, "Options:\n")
		flag.PrintDefaults()
		os.Exit(1)
	}

	// Parse format if provided
	if *formatStr != "" {
		switch strings.ToLower(*formatStr) {
		case "binary":
			format := LogFormatBinary
			args.Format = &format
		case "json":
			format := LogFormatJSON
			args.Format = &format
		default:
			log.Fatalf("Invalid format: %s. Valid formats are: binary, json", *formatStr)
		}
	}

	return args
}

func parseJSONLog(content []byte) ([]*spawn.SpawnExec, error) {
	var spawns []*spawn.SpawnExec

	// The JSON log is a stream of JSON objects, not a single array
	// We need to parse line by line or use a JSON decoder
	decoder := json.NewDecoder(strings.NewReader(string(content)))

	for {
		var spawnExec spawn.SpawnExec
		if err := decoder.Decode(&spawnExec); err == io.EOF {
			break
		} else if err != nil {
			// Try using protojson for better protobuf-JSON compatibility
			if err := protojson.Unmarshal(content, &spawnExec); err != nil {
				return nil, fmt.Errorf("failed to parse JSON: %v", err)
			}
			spawns = append(spawns, &spawnExec)
			break
		}
		spawns = append(spawns, &spawnExec)
	}

	// If we didn't get any spawns from the streaming approach, try line-by-line
	if len(spawns) == 0 {
		scanner := bufio.NewScanner(strings.NewReader(string(content)))
		for scanner.Scan() {
			line := strings.TrimSpace(scanner.Text())
			if line == "" {
				continue
			}

			var spawnExec spawn.SpawnExec
			if err := protojson.Unmarshal([]byte(line), &spawnExec); err != nil {
				return nil, fmt.Errorf("failed to parse JSON line: %v", err)
			}
			spawns = append(spawns, &spawnExec)
		}

		if err := scanner.Err(); err != nil {
			return nil, fmt.Errorf("error reading JSON lines: %v", err)
		}
	}

	return spawns, nil
}

func parseBinaryLog(content []byte) ([]*spawn.SpawnExec, error) {
	var spawns []*spawn.SpawnExec
	offset := 0

	for offset < len(content) {
		// Parse length-delimited protobuf messages
		size, n := protowire.DecodeVarint(content[offset:]))
		if n == 0 {
			break // No more data or invalid varint
		}
		offset += n

		if offset+int(size) > len(content) {
			break // Not enough data for the message
		}

		var spawnExec spawn.SpawnExec
		if err := proto.Unmarshal(content[offset:offset+int(size)], &spawnExec); err != nil {
			return nil, fmt.Errorf("failed to unmarshal protobuf at offset %d: %v", offset, err)
		}

		spawns = append(spawns, &spawnExec)
		offset += int(size)
	}

	return spawns, nil
}

func toDuration(protoDuration *spawn.SpawnMetrics) time.Duration {
	if protoDuration == nil || protoDuration.TotalTime == nil {
		return 0
	}
	return time.Duration(protoDuration.TotalTime.Seconds)*time.Second +
		time.Duration(protoDuration.TotalTime.Nanos)*time.Nanosecond
}

func printMainReport(spawns []*spawn.SpawnExec, args *Args) {
	totalActions := len(spawns)
	cacheHits := 0
	for _, s := range spawns {
		if s.CacheHit {
			cacheHits++
		}
	}

	// Sort by duration (slowest first)
	slowestActions := make([]*spawn.SpawnExec, len(spawns))
	copy(slowestActions, spawns)
	sort.Slice(slowestActions, func(i, j int) bool {
		durI := toDuration(slowestActions[i].Metrics)
		durJ := toDuration(slowestActions[j].Metrics)
		return durI > durJ
	})

	// Collect metrics by mnemonic
	mnemonicMetrics := make(map[string]*MnemonicMetrics)
	for _, spawn := range spawns {
		metrics, exists := mnemonicMetrics[spawn.Mnemonic]
		if !exists {
			metrics = &MnemonicMetrics{}
			mnemonicMetrics[spawn.Mnemonic] = metrics
		}
		metrics.Count++
		if spawn.CacheHit {
			metrics.CacheHits++
		}
		metrics.TotalDuration += toDuration(spawn.Metrics)
	}

	// Print the report
	fmt.Println("========================================")
	fmt.Println(" Bazel Execution Log Analysis Report")
	fmt.Println("========================================")
	fmt.Printf("Log file: %s\n\n", args.File)

	fmt.Println("--- Overall Summary ---")
	fmt.Printf("Total Actions: %d\n", totalActions)
	fmt.Printf("Cache Hits: %d (%.2f%%)\n", cacheHits, float64(cacheHits)/float64(totalActions)*100.0)
	fmt.Println()

	fmt.Printf("--- Top %d Slowest Actions ---\n", args.TopN)
	fmt.Printf("%-10s | %-25s | %s\n", "Time", "Mnemonic", "Target")
	fmt.Println("---------------------------------------------------------------------------------")
	for i, spawn := range slowestActions {
		if i >= args.TopN {
			break
		}
		duration := toDuration(spawn.Metrics)
		fmt.Printf("%-10.3fs | %-25s | %s\n",
			duration.Seconds(),
			spawn.Mnemonic,
			spawn.TargetLabel)
	}
	fmt.Println()

	fmt.Println("--- Analysis by Mnemonic ---")
	fmt.Printf("%-25s | %10s | %10s | %10s | %10s\n", "Mnemonic", "Count", "Cache Hits", "Total Time", "Avg Time")
	fmt.Println("---------------------------------------------------------------------------------")

	// Sort mnemonics by total duration
	type mnemonicPair struct {
		name    string
		metrics *MnemonicMetrics
	}
	var sortedMnemonics []mnemonicPair
	for name, metrics := range mnemonicMetrics {
		sortedMnemonics = append(sortedMnemonics, mnemonicPair{name, metrics})
	}
	sort.Slice(sortedMnemonics, func(i, j int) bool {
		return sortedMnemonics[i].metrics.TotalDuration > sortedMnemonics[j].metrics.TotalDuration
	})

	for _, pair := range sortedMnemonics {
		metrics := pair.metrics
		avgTime := 0.0
		if metrics.Count > 0 {
			avgTime = metrics.TotalDuration.Seconds() / float64(metrics.Count)
		}

		fmt.Printf("%-25s | %10d | %9.1f%% | %9.2fs | %9.3fs\n",
			pair.name,
			metrics.Count,
			float64(metrics.CacheHits)/float64(metrics.Count)*100.0,
			metrics.TotalDuration.Seconds(),
			avgTime)
	}
	fmt.Println()
}

func printCachePerformanceReport(spawns []*spawn.SpawnExec) {
	var totalBytesDownloaded int64
	var totalFetchTime time.Duration
	var remoteCacheHitCount int

	for _, spawn := range spawns {
		if spawn.Runner == "remote cache hit" {
			remoteCacheHitCount++

			// Sum the size of all output files for this spawn
			for _, file := range spawn.ActualOutputs {
				if file.Digest != nil {
					totalBytesDownloaded += file.Digest.SizeBytes
				}
			}

			// Add the time spent fetching remote outputs
			if spawn.Metrics != nil && spawn.Metrics.FetchTime != nil {
				fetchDuration := time.Duration(spawn.Metrics.FetchTime.Seconds)*time.Second +
					time.Duration(spawn.Metrics.FetchTime.Nanos)*time.Nanosecond
				totalFetchTime += fetchDuration
			}
		}
	}

	fmt.Println("--- Remote Cache Performance ---")

	if remoteCacheHitCount == 0 {
		fmt.Println("No remote cache hits found in the log.")
		fmt.Println()
		return
	}

	totalMBDownloaded := float64(totalBytesDownloaded) / 1_000_000.0
	totalFetchSeconds := totalFetchTime.Seconds()

	fmt.Printf("Remote Cache Hits Count: %d\n", remoteCacheHitCount)
	fmt.Printf("Total Data Downloaded: %.2f MB\n", totalMBDownloaded)
	fmt.Printf("Total Time Fetching from Cache: %.2fs\n", totalFetchSeconds)

	if totalFetchSeconds > 0.001 {
		downloadRateMBPS := totalMBDownloaded / totalFetchSeconds
		fmt.Printf("Average Download Rate: %.2f MB/s\n", downloadRateMBPS)
	} else {
		fmt.Println("Average Download Rate: N/A (total fetch time is negligible)")
	}
	fmt.Println()
}
