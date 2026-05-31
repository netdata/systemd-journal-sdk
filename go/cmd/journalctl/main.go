package main

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

// HEADER_COMPATIBLE_SEALED from systemd journal-def.h
const compatibleSealed = 1

var (
	errStopIteration = errors.New("stop iteration")

	signedDatePrefixRe = regexp.MustCompile(`^[+-]\d{4}-`)
	epochTimestampRe   = regexp.MustCompile(`^\d+(\.\d+)?$`)
	durationTokenRe    = regexp.MustCompile(`\s*(\d+(?:\.\d+)?)(?:\s*([A-Za-z]+))?`)

	bootDescriptorRe       = regexp.MustCompile(`^(([0-9A-Fa-f]{32})|([0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}))?([+-]?\d+)?$`)
	bootDescriptorPatterns = []*regexp.Regexp{
		regexp.MustCompile(`^[+-]?\d+$`),
		regexp.MustCompile(`^[0-9A-Fa-f]{32}([+-]\d+)?$`),
		regexp.MustCompile(`^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}([+-]\d+)?$`),
	}
)

type cliJournal interface {
	Close() error
	AddMatch([]byte)
	AddDisjunction()
	AddConjunction()
	SeekHead() error
	SeekRealtimeUsec(uint64) error
	Next() (int, error)
	GetEntry() (*journal.Entry, error)
	SetOutputMode(string)
	ProcessOutput(*journal.Entry) (string, error)
	ListBoots() ([]journal.BootInfo, error)
	EnumerateFields() ([]string, error)
	VisitUnique(string, func([]byte) error) error
}

type optionalStringFlag struct {
	set   bool
	value string
}

func (f *optionalStringFlag) Set(value string) error {
	f.set = true
	if value == "true" {
		value = ""
	}
	f.value = value
	return nil
}

func (f *optionalStringFlag) String() string {
	return f.value
}

func (f *optionalStringFlag) IsBoolFlag() bool {
	return true
}

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
		noTailFlag     = fs.Bool("no-tail", false, "show all entries, start from the beginning")
		followFlag     = fs.Bool("follow", false, "follow appended entries")
		bootFlag       optionalStringFlag
		fieldsFlag     = fs.Bool("fields", false, "show field names")
		fieldFlag      = fs.String("field", "", "show values for a field")
		headFlag       = fs.Int("head", 0, "show first N entries")
		tailFlag       = fs.Int("tail", 0, "show last N entries")
		sinceFlag      = fs.String("since", "", "show entries since timestamp")
		untilFlag      = fs.String("until", "", "show entries until timestamp")
		syncFlag       = fs.Bool("sync", false, "sync journal (unsupported)")
		flushFlag      = fs.Bool("flush", false, "flush journal (unsupported)")
		rotateFlag     = fs.Bool("rotate", false, "rotate journal (unsupported)")
		relinquishFlag = fs.Bool("relinquish-var", false, "relinquish var (unsupported)")
		verifyFlag     = fs.Bool("verify", false, "verify journal file")
		verifyOnlyFlag = fs.Bool("verify-only", false, "verify only")
		verifyKeyFlag  = fs.String("verify-key", "", "FSS verification key")
	)
	fs.Var(&bootFlag, "boot", "boot filter")
	fs.Var(&bootFlag, "b", "boot filter")
	fs.StringVar(fieldFlag, "F", "", "show values for a field")
	fs.StringVar(sinceFlag, "S", "", "show entries since timestamp")
	fs.StringVar(untilFlag, "U", "", "show entries until timestamp")

	fs.Usage = func() {
		fmt.Fprintf(stderr, "Usage: %s [options]\n", fs.Name())
		fmt.Fprintf(stderr, "Pure-Go systemd journal reader\n")
		fmt.Fprintf(stderr, "\nOptions:\n")
		fs.PrintDefaults()
	}

	if err := fs.Parse(preprocessOptionalBootArgs(args)); err != nil {
		if errors.Is(err, flag.ErrHelp) {
			return nil
		}
		return err
	}

	if *syncFlag || *flushFlag || *rotateFlag || *relinquishFlag {
		return journal.ErrUnsupported
	}
	if *headFlag < 0 {
		return errors.New("--head must be a non-negative integer")
	}
	if *tailFlag < 0 {
		return errors.New("--tail must be a non-negative integer")
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

	sinceUsec, err := parseOptionalTimestampUsec(*sinceFlag)
	if err != nil {
		return err
	}
	untilUsec, err := parseOptionalTimestampUsec(*untilFlag)
	if err != nil {
		return err
	}
	if sinceUsec != nil && untilUsec != nil && *sinceUsec > *untilUsec {
		return errors.New("--since= must be before --until=.")
	}

	if *followFlag {
		tail := 10
		if flagWasSet(fs, "tail") {
			tail = *tailFlag
		}
		return runFollow(inputPath, fs.Args(), bootFlag, *outputFlag, sinceUsec, untilUsec, tail, *noTailFlag, stdout)
	}

	j, err := openFilteredJournal(inputPath, fs.Args(), bootFlag, *outputFlag)
	if err != nil {
		return err
	}
	defer j.Close()

	switch {
	case *listBootsFlag:
		boots, err := j.ListBoots()
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
		fields, err := j.EnumerateFields()
		if err != nil {
			return fmt.Errorf("enumerate fields: %w", err)
		}
		sort.Strings(fields)
		for _, f := range fields {
			fmt.Fprintln(stdout, f)
		}
		return nil

	case *fieldFlag != "":
		return j.VisitUnique(*fieldFlag, func(value []byte) error {
			if _, err := stdout.Write(value); err != nil {
				return err
			}
			if _, err := fmt.Fprintln(stdout); err != nil {
				return err
			}
			return nil
		})

	case *headFlag > 0:
		return showForward(j, *headFlag, sinceUsec, untilUsec, stdout)

	case *tailFlag > 0:
		return showTail(j, *tailFlag, sinceUsec, untilUsec, stdout)

	default:
		return showForward(j, *headFlag, sinceUsec, untilUsec, stdout)
	}
}

