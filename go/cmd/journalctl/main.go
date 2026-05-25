package main

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

// HEADER_COMPATIBLE_SEALED from systemd journal-def.h
const compatibleSealed = 1

func main() {
	if err := run(os.Args[1:], os.Stdin, os.Stdout, os.Stderr); err != nil {
		if errors.Is(err, journal.ErrUnsupported) {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}

func run(args []string, stdin io.Reader, stdout, stderr io.Writer) error {
	fs := flag.NewFlagSet("journalctl", flag.ContinueOnError)
	fs.SetOutput(stderr)

	var (
		fileFlag       = fs.String("file", "", "journal file")
		directoryFlag  = fs.String("directory", "", "journal directory")
		outputFlag     = fs.String("output", "default", "output mode: default, json, export")
		listBootsFlag  = fs.Bool("list-boots", false, "list boots")
		_              = fs.Bool("no-tail", false, "show all entries, start from the beginning")
		followFlag     = fs.Bool("follow", false, "follow (unsupported in pure-Go)")
		_              = fs.String("boot", "", "boot filter (unsupported)")
		fieldsFlag     = fs.Bool("fields", false, "show field names")
		headFlag       = fs.Int("head", 0, "show first N entries")
		tailFlag       = fs.Int("tail", 0, "show last N entries")
		_              = fs.String("since", "", "show entries since timestamp (unsupported)")
		_              = fs.String("until", "", "show entries until timestamp (unsupported)")
		syncFlag       = fs.Bool("sync", false, "sync journal (unsupported)")
		flushFlag      = fs.Bool("flush", false, "flush journal (unsupported)")
		rotateFlag     = fs.Bool("rotate", false, "rotate journal (unsupported)")
		relinquishFlag = fs.Bool("relinquish-var", false, "relinquish var (unsupported)")
		verifyFlag     = fs.Bool("verify", false, "verify journal file")
		verifyOnlyFlag = fs.Bool("verify-only", false, "verify only")
		verifyKeyFlag  = fs.String("verify-key", "", "FSS verification key")
	)

	fs.Usage = func() {
		fmt.Fprintf(stderr, "Usage: %s [options]\n", fs.Name())
		fmt.Fprintf(stderr, "Pure-Go systemd journal reader\n")
		fmt.Fprintf(stderr, "\nOptions:\n")
		fs.PrintDefaults()
	}

	if err := fs.Parse(args); err != nil {
		if errors.Is(err, flag.ErrHelp) {
			return nil
		}
		return err
	}

	if *syncFlag || *flushFlag || *rotateFlag || *relinquishFlag {
		return journal.ErrUnsupported
	}

	if *followFlag {
		return journal.ErrUnsupported
	}

	inputPath := *fileFlag
	if inputPath == "" && *directoryFlag != "" {
		inputPath = *directoryFlag
	}

	if inputPath == "" {
		return errors.New("no journal file or directory specified (use --file or --directory)")
	}

	hasVerifyKey := hasStringFlag(args, "verify-key")
	if *verifyFlag || *verifyOnlyFlag || hasVerifyKey {
		return runVerify(inputPath, *verifyKeyFlag, hasVerifyKey, stdout, stderr)
	}

	j, err := journal.SdJournalOpen(inputPath, 0)
	if err != nil {
		return fmt.Errorf("open journal: %w", err)
	}
	defer j.Close()

	for _, arg := range fs.Args() {
		if arg == "+" {
			j.AddDisjunction()
			continue
		}
		if strings.Contains(arg, "=") {
			match, err := journal.ParseMatchString(arg)
			if err != nil {
				return err
			}
			j.AddMatch(match)
		}
	}

	j.SetOutputMode(*outputFlag)

	switch {
	case *listBootsFlag:
		boots, err := journal.SdJournalListBoots(j)
		if err != nil {
			return fmt.Errorf("list boots: %w", err)
		}
		for _, b := range boots {
			first := time.UnixMicro(b.FirstEntry)
			last := time.UnixMicro(b.LastEntry)
			fmt.Fprintf(stdout, "[%4d] %s %s - %s\n",
				b.Index, b.BootID[:8],
				first.Format(time.DateTime),
				last.Format(time.DateTime))
		}
		return nil

	case *fieldsFlag:
		fields, err := journal.SdJournalEnumerateFields(j)
		if err != nil {
			return fmt.Errorf("enumerate fields: %w", err)
		}
		sort.Strings(fields)
		for _, f := range fields {
			fmt.Fprintln(stdout, f)
		}
		return nil

	case *headFlag > 0:
		if err := j.SeekHead(); err != nil {
			return err
		}
		count := 0
		for {
			ok, err := j.Next()
			if err != nil {
				return err
			}
			if ok == 0 {
				break
			}
			entry, err := journal.SdJournalGetEntry(j)
			if err != nil {
				return err
			}
			out, err := j.ProcessOutput(entry)
			if err != nil {
				return err
			}
			fmt.Fprint(stdout, out)
			count++
			if count >= *headFlag {
				break
			}
		}
		return nil

	case *tailFlag > 0:
		if err := j.SeekTail(); err != nil {
			return err
		}
		entries := make([]string, 0, *tailFlag)
		for len(entries) < *tailFlag {
			ok, err := j.Previous()
			if err != nil {
				if errors.Is(err, journal.ErrStartOfEntries) {
					break
				}
				return err
			}
			if ok == 0 {
				break
			}
			entry, err := journal.SdJournalGetEntry(j)
			if err != nil {
				return err
			}
			out, err := j.ProcessOutput(entry)
			if err != nil {
				return err
			}
			entries = append(entries, out)
		}
		for i := len(entries) - 1; i >= 0; i-- {
			fmt.Fprint(stdout, entries[i])
		}
		return nil

	default:
		if err := j.SeekHead(); err != nil {
			return err
		}

		for {
			ok, err := j.Next()
			if err != nil {
				if errors.Is(err, journal.ErrEndOfEntries) {
					break
				}
				return err
			}
			if ok == 0 {
				break
			}
			entry, err := journal.SdJournalGetEntry(j)
			if err != nil {
				return err
			}
			out, err := j.ProcessOutput(entry)
			if err != nil {
				return err
			}
			fmt.Fprint(stdout, out)
		}
		return nil
	}
}

func runVerify(inputPath, verifyKey string, hasVerifyKey bool, stdout, stderr io.Writer) error {
	if hasVerifyKey && !validVerificationKey(verifyKey) {
		fmt.Fprintln(stderr, "Failed to parse seed.")
		return errors.New("failed to parse seed")
	}

	info, err := os.Stat(inputPath)
	if err != nil {
		return fmt.Errorf("verify: %w", err)
	}

	var files []string
	if info.IsDir() {
		entries, err := os.ReadDir(inputPath)
		if err != nil {
			return fmt.Errorf("verify: read directory: %w", err)
		}
		for _, entry := range entries {
			name := entry.Name()
			candidate := filepath.Join(inputPath, name)
			info, err := os.Stat(candidate)
			if err != nil || !info.Mode().IsRegular() {
				continue
			}
			if isJournalFileName(name) {
				files = append(files, candidate)
			}
		}
		sort.Strings(files)
	} else {
		files = append(files, inputPath)
	}

	if len(files) == 0 {
		return errors.New("verify: no journal files found")
	}

	var firstErr error
	for _, path := range files {
		sealed, err := isFileSealed(path)
		if err != nil {
			fmt.Fprintf(stderr, "FAIL: %s (%v)\n", path, err)
			if firstErr == nil {
				firstErr = err
			}
			continue
		}

		if sealed && !hasVerifyKey {
			fmt.Fprintf(stderr, "Journal file %s has sealing enabled but verification key has not been passed using --verify-key=.\n", path)
			fmt.Fprintf(stderr, "FAIL: %s (verification key required for sealed journal file)\n", path)
			if firstErr == nil {
				firstErr = errors.New("verification key required for sealed journal file")
			}
			continue
		}

		if sealed && hasVerifyKey {
			if err := journal.VerifyFileWithKey(path, verifyKey); err != nil {
				fmt.Fprintf(stderr, "FAIL: %s (%v)\n", path, err)
				if firstErr == nil {
					firstErr = err
				}
				continue
			}
			fmt.Fprintf(stderr, "PASS: %s\n", path)
			continue
		}

		if err := journal.VerifyFile(path); err != nil {
			fmt.Fprintf(stderr, "FAIL: %s (%v)\n", path, err)
			if firstErr == nil {
				firstErr = err
			}
			continue
		}
		fmt.Fprintf(stderr, "PASS: %s\n", path)
	}

	return firstErr
}

func isFileSealed(path string) (bool, error) {
	r, err := journal.OpenFile(path)
	if err != nil {
		return false, err
	}
	defer r.Close()
	return r.Header().CompatibleFlags()&compatibleSealed != 0, nil
}

func isJournalFileName(name string) bool {
	return strings.HasSuffix(name, ".journal") ||
		strings.HasSuffix(name, ".journal~") ||
		strings.HasSuffix(name, ".journal.zst") ||
		strings.HasSuffix(name, ".journal~.zst")
}

func hasStringFlag(args []string, name string) bool {
	long := "--" + name
	single := "-" + name
	withEquals := long + "="
	singleWithEquals := single + "="
	for _, arg := range args {
		if arg == long || arg == single || strings.HasPrefix(arg, withEquals) || strings.HasPrefix(arg, singleWithEquals) {
			return true
		}
		if arg == "--" {
			return false
		}
		if strings.HasPrefix(arg, "-") && arg != "-" {
			continue
		}
	}
	return false
}

func validVerificationKey(key string) bool {
	i := 0
	for c := 0; c < 12; c++ {
		for i < len(key) && key[i] == '-' {
			i++
		}
		if i+2 > len(key) || !isHex(key[i]) || !isHex(key[i+1]) {
			return false
		}
		i += 2
	}
	if i >= len(key) || key[i] != '/' {
		return false
	}
	i++
	next, ok := consumeHex(key, i)
	if !ok || next >= len(key) || key[next] != '-' {
		return false
	}
	end, ok := consumeHex(key, next+1)
	if !ok || end != len(key) {
		return false
	}
	for _, b := range key[next+1 : end] {
		if b != '0' {
			return true
		}
	}
	return false
}

func consumeHex(s string, start int) (int, bool) {
	i := start
	for i < len(s) && isHex(s[i]) {
		i++
	}
	return i, i > start
}

func isHex(b byte) bool {
	return ('0' <= b && b <= '9') || ('a' <= b && b <= 'f') || ('A' <= b && b <= 'F')
}
