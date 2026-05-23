package main

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"sort"
	"strings"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

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
		verifyFlag     = fs.Bool("verify", false, "verify journal file (unsupported)")
		verifyOnlyFlag = fs.Bool("verify-only", false, "verify only (unsupported)")
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

	if *verifyFlag || *verifyOnlyFlag {
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