func openFilteredJournal(inputPath string, matches []string, boot optionalStringFlag, outputMode string) (cliJournal, error) {
	j, err := journal.SdJournalOpen(inputPath, 0)
	if err != nil {
		return nil, fmt.Errorf("open journal: %w", err)
	}
	ok := false
	defer func() {
		if !ok {
			_ = j.Close()
		}
	}()

	if boot.set && strings.TrimSpace(boot.value) != "all" {
		bootID, err := resolveBootID(j, strings.TrimSpace(boot.value))
		if err != nil {
			return nil, err
		}
		if bootID != "" {
			match, err := journal.ParseMatchString("_BOOT_ID=" + bootID)
			if err != nil {
				return nil, err
			}
			j.AddMatch(match)
			j.AddConjunction()
		}
	}

	for _, arg := range matches {
		if arg == "+" {
			j.AddDisjunction()
			continue
		}
		if strings.Contains(arg, "=") {
			match, err := journal.ParseMatchString(arg)
			if err != nil {
				return nil, err
			}
			j.AddMatch(match)
		}
	}

	j.SetOutputMode(outputMode)
	ok = true
	return j, nil
}

func preprocessOptionalBootArgs(args []string) []string {
	out := make([]string, 0, len(args))
	for i := 0; i < len(args); i++ {
		arg := args[i]
		if arg == "--boot" || arg == "-b" {
			if i+1 < len(args) && looksLikeBootDescriptor(args[i+1]) {
				out = append(out, arg+"="+args[i+1])
				i++
			} else {
				out = append(out, arg+"=")
			}
			continue
		}
		out = append(out, arg)
	}
	return out
}

func looksLikeBootDescriptor(value string) bool {
	if value == "all" {
		return true
	}
	for _, pattern := range bootDescriptorPatterns {
		if pattern.MatchString(value) {
			return true
		}
	}
	return false
}

func parseOptionalTimestampUsec(value string) (*uint64, error) {
	if strings.TrimSpace(value) == "" {
		return nil, nil
	}
	usec, err := parseTimestampUsec(value)
	if err != nil {
		return nil, err
	}
	return &usec, nil
}

