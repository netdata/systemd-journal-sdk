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

type cliFlags struct {
	file       *string
	directory  *string
	output     *string
	listBoots  *bool
	noTail     *bool
	follow     *bool
	boot       optionalStringFlag
	fields     *bool
	field      *string
	head       *int
	tail       *int
	since      *string
	until      *string
	sync       *bool
	flush      *bool
	rotate     *bool
	relinquish *bool
	verify     *bool
	verifyOnly *bool
	verifyKey  *string
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
	fs, flags := newCLIFlagSet(stderr)

	if err := fs.Parse(preprocessOptionalBootArgs(args)); err != nil {
		if errors.Is(err, flag.ErrHelp) {
			return nil
		}
		return err
	}

	if err := flags.validate(); err != nil {
		return err
	}

	hasVerifyKey := hasStringFlag(args, "verify-key")
	inputPath, err := flags.inputPath()
	if err != nil {
		return err
	}
	if *flags.verify || *flags.verifyOnly || hasVerifyKey {
		return runVerify(inputPath, *flags.verifyKey, hasVerifyKey, stdout, stderr)
	}

	sinceUsec, untilUsec, err := flags.timeBounds()
	if err != nil {
		return err
	}

	if *flags.follow {
		tail := 10
		if flagWasSet(fs, "tail") {
			tail = *flags.tail
		}
		return runFollow(inputPath, fs.Args(), flags.boot, *flags.output, sinceUsec, untilUsec, tail, *flags.noTail, stdout)
	}

	j, err := openFilteredJournal(inputPath, fs.Args(), flags.boot, *flags.output)
	if err != nil {
		return err
	}
	defer j.Close()

	return flags.dispatch(j, sinceUsec, untilUsec, stdout)
}

func newCLIFlagSet(stderr io.Writer) (*flag.FlagSet, *cliFlags) {
	fs := flag.NewFlagSet("journalctl", flag.ContinueOnError)
	fs.SetOutput(stderr)
	flags := &cliFlags{
		file:       fs.String("file", "", "journal file"),
		directory:  fs.String("directory", "", "journal directory"),
		output:     fs.String("output", "default", "output mode: default, json, export"),
		listBoots:  fs.Bool("list-boots", false, "list boots"),
		noTail:     fs.Bool("no-tail", false, "show all entries, start from the beginning"),
		follow:     fs.Bool("follow", false, "follow appended entries"),
		fields:     fs.Bool("fields", false, "show field names"),
		field:      fs.String("field", "", "show values for a field"),
		head:       fs.Int("head", 0, "show first N entries"),
		tail:       fs.Int("tail", 0, "show last N entries"),
		since:      fs.String("since", "", "show entries since timestamp"),
		until:      fs.String("until", "", "show entries until timestamp"),
		sync:       fs.Bool("sync", false, "sync journal (unsupported)"),
		flush:      fs.Bool("flush", false, "flush journal (unsupported)"),
		rotate:     fs.Bool("rotate", false, "rotate journal (unsupported)"),
		relinquish: fs.Bool("relinquish-var", false, "relinquish var (unsupported)"),
		verify:     fs.Bool("verify", false, "verify journal file"),
		verifyOnly: fs.Bool("verify-only", false, "verify only"),
		verifyKey:  fs.String("verify-key", "", "FSS verification key"),
	}
	fs.Var(&flags.boot, "boot", "boot filter")
	fs.Var(&flags.boot, "b", "boot filter")
	fs.StringVar(flags.field, "F", "", "show values for a field")
	fs.StringVar(flags.since, "S", "", "show entries since timestamp")
	fs.StringVar(flags.until, "U", "", "show entries until timestamp")
	fs.Usage = func() {
		fmt.Fprintf(stderr, "Usage: %s [options]\n", fs.Name())
		fmt.Fprintf(stderr, "Pure-Go systemd journal reader\n")
		fmt.Fprintf(stderr, "\nOptions:\n")
		fs.PrintDefaults()
	}
	return fs, flags
}

func (f *cliFlags) validate() error {
	if *f.sync || *f.flush || *f.rotate || *f.relinquish {
		return journal.ErrUnsupported
	}
	if *f.head < 0 {
		return errors.New("--head must be a non-negative integer")
	}
	if *f.tail < 0 {
		return errors.New("--tail must be a non-negative integer")
	}
	return nil
}

