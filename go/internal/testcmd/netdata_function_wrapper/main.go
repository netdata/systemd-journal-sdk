package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"sync/atomic"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

type args struct {
	functionName        string
	directory           string
	timeoutSeconds      uint64
	progressJSONL       string
	cancelImmediately   bool
	cancelAfterProgress uint64
}

type progressRecorder struct {
	file                *os.File
	cancelled           atomic.Bool
	reports             atomic.Uint64
	cancelAfterProgress uint64
}

func main() {
	args := parseArgs()
	if err := validateFunctionName(args.functionName); err != nil {
		exitError(err)
	}
	request, err := io.ReadAll(os.Stdin)
	if err != nil {
		exitError(fmt.Errorf("failed to read request JSON from stdin: %w", err))
	}
	recorder, err := newProgressRecorder(args)
	if err != nil {
		exitError(err)
	}
	defer recorder.close()

	options := journal.NetdataFunctionRunOptionsFromTimeoutSeconds(args.timeoutSeconds)
	if args.progressJSONL != "" || args.cancelAfterProgress != 0 {
		options.ProgressCallback = recorder.handle
	}
	if args.cancelImmediately {
		recorder.cancelled.Store(true)
	}
	if args.cancelImmediately || args.cancelAfterProgress != 0 {
		options.CancellationCallback = recorder.isCancelled
	}

	response, err := journal.SystemdJournalPluginCompatibleNetdataFunction().
		RunDirectoryRequestBytesWithOptions(args.directory, request, options)
	if err != nil {
		exitError(fmt.Errorf("failed to run function %q for %s: %w", args.functionName, args.directory, err))
	}
	encoder := json.NewEncoder(os.Stdout)
	if err := encoder.Encode(response); err != nil {
		exitError(fmt.Errorf("failed to write response JSON: %w", err))
	}
}

func parseArgs() args {
	var args args
	flag.StringVar(&args.functionName, "test", "", "Netdata function name to run")
	flag.StringVar(&args.directory, "dir", "", "journal directory")
	flag.Uint64Var(&args.timeoutSeconds, "timeout", 0, "timeout in seconds; 0 disables timeout")
	flag.StringVar(&args.progressJSONL, "progress-jsonl", "", "optional progress JSONL output path")
	flag.BoolVar(&args.cancelImmediately, "cancel-immediately", false, "cancel before scanning starts")
	flag.Uint64Var(&args.cancelAfterProgress, "cancel-after-progress", 0, "cancel after N progress callbacks")
	flag.Parse()
	if args.functionName == "" || args.directory == "" {
		exitError(fmt.Errorf("--test and --dir are required"))
	}
	return args
}

func validateFunctionName(functionName string) error {
	if functionName != "systemd-journal" {
		return fmt.Errorf("unsupported function %q", functionName)
	}
	return nil
}

func newProgressRecorder(args args) (*progressRecorder, error) {
	recorder := &progressRecorder{cancelAfterProgress: args.cancelAfterProgress}
	if args.progressJSONL != "" {
		file, err := os.Create(args.progressJSONL)
		if err != nil {
			return nil, fmt.Errorf("failed to create progress log %s: %w", args.progressJSONL, err)
		}
		recorder.file = file
	}
	return recorder, nil
}

func (r *progressRecorder) handle(progress journal.NetdataFunctionProgress) {
	reports := r.reports.Add(1)
	if r.file != nil {
		line := map[string]any{
			"current_file":    progress.CurrentFile,
			"total_files":     progress.TotalFiles,
			"matched_files":   progress.MatchedFiles,
			"skipped_files":   progress.SkippedFiles,
			"elapsed_seconds": progress.Elapsed.Seconds(),
			"stats":           progress.Stats,
		}
		encoder := json.NewEncoder(r.file)
		_ = encoder.Encode(line)
	}
	if r.cancelAfterProgress != 0 && reports >= r.cancelAfterProgress {
		r.cancelled.Store(true)
	}
}

func (r *progressRecorder) isCancelled() bool {
	return r.cancelled.Load()
}

func (r *progressRecorder) close() {
	if r.file != nil {
		_ = r.file.Close()
	}
}

func exitError(err error) {
	fmt.Fprintf(os.Stderr, "%v\n", err)
	os.Exit(1)
}