func parseTimestampUsec(value string) (uint64, error) {
	value = strings.TrimSpace(value)
	switch value {
	case "now":
		return uint64(time.Now().UnixMicro()), nil
	case "today", "yesterday", "tomorrow":
		now := time.Now()
		day := time.Date(now.Year(), now.Month(), now.Day(), 0, 0, 0, 0, time.Local)
		if value == "yesterday" {
			day = day.AddDate(0, 0, -1)
		} else if value == "tomorrow" {
			day = day.AddDate(0, 0, 1)
		}
		return uint64(day.UnixMicro()), nil
	}
	if strings.HasPrefix(value, "@") {
		return parseEpochTimestampUsec(strings.TrimPrefix(value, "@"))
	}
	if len(value) > 1 && (value[0] == '+' || value[0] == '-') && !signedDatePrefixRe.MatchString(value) {
		delta, err := parseDurationUsec(value[1:])
		if err != nil {
			return 0, err
		}
		now := time.Now().UnixMicro()
		if value[0] == '+' {
			return uint64(now + int64(delta)), nil
		}
		return uint64(now - int64(delta)), nil
	}

	now := time.Now()
	for _, layout := range []string{
		"2006-01-02 15:04:05.999999",
		"2006-01-02 15:04:05",
		"2006-01-02 15:04",
		"2006-01-02",
	} {
		if t, err := time.ParseInLocation(layout, value, time.Local); err == nil {
			return uint64(t.UnixMicro()), nil
		}
	}
	for _, layout := range []string{"15:04:05.999999", "15:04:05", "15:04"} {
		if t, err := time.ParseInLocation(layout, value, time.Local); err == nil {
			t = time.Date(now.Year(), now.Month(), now.Day(), t.Hour(), t.Minute(), t.Second(), t.Nanosecond(), time.Local)
			return uint64(t.UnixMicro()), nil
		}
	}
	return 0, fmt.Errorf("failed to parse timestamp: %s", value)
}

func parseEpochTimestampUsec(value string) (uint64, error) {
	if !epochTimestampRe.MatchString(value) {
		return 0, fmt.Errorf("failed to parse timestamp: @%s", value)
	}
	whole, frac, _ := strings.Cut(value, ".")
	seconds, err := strconv.ParseUint(whole, 10, 64)
	if err != nil {
		return 0, err
	}
	frac = (frac + "000000")[:6]
	usec, err := strconv.ParseUint(frac, 10, 64)
	if err != nil {
		return 0, err
	}
	return seconds*1_000_000 + usec, nil
}

func parseDurationUsec(value string) (uint64, error) {
	units := map[string]float64{
		"us": 1, "usec": 1, "usecs": 1,
		"ms": 1_000, "msec": 1_000, "msecs": 1_000,
		"s": 1_000_000, "sec": 1_000_000, "secs": 1_000_000, "second": 1_000_000, "seconds": 1_000_000,
		"m": 60_000_000, "min": 60_000_000, "mins": 60_000_000, "minute": 60_000_000, "minutes": 60_000_000,
		"h": 3_600_000_000, "hr": 3_600_000_000, "hour": 3_600_000_000, "hours": 3_600_000_000,
		"d": 86_400_000_000, "day": 86_400_000_000, "days": 86_400_000_000,
		"w": 604_800_000_000, "week": 604_800_000_000, "weeks": 604_800_000_000,
	}
	var total float64
	pos := 0
	for _, match := range durationTokenRe.FindAllStringSubmatchIndex(value, -1) {
		if match[0] != pos {
			return 0, fmt.Errorf("failed to parse duration: %s", value)
		}
		number, err := strconv.ParseFloat(value[match[2]:match[3]], 64)
		if err != nil {
			return 0, err
		}
		unit := "s"
		if match[4] >= 0 {
			unit = strings.ToLower(value[match[4]:match[5]])
		}
		multiplier, ok := units[unit]
		if !ok {
			return 0, fmt.Errorf("failed to parse duration: %s", value)
		}
		total += number * multiplier
		pos = match[1]
	}
	if pos != len(value) || total == 0 {
		return 0, fmt.Errorf("failed to parse duration: %s", value)
	}
	return uint64(total), nil
}

type bootInfo struct {
	bootID     string
	firstEntry uint64
	lastEntry  uint64
	index      int
}

func collectBoots(j cliJournal) ([]bootInfo, error) {
	if err := j.SeekHead(); err != nil {
		return nil, err
	}
	boots := make(map[string]*bootInfo)
	for {
		ok, err := j.Next()
		if err != nil {
			return nil, err
		}
		if ok == 0 {
			break
		}
		entry, err := j.GetEntry()
		if err != nil {
			return nil, err
		}
		bootID := entry.BootID.String()
		if bootID == "" || strings.Trim(bootID, "0") == "" {
			continue
		}
		item := boots[bootID]
		if item == nil {
			boots[bootID] = &bootInfo{bootID: bootID, firstEntry: entry.Realtime, lastEntry: entry.Realtime}
		} else {
			if entry.Realtime < item.firstEntry {
				item.firstEntry = entry.Realtime
			}
			if entry.Realtime > item.lastEntry {
				item.lastEntry = entry.Realtime
			}
		}
	}
	out := make([]bootInfo, 0, len(boots))
	for _, item := range boots {
		out = append(out, *item)
	}
	sort.Slice(out, func(i, k int) bool {
		if out[i].firstEntry != out[k].firstEntry {
			return out[i].firstEntry < out[k].firstEntry
		}
		return out[i].bootID < out[k].bootID
	})
	base := 1 - len(out)
	for i := range out {
		out[i].index = base + i
	}
	return out, nil
}