func (f *cliFlags) inputPath() (string, error) {
	inputPath := *f.file
	if inputPath == "" && *f.directory != "" {
		inputPath = *f.directory
	}
	if inputPath == "" {
		return "", errors.New("no journal file or directory specified (use --file or --directory)")
	}
	return inputPath, nil
}

func (f *cliFlags) timeBounds() (*uint64, *uint64, error) {
	sinceUsec, err := parseOptionalTimestampUsec(*f.since)
	if err != nil {
		return nil, nil, err
	}
	untilUsec, err := parseOptionalTimestampUsec(*f.until)
	if err != nil {
		return nil, nil, err
	}
	if sinceUsec != nil && untilUsec != nil && *sinceUsec > *untilUsec {
		return nil, nil, errors.New("--since= must be before --until=.")
	}
	return sinceUsec, untilUsec, nil
}

func (f *cliFlags) dispatch(j cliJournal, sinceUsec, untilUsec *uint64, stdout io.Writer) error {
	switch {
	case *f.listBoots:
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

	case *f.fields:
		fields, err := j.EnumerateFields()
		if err != nil {
			return fmt.Errorf("enumerate fields: %w", err)
		}
		sort.Strings(fields)
		for _, f := range fields {
			fmt.Fprintln(stdout, f)
		}
		return nil

	case *f.field != "":
		return j.VisitUnique(*f.field, func(value []byte) error {
			if _, err := stdout.Write(value); err != nil {
				return err
			}
			if _, err := fmt.Fprintln(stdout); err != nil {
				return err
			}
			return nil
		})

	case *f.head > 0:
		return showForward(j, *f.head, sinceUsec, untilUsec, stdout)

	case *f.tail > 0:
		return showTail(j, *f.tail, sinceUsec, untilUsec, stdout)

	default:
		return showForward(j, *f.head, sinceUsec, untilUsec, stdout)
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
	if usec, ok := parseRelativeTimestampUsec(value); ok {
		return usec, nil
	}
	if strings.HasPrefix(value, "@") {
		return parseEpochTimestampUsec(strings.TrimPrefix(value, "@"))
	}
	if usec, ok, err := parseSignedDurationTimestampUsec(value); ok || err != nil {
		return usec, err
	}

	now := time.Now()
	if usec, ok := parseDateTimestampUsec(value); ok {
		return usec, nil
	}
	if usec, ok := parseTimeOfDayTimestampUsec(value, now); ok {
		return usec, nil
	}
	return 0, fmt.Errorf("failed to parse timestamp: %s", value)
}

func parseRelativeTimestampUsec(value string) (uint64, bool) {
	switch value {
	case "now":
		return uint64(time.Now().UnixMicro()), true
	case "today", "yesterday", "tomorrow":
		now := time.Now()
		day := time.Date(now.Year(), now.Month(), now.Day(), 0, 0, 0, 0, time.Local)
		if value == "yesterday" {
			day = day.AddDate(0, 0, -1)
		} else if value == "tomorrow" {
			day = day.AddDate(0, 0, 1)
		}
		return uint64(day.UnixMicro()), true
	default:
		return 0, false
	}
}

func parseSignedDurationTimestampUsec(value string) (uint64, bool, error) {
	if len(value) <= 1 || (value[0] != '+' && value[0] != '-') || signedDatePrefixRe.MatchString(value) {
		return 0, false, nil
	}
	delta, err := parseDurationUsec(value[1:])
	if err != nil {
		return 0, true, err
	}
	now := time.Now().UnixMicro()
	if value[0] == '+' {
		return uint64(now + int64(delta)), true, nil
	}
	return uint64(now - int64(delta)), true, nil
}

func parseDateTimestampUsec(value string) (uint64, bool) {
	for _, layout := range []string{
		"2006-01-02 15:04:05.999999",
		"2006-01-02 15:04:05",
		"2006-01-02 15:04",
		"2006-01-02",
	} {
		if t, err := time.ParseInLocation(layout, value, time.Local); err == nil {
			return uint64(t.UnixMicro()), true
		}
	}
	return 0, false
}

func parseTimeOfDayTimestampUsec(value string, now time.Time) (uint64, bool) {
	for _, layout := range []string{"15:04:05.999999", "15:04:05", "15:04"} {
		if t, err := time.ParseInLocation(layout, value, time.Local); err == nil {
			t = time.Date(now.Year(), now.Month(), now.Day(), t.Hour(), t.Minute(), t.Second(), t.Nanosecond(), time.Local)
			return uint64(t.UnixMicro()), true
		}
	}
	return 0, false
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
		updateBootInfo(boots, bootID, entry.Realtime)
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

func updateBootInfo(boots map[string]*bootInfo, bootID string, realtime uint64) {
	item := boots[bootID]
	if item == nil {
		boots[bootID] = &bootInfo{bootID: bootID, firstEntry: realtime, lastEntry: realtime}
		return
	}
	if realtime < item.firstEntry {
		item.firstEntry = realtime
	}
	if realtime > item.lastEntry {
		item.lastEntry = realtime
	}
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
	if err := validateVerificationKeyOption(verifyKey, hasVerifyKey, stderr); err != nil {
		return err
	}

	files, directoryInput, err := verifyInputFiles(inputPath)
	if err != nil {
		return err
	}
	if len(files) == 0 {
		if directoryInput {
			return nil
		}
		return errors.New("verify: no journal files found")
	}

	var firstErr error
	for _, path := range files {
		if err := verifyOneFile(path, verifyKey, hasVerifyKey, directoryInput, stderr); err != nil {
			if firstErr == nil {
				firstErr = err
			}
		}
	}

	return firstErr
}

func validateVerificationKeyOption(verifyKey string, hasVerifyKey bool, stderr io.Writer) error {
	if !hasVerifyKey || validVerificationKey(verifyKey) {
		return nil
	}
	fmt.Fprintln(stderr, "Failed to parse seed.")
	return errors.New("failed to parse seed")
}

func verifyInputFiles(inputPath string) ([]string, bool, error) {
	info, err := os.Stat(inputPath)
	if err != nil {
		return nil, false, fmt.Errorf("verify: %w", err)
	}
	if !info.IsDir() {
		return []string{inputPath}, false, nil
	}
	files, err := collectJournalFilesForVerify(inputPath)
	if err != nil {
		return nil, true, fmt.Errorf("verify: read directory: %w", err)
	}
	return files, true, nil
}

func verifyOneFile(path, verifyKey string, hasVerifyKey, directoryInput bool, stderr io.Writer) error {
	sealed, err := isFileSealed(path)
	if err != nil {
		return reportVerifyOpenError(path, err, directoryInput, stderr)
	}
	if sealed && !hasVerifyKey {
		return reportVerifyMissingKey(path, stderr)
	}
	if err := verifyFileWithOptionalKey(path, verifyKey, sealed && hasVerifyKey); err != nil {
		fmt.Fprintf(stderr, "FAIL: %s (%v)\n", path, err)
		return err
	}
	fmt.Fprintf(stderr, "PASS: %s\n", path)
	return nil
}

func reportVerifyOpenError(path string, err error, directoryInput bool, stderr io.Writer) error {
	if directoryInput {
		return nil
	}
	fmt.Fprintf(stderr, "FAIL: %s (%v)\n", path, err)
	return err
}

func reportVerifyMissingKey(path string, stderr io.Writer) error {
	err := errors.New("verification key required for sealed journal file")
	fmt.Fprintf(stderr, "Journal file %s has sealing enabled but verification key has not been passed using --verify-key=.\n", path)
	fmt.Fprintf(stderr, "FAIL: %s (%v)\n", path, err)
	return err
}

func verifyFileWithOptionalKey(path, verifyKey string, useKey bool) error {
	if useKey {
		return journal.VerifyFileWithKey(path, verifyKey)
	}
	return journal.VerifyFile(path)
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
	seedEnd, ok := consumeVerificationSeed(key)
	if !ok || seedEnd >= len(key) || key[seedEnd] != '/' {
		return false
	}
	startEnd, ok := consumeHex(key, seedEnd+1)
	if !ok || startEnd >= len(key) || key[startEnd] != '-' {
		return false
	}
	end, ok := consumeHex(key, startEnd+1)
	return ok && end == len(key) && hexRangeHasNonZero(key[startEnd+1:end])
}

func consumeVerificationSeed(key string) (int, bool) {
	i := 0
	for c := 0; c < 12; c++ {
		next, ok := consumeVerificationSeedByte(key, i)
		if !ok {
			return 0, false
		}
		i = next
	}
	return i, true
}

func consumeVerificationSeedByte(key string, start int) (int, bool) {
	i := start
	for i < len(key) && key[i] == '-' {
		i++
	}
	if i+2 > len(key) || !isHex(key[i]) || !isHex(key[i+1]) {
		return 0, false
	}
	return i + 2, true
}

func hexRangeHasNonZero(s string) bool {
	for _, b := range s {
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