func resolveBootID(j cliJournal, descriptor string) (string, error) {
	if descriptor == "all" {
		return "", nil
	}
	bootID, offset, err := parseBootDescriptor(descriptor)
	if err != nil {
		return "", err
	}
	boots, err := collectBoots(j)
	if err != nil {
		return "", err
	}
	if len(boots) == 0 {
		return "", errors.New("no journal boot entry found for the specified boot")
	}
	var target int
	if bootID != "" {
		base := -1
		for i, boot := range boots {
			if boot.bootID == bootID {
				base = i
				break
			}
		}
		if base < 0 {
			return "", fmt.Errorf("no journal boot entry found for the specified boot (%s%+d)", bootID, offset)
		}
		target = base + offset
	} else if offset > 0 {
		target = offset - 1
	} else {
		target = len(boots) - 1 + offset
	}
	if target < 0 || target >= len(boots) {
		return "", fmt.Errorf("no journal boot entry found for the specified boot (%s%+d)", bootID, offset)
	}
	return boots[target].bootID, nil
}

func parseBootDescriptor(descriptor string) (string, int, error) {
	if descriptor == "" {
		return "", 0, nil
	}
	m := bootDescriptorRe.FindStringSubmatch(descriptor)
	if m == nil {
		return "", 0, fmt.Errorf("failed to parse boot descriptor: %s", descriptor)
	}
	bootID := strings.ToLower(strings.ReplaceAll(m[1], "-", ""))
	offset := 0
	if m[4] != "" {
		var err error
		offset, err = strconv.Atoi(m[4])
		if err != nil {
			return "", 0, err
		}
	}
	return bootID, offset, nil
}

func entryInTimeRange(entry *journal.Entry, sinceUsec, untilUsec *uint64) bool {
	if sinceUsec != nil && entry.Realtime < *sinceUsec {
		return false
	}
	if untilUsec != nil && entry.Realtime > *untilUsec {
		return false
	}
	return true
}

func nextMatchingEntries(j cliJournal, sinceUsec, untilUsec *uint64, fn func(*journal.Entry) error) error {
	if sinceUsec != nil {
		if err := j.SeekRealtimeUsec(*sinceUsec); err != nil {
			return err
		}
	} else if err := j.SeekHead(); err != nil {
		return err
	}
	for {
		ok, err := j.Next()
		if err != nil {
			return err
		}
		if ok == 0 {
			return nil
		}
		entry, err := j.GetEntry()
		if err != nil {
			return err
		}
		if untilUsec != nil && entry.Realtime > *untilUsec {
			return nil
		}
		if entryInTimeRange(entry, sinceUsec, untilUsec) {
			if err := fn(entry); err != nil {
				return err
			}
		}
	}
}

func showForward(j cliJournal, limit int, sinceUsec, untilUsec *uint64, stdout io.Writer) error {
	count := 0
	err := nextMatchingEntries(j, sinceUsec, untilUsec, func(entry *journal.Entry) error {
		if limit > 0 && count >= limit {
			return errStopIteration
		}
		out, err := j.ProcessOutput(entry)
		if err != nil {
			return err
		}
		fmt.Fprint(stdout, out)
		count++
		return nil
	})
	if errors.Is(err, errStopIteration) {
		return nil
	}
	return err
}

func showTail(j cliJournal, limit int, sinceUsec, untilUsec *uint64, stdout io.Writer) error {
	var outputs []string
	if err := nextMatchingEntries(j, sinceUsec, untilUsec, func(entry *journal.Entry) error {
		out, err := j.ProcessOutput(entry)
		if err != nil {
			return err
		}
		outputs = append(outputs, out)
		return nil
	}); err != nil {
		return err
	}
	start := len(outputs) - limit
	if start < 0 {
		start = 0
	}
	for _, out := range outputs[start:] {
		fmt.Fprint(stdout, out)
	}
	return nil
}

type followEntry struct {
	cursor string
	output string
}

func scanFollowSnapshot(inputPath string, matches []string, boot optionalStringFlag, outputMode string, sinceUsec, untilUsec *uint64) []followEntry {
	j, err := openFilteredJournal(inputPath, matches, boot, outputMode)
	if err != nil {
		return nil
	}
	defer j.Close()
	var out []followEntry
	_ = nextMatchingEntries(j, sinceUsec, untilUsec, func(entry *journal.Entry) error {
		if entry.Cursor == "" {
			return nil
		}
		processed, err := j.ProcessOutput(entry)
		if err != nil {
			return nil
		}
		out = append(out, followEntry{cursor: entry.Cursor, output: processed})
		return nil
	})
	return out
}

func runFollow(inputPath string, matches []string, boot optionalStringFlag, outputMode string, sinceUsec, untilUsec *uint64, tail int, noTail bool, stdout io.Writer) error {
	seen := make(map[string]struct{})
	initial := scanFollowSnapshot(inputPath, matches, boot, outputMode, sinceUsec, untilUsec)
	for _, entry := range initial {
		seen[entry.cursor] = struct{}{}
	}
	toPrint := initial
	if !noTail && sinceUsec == nil && len(toPrint) > tail {
		toPrint = toPrint[len(toPrint)-tail:]
	}
	for _, entry := range toPrint {
		if _, err := fmt.Fprint(stdout, entry.output); err != nil {
			return err
		}
	}
	for {
		time.Sleep(100 * time.Millisecond)
		for _, entry := range scanFollowSnapshot(inputPath, matches, boot, outputMode, sinceUsec, untilUsec) {
			if _, ok := seen[entry.cursor]; ok {
				continue
			}
			seen[entry.cursor] = struct{}{}
			if _, err := fmt.Fprint(stdout, entry.output); err != nil {
				return err
			}
		}
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
	directoryInput := info.IsDir()
	if directoryInput {
		files, err = collectJournalFilesForVerify(inputPath)
		if err != nil {
			return fmt.Errorf("verify: read directory: %w", err)
		}
	} else {
		files = append(files, inputPath)
	}

	if len(files) == 0 {
		if directoryInput {
			return nil
		}
		return errors.New("verify: no journal files found")
	}

	var firstErr error
	for _, path := range files {
		sealed, err := isFileSealed(path)
		if err != nil {
			if directoryInput {
				continue
			}
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

func collectJournalFilesForVerify(path string) ([]string, error) {
	entries, err := os.ReadDir(path)
	if err != nil {
		return nil, err
	}

	var files []string
	for _, entry := range entries {
		candidate := filepath.Join(path, entry.Name())
		if isRegularFile(candidate) && isJournalFileName(entry.Name()) {
			files = append(files, candidate)
		}
	}

	for _, entry := range entries {
		if !isJournalSubdirName(entry.Name()) {
			continue
		}
		childPath := filepath.Join(path, entry.Name())
		if !isDirectory(childPath) {
			continue
		}
		children, err := os.ReadDir(childPath)
		if err != nil {
			continue
		}
		for _, child := range children {
			candidate := filepath.Join(childPath, child.Name())
			if isRegularFile(candidate) && isJournalFileName(child.Name()) {
				files = append(files, candidate)
			}
		}
	}

	sort.Strings(files)
	return files, nil
}

func isRegularFile(path string) bool {
	info, err := os.Stat(path)
	return err == nil && info.Mode().IsRegular()
}

func isDirectory(path string) bool {
	info, err := os.Stat(path)
	return err == nil && info.IsDir()
}

func isJournalSubdirName(name string) bool {
	if strings.Contains(name, ".") {
		return false
	}
	return id128StringValid(name)
}

func id128StringValid(s string) bool {
	if len(s) == 32 {
		for _, ch := range s {
			if !isASCIIHex(ch) {
				return false
			}
		}
		return true
	}
	if len(s) == 36 {
		for i, ch := range s {
			if i == 8 || i == 13 || i == 18 || i == 23 {
				if ch != '-' {
					return false
				}
				continue
			}
			if !isASCIIHex(ch) {
				return false
			}
		}
		return true
	}
	return false
}

func isASCIIHex(ch rune) bool {
	return (ch >= '0' && ch <= '9') || (ch >= 'a' && ch <= 'f') || (ch >= 'A' && ch <= 'F')
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

func flagWasSet(fs *flag.FlagSet, name string) bool {
	wasSet := false
	fs.Visit(func(f *flag.Flag) {
		if f.Name == name {
			wasSet = true
		}
	})
	return wasSet
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
