package main

import (
	"crypto/rand"
	"encoding/binary"
	"encoding/hex"
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
	"unicode"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

// HEADER_COMPATIBLE_SEALED from systemd journal-def.h
const compatibleSealed = 1
const compatibleTailEntryBootID = 1 << 1
const compatibleSealedContinuous = 1 << 2
const incompatibleCompressedXZ = 1 << 0
const incompatibleCompressedLZ4 = 1 << 1
const incompatibleKeyedHash = 1 << 2
const incompatibleCompressedZSTD = 1 << 3
const incompatibleCompact = 1 << 4
const hashItemSizeBytes = 16
const journalHeaderSize = 272
const journalHeaderNEntriesOffset = 152
const headerChainDepthMax = 100
const coredumpMessageID = "fc2e22bc6ee647b6b90729ab34a250b1"

var (
	errStopIteration = errors.New("stop iteration")

	signedDatePrefixRe = regexp.MustCompile(`^[+-]\d{4}-`)
	epochTimestampRe   = regexp.MustCompile(`^\d+(\.\d+)?$`)
	durationTokenRe    = regexp.MustCompile(`\s*(\d+(?:\.\d+)?)(?:\s*([A-Za-z]+))?`)
	sizeTokenRe        = regexp.MustCompile(`^\s*(\d+(?:\.\d+)?)\s*([A-Za-z]*)\s*$`)

	bootDescriptorRe       = regexp.MustCompile(`^(([0-9A-Fa-f]{32})|([0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}))?([+-]?\d+)?$`)
	bootDescriptorPatterns = []*regexp.Regexp{
		regexp.MustCompile(`^[+-]?\d+$`),
		regexp.MustCompile(`^[0-9A-Fa-f]{32}([+-]\d+)?$`),
		regexp.MustCompile(`^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}([+-]\d+)?$`),
	}
	archivedJournalNameRe = regexp.MustCompile(`^.+@([0-9A-Fa-f]{32})-([0-9A-Fa-f]{16})-([0-9A-Fa-f]{16})\.journal$`)
	corruptJournalNameRe  = regexp.MustCompile(`^.+@([0-9A-Fa-f]{16})-([0-9A-Fa-f]{16})\.journal~$`)
	unitSuffixes          = []string{
		".automount",
		".device",
		".mount",
		".path",
		".scope",
		".service",
		".slice",
		".socket",
		".swap",
		".target",
		".timer",
	}
	systemUnitFieldsFull = []string{"_SYSTEMD_UNIT", "UNIT", "OBJECT_SYSTEMD_UNIT", "COREDUMP_UNIT", "_SYSTEMD_SLICE"}
	userUnitFieldsFull   = []string{"_SYSTEMD_USER_UNIT", "USER_UNIT", "OBJECT_SYSTEMD_USER_UNIT", "COREDUMP_USER_UNIT", "_SYSTEMD_USER_SLICE"}
	outputModeHelpList   = []string{
		"short",
		"short-full",
		"short-iso",
		"short-iso-precise",
		"short-precise",
		"short-monotonic",
		"short-delta",
		"short-unix",
		"verbose",
		"export",
		"json",
		"json-pretty",
		"json-sse",
		"json-seq",
		"cat",
		"with-unit",
	}
)

type cliJournal interface {
	Close() error
	AddMatch([]byte)
	AddDisjunction()
	AddConjunction()
	SeekHead() error
	SeekTail() error
	SeekRealtimeUsec(uint64) error
	SeekCursor(string) error
	TestCursor(string) (bool, error)
	Next() (int, error)
	Previous() (int, error)
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
	file       multiStringFlag
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

	// Parser-recognized v260.1 options. Each long option is registered
	// with `flag.Var` so the parser accepts it; the runtime dispatch
	// then either implements the requested behavior or returns a
	// portable-mode unsupported message. Source-of-truth for the option
	// list is tests/parser-parity/v260-manifest.json.

	systemFlag      *bool
	userFlag        *bool
	machineFlag     *string
	mergeFlag       *bool
	rootFlag        *string
	imageFlag       *string
	imagePolicyFlag *string
	namespaceFlag   *string

	cursorFlag            *string
	afterCursorFlag       *string
	cursorFileFlag        *string
	thisBootFlag          *bool
	unitFlag              multiStringFlag
	userUnitFlag          multiStringFlag
	invocationFlag        *string
	invocationShortFlag   *bool
	invocationSet         bool
	identifierFlag        multiStringFlag
	excludeIdentifierFlag multiStringFlag
	priorityFlag          multiStringFlag
	facilityFlag          multiStringFlag
	grepFlag              *string
	caseSensitiveFlag     optionalStringFlag
	dmesgFlag             *bool

	outputFieldsFlag      *string
	linesFlag             optionalStringFlag
	reverseFlag           *bool
	showCursorFlag        *bool
	utcFlag               *bool
	catalogFlag           *bool
	noHostnameFlag        *bool
	noFullFlag            *bool
	fullFlag              *bool
	allFlag               *bool
	truncateNewlineFlag   *bool
	quietFlag             *bool
	synchronizeOnExitFlag explicitStringFlag
	noPagerFlag           *bool
	pagerEndFlag          *bool

	intervalFlag  *string
	forceFlag     *bool
	setupKeysFlag *bool

	versionFlag            *bool
	newID128Flag           *bool
	listInvocationsFlag    *bool
	listNamespacesFlag     *bool
	diskUsageFlag          *bool
	vacuumSizeFlag         *string
	vacuumFilesFlag        *string
	vacuumTimeFlag         *string
	headerFlag             *bool
	listCatalogFlag        *bool
	dumpCatalogFlag        *bool
	updateCatalogFlag      *bool
	smartRelinquishVarFlag *bool
}

// multiStringFlag collects repeated `FIELD` occurrences into a slice while
// remaining compatible with Go's `flag` package contract.
type multiStringFlag struct {
	values []string
}

func (m *multiStringFlag) String() string {
	return strings.Join(m.values, ",")
}

func (m *multiStringFlag) Set(value string) error {
	m.values = append(m.values, value)
	return nil
}

func (m *multiStringFlag) Values() []string {
	return append([]string(nil), m.values...)
}

// explicitStringFlag records whether a required string option was supplied
// and the value as supplied. It does NOT register as a bool flag because
// `--synchronize-on-exit=true` must be distinguished from
// `--synchronize-on-exit=false`.
type explicitStringFlag struct {
	set   bool
	value string
}

func (f *explicitStringFlag) String() string {
	return f.value
}

func (f *explicitStringFlag) Set(value string) error {
	f.set = true
	f.value = value
	return nil
}

func (f *explicitStringFlag) IsBoolFlag() bool {
	return false
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
	flags.invocationSet = flagWasSet(fs, "invocation") || *flags.invocationShortFlag
	if *flags.output == "help" {
		printOutputModeHelp(stdout)
		return nil
	}

	if err := flags.validate(); err != nil {
		return err
	}

	if *flags.versionFlag {
		fmt.Fprintln(stdout, "journalctl (systemd-journal-sdk Go rewrite)")
		fmt.Fprintln(stdout, "baseline: systemd v260.1 (c0a5a2516d28)")
		fmt.Fprintln(stdout, "portable file-backed mode")
		return nil
	}
	if *flags.newID128Flag {
		return printNewID128(stdout)
	}

	if facilityHelpRequested(flags.facilityFlag.Values()) {
		printFacilityHelp(stdout, *flags.quietFlag)
		return nil
	}

	hasVerifyKey := hasStringFlag(args, "verify-key")
	input, err := flags.input()
	if err != nil {
		return err
	}
	if err := validatePathMatchArguments(fs.Args()); err != nil {
		return err
	}
	if *flags.diskUsageFlag {
		return runDiskUsage(input, stdout)
	}
	if hasVacuumFlags(flags) {
		return runVacuum(input.directory, flags, stderr)
	}
	if *flags.headerFlag {
		return runHeader(input, stdout)
	}
	if *flags.listInvocationsFlag {
		return runListInvocations(input, flags, stdout)
	}
	if *flags.verify || *flags.verifyOnly || hasVerifyKey {
		return runVerify(input, *flags.verifyKey, hasVerifyKey, stdout, stderr)
	}

	sinceUsec, untilUsec, err := flags.timeBounds()
	if err != nil {
		return err
	}
	postFilters, err := newCLIPostFilters(flags)
	if err != nil {
		return err
	}
	cursorControl, err := newCursorControl(flags)
	if err != nil {
		return err
	}

	outputOptions := newOutputOptions(flags, flagWasSet(fs, "output-fields"), resolveFullWidth(args))

	if *flags.follow {
		tail := 10
		if flagWasSet(fs, "tail") {
			tail = *flags.tail
		}
		if flagWasSet(fs, "lines") {
			limit, err := parseLinesLimitValue(flags.linesFlag.value)
			if err != nil {
				return err
			}
			if limit.set && !limit.all {
				tail = limit.count
			}
		}
		return runFollow(input, fs.Args(), flags, *flags.output, sinceUsec, untilUsec, tail, *flags.noTail, stdout, postFilters, cursorControl, outputOptions)
	}

	j, err := openFilteredJournal(input, fs.Args(), flags, *flags.output)
	if err != nil {
		return err
	}
	defer j.Close()

	return flags.dispatch(j, sinceUsec, untilUsec, stdout, postFilters, cursorControl, outputOptions)
}

func resolveFullWidth(args []string) bool {
	fullWidth := true
	for _, arg := range args {
		switch arg {
		case "--no-full":
			fullWidth = false
		case "--full", "-l":
			fullWidth = true
		}
	}
	return fullWidth
}

func printOutputModeHelp(stdout io.Writer) {
	for _, mode := range outputModeHelpList {
		fmt.Fprintln(stdout, mode)
	}
}

type linesLimit struct {
	set    bool
	all    bool
	oldest bool
	count  int
}

// parseLinesLimitValue parses a `--lines=[+]N` value. A missing optional
// argument uses systemd's default count of 10. The `+` prefix means oldest
// entries, not tail entries.
func parseLinesLimitValue(value string) (linesLimit, error) {
	if value == "" {
		return linesLimit{set: true, count: 10}, nil
	}
	if value == "all" {
		return linesLimit{set: true, all: true}, nil
	}
	oldest := strings.HasPrefix(value, "+")
	stripped := strings.TrimPrefix(value, "+")
	n, err := strconv.Atoi(stripped)
	if err != nil {
		return linesLimit{}, fmt.Errorf("failed to parse --lines value: %s", value)
	}
	return linesLimit{set: true, oldest: oldest, count: n}, nil
}

type cursorSeek struct {
	cursor string
	after  bool
}

type cursorControl struct {
	seek       *cursorSeek
	updateFile string
}

func newCursorControl(f *cliFlags) (cursorControl, error) {
	if *f.cursorFlag != "" {
		return cursorControl{seek: &cursorSeek{cursor: *f.cursorFlag}}, nil
	}
	if *f.afterCursorFlag != "" {
		return cursorControl{seek: &cursorSeek{cursor: *f.afterCursorFlag, after: true}}, nil
	}
	if *f.cursorFileFlag == "" {
		return cursorControl{}, nil
	}

	content, err := os.ReadFile(*f.cursorFileFlag) // nosec G304 - caller explicitly supplies --cursor-file path.
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return cursorControl{updateFile: *f.cursorFileFlag}, nil
		}
		return cursorControl{}, fmt.Errorf("Failed to read cursor file %s: %w", *f.cursorFileFlag, err)
	}
	cursor := strings.SplitN(string(content), "\n", 2)[0]
	cursor = strings.TrimSuffix(cursor, "\r")
	control := cursorControl{updateFile: *f.cursorFileFlag}
	if cursor != "" {
		control.seek = &cursorSeek{cursor: cursor, after: true}
	}
	return control, nil
}

func newCLIFlagSet(stderr io.Writer) (*flag.FlagSet, *cliFlags) {
	fs := flag.NewFlagSet("journalctl", flag.ContinueOnError)
	fs.SetOutput(stderr)
	flags := &cliFlags{
		directory:  fs.String("directory", "", "journal directory"),
		output:     fs.String("output", "short", "output mode: short, short-full, short-iso, short-iso-precise, short-precise, short-monotonic, short-delta, short-unix, verbose, export, json, json-pretty, json-sse, json-seq, cat, with-unit"),
		listBoots:  fs.Bool("list-boots", false, "list boots"),
		noTail:     fs.Bool("no-tail", false, "show all entries, start from the beginning"),
		follow:     fs.Bool("follow", false, "follow appended entries"),
		fields:     fs.Bool("fields", false, "show field names"),
		field:      fs.String("field", "", "show values for a field"),
		head:       fs.Int("head", 0, "show first N entries"),
		tail:       fs.Int("tail", 0, "show last N entries"),
		since:      fs.String("since", "", "show entries since timestamp"),
		until:      fs.String("until", "", "show entries until timestamp"),
		sync:       fs.Bool("sync", false, "sync journal (portable mode does not support this)"),
		flush:      fs.Bool("flush", false, "flush journal (portable mode does not support this)"),
		rotate:     fs.Bool("rotate", false, "rotate journal (portable mode does not support this)"),
		relinquish: fs.Bool("relinquish-var", false, "relinquish var (portable mode does not support this)"),
		verify:     fs.Bool("verify", false, "verify journal file"),
		verifyOnly: fs.Bool("verify-only", false, "verify only"),
		verifyKey:  fs.String("verify-key", "", "FSS verification key"),

		systemFlag:      fs.Bool("system", false, "show the system journal (portable: no-op)"),
		userFlag:        fs.Bool("user", false, "show the user journal (portable: no-op)"),
		machineFlag:     fs.String("machine", "", "operate on local container (portable: unsupported)"),
		mergeFlag:       fs.Bool("merge", false, "merge entries from available journals (portable: no-op)"),
		rootFlag:        fs.String("root", "", "alternate filesystem root (portable: unsupported)"),
		imageFlag:       fs.String("image", "", "disk image filesystem root (portable: unsupported)"),
		imagePolicyFlag: fs.String("image-policy", "", "disk image dissection policy (portable: unsupported)"),
		namespaceFlag:   fs.String("namespace", "", "journal namespace (portable: unsupported)"),

		cursorFlag:          fs.String("cursor", "", "start at the specified cursor"),
		afterCursorFlag:     fs.String("after-cursor", "", "start after the specified cursor"),
		cursorFileFlag:      fs.String("cursor-file", "", "use cursor from FILE and update FILE"),
		thisBootFlag:        fs.Bool("this-boot", false, "deprecated alias for --boot"),
		invocationFlag:      fs.String("invocation", "", "show logs from the matching invocation ID"),
		invocationShortFlag: fs.Bool("I", false, "show logs from the latest invocation of unit"),
		grepFlag:            fs.String("grep", "", "show entries with MESSAGE matching PATTERN"),
		dmesgFlag:           fs.Bool("dmesg", false, "show kernel message log from the current boot"),

		outputFieldsFlag:    fs.String("output-fields", "", "select fields to print in verbose/export/json modes"),
		reverseFlag:         fs.Bool("reverse", false, "show newest entries first"),
		showCursorFlag:      fs.Bool("show-cursor", false, "print the cursor after all entries"),
		utcFlag:             fs.Bool("utc", false, "express timestamps in UTC"),
		catalogFlag:         fs.Bool("catalog", false, "add message explanations (portable: no-op)"),
		noHostnameFlag:      fs.Bool("no-hostname", false, "suppress hostname field"),
		noFullFlag:          fs.Bool("no-full", false, "ellipsize fields"),
		fullFlag:            fs.Bool("full", false, "enable full-width output"),
		allFlag:             fs.Bool("all", false, "show all fields including long/unprintable"),
		truncateNewlineFlag: fs.Bool("truncate-newline", false, "truncate entries by first newline"),
		quietFlag:           fs.Bool("quiet", false, "do not show info messages and privilege warning"),
		noPagerFlag:         fs.Bool("no-pager", false, "do not pipe output into a pager (portable: no-op)"),
		pagerEndFlag:        fs.Bool("pager-end", false, "immediately jump to the end in the pager"),

		intervalFlag:  fs.String("interval", "", "FSS sealing key change interval"),
		forceFlag:     fs.Bool("force", false, "override the FSS key pair with --setup-keys"),
		setupKeysFlag: fs.Bool("setup-keys", false, "generate a new FSS key pair"),

		versionFlag:            fs.Bool("version", false, "show package version"),
		newID128Flag:           fs.Bool("new-id128", false, "print a new ID128 (deprecated utility action)"),
		listInvocationsFlag:    fs.Bool("list-invocations", false, "show invocation IDs of specified unit"),
		listNamespacesFlag:     fs.Bool("list-namespaces", false, "show list of journal namespaces (portable: unsupported)"),
		diskUsageFlag:          fs.Bool("disk-usage", false, "show total disk usage of all journal files"),
		vacuumSizeFlag:         fs.String("vacuum-size", "", "reduce disk usage below specified size (portable: maintenance)"),
		vacuumFilesFlag:        fs.String("vacuum-files", "", "leave only the specified number of journal files (portable: maintenance)"),
		vacuumTimeFlag:         fs.String("vacuum-time", "", "remove journal files older than specified time (portable: maintenance)"),
		headerFlag:             fs.Bool("header", false, "show journal header information"),
		listCatalogFlag:        fs.Bool("list-catalog", false, "show all message IDs in the catalog (portable: unsupported)"),
		dumpCatalogFlag:        fs.Bool("dump-catalog", false, "show entries in the message catalog (portable: unsupported)"),
		updateCatalogFlag:      fs.Bool("update-catalog", false, "update the message catalog database (portable: unsupported)"),
		smartRelinquishVarFlag: fs.Bool("smart-relinquish-var", false, "stop logging to disk with mount inspection (portable: unsupported)"),
	}
	fs.Var(&flags.file, "file", "journal file")
	fs.Var(&flags.boot, "boot", "boot filter")
	fs.Var(&flags.boot, "b", "boot filter")
	fs.StringVar(flags.field, "F", "", "show values for a field")
	fs.StringVar(flags.since, "S", "", "show entries since timestamp")
	fs.StringVar(flags.until, "U", "", "show entries until timestamp")

	fs.Var(&flags.unitFlag, "unit", "show logs from the specified unit")
	fs.Var(&flags.unitFlag, "u", "show logs from the specified unit (short)")
	fs.Var(&flags.userUnitFlag, "user-unit", "show logs from the specified user unit")
	fs.Var(&flags.identifierFlag, "identifier", "show entries with the specified syslog identifier")
	fs.Var(&flags.identifierFlag, "t", "show entries with the specified syslog identifier (short)")
	fs.Var(&flags.excludeIdentifierFlag, "exclude-identifier", "hide entries with the specified syslog identifier")
	fs.Var(&flags.excludeIdentifierFlag, "T", "hide entries with the specified syslog identifier (short)")
	fs.Var(&flags.priorityFlag, "priority", "show entries within the specified priority range")
	fs.Var(&flags.priorityFlag, "p", "show entries within the specified priority range (short)")
	fs.Var(&flags.facilityFlag, "facility", "show entries with the specified facilities")

	fs.Var(&flags.linesFlag, "lines", "number of journal entries to show")
	fs.Var(&flags.linesFlag, "n", "number of journal entries to show (short)")
	fs.Var(&flags.caseSensitiveFlag, "case-sensitive", "force case sensitive or insensitive matching")
	fs.Var(&flags.synchronizeOnExitFlag, "synchronize-on-exit", "wait for Journal synchronization before exiting (portable: unsupported)")

	fs.BoolVar(flags.allFlag, "a", false, "show all fields including long/unprintable (short)")
	fs.BoolVar(flags.follow, "f", false, "follow appended entries (short)")
	fs.BoolVar(flags.fullFlag, "l", false, "enable full-width output (short)")
	fs.BoolVar(flags.dmesgFlag, "k", false, "show kernel message log from the current boot (short)")
	fs.BoolVar(flags.mergeFlag, "m", false, "merge entries from available journals (short)")
	fs.BoolVar(flags.quietFlag, "q", false, "do not show info messages and privilege warning (short)")
	fs.BoolVar(flags.reverseFlag, "r", false, "show newest entries first (short)")
	fs.BoolVar(flags.catalogFlag, "x", false, "add message explanations (short)")
	fs.BoolVar(flags.pagerEndFlag, "e", false, "immediately jump to the end in the pager (short)")
	fs.BoolVar(flags.noHostnameFlag, "W", false, "suppress hostname field (short)")

	fs.StringVar(flags.cursorFlag, "c", "", "start at the specified cursor (short)")
	fs.StringVar(flags.directory, "D", "", "journal directory (short)")
	fs.Var(&flags.file, "i", "journal file (short)")
	fs.StringVar(flags.machineFlag, "M", "", "operate on local container (short) (portable: unsupported)")
	fs.StringVar(flags.output, "o", "short", "change journal output mode (short)")
	fs.BoolVar(flags.fields, "N", false, "list all field names currently used (short)")

	fs.Usage = func() {
		fmt.Fprintf(stderr, "Usage: %s [options]\n", fs.Name())
		fmt.Fprintf(stderr, "Pure-Go systemd journal reader (portable mode, systemd v260.1 baseline)\n")
		fmt.Fprintf(stderr, "\nOptions:\n")
		fs.PrintDefaults()
	}
	return fs, flags
}

func (f *cliFlags) validate() error {
	// Source exclusivity: --directory=, --file=, --machine=, --root=,
	// --image= are mutually exclusive.
	sources := 0
	if len(f.file.values) > 0 {
		sources++
	}
	if *f.directory != "" {
		sources++
	}
	if *f.machineFlag != "" {
		sources++
	}
	if *f.rootFlag != "" {
		sources++
	}
	if *f.imageFlag != "" {
		sources++
	}
	if sources > 1 {
		return errors.New("Please specify at most one of -D/--directory=, --file=, -M/--machine=, --root=, --image=.")
	}

	// Since/until order.
	sinceUsec, err := parseOptionalTimestampUsec(*f.since)
	if err != nil {
		return err
	}
	untilUsec, err := parseOptionalTimestampUsec(*f.until)
	if err != nil {
		return err
	}
	if sinceUsec != nil && untilUsec != nil && *sinceUsec > *untilUsec {
		return errors.New("--since= must be before --until=.")
	}

	// Cursor source exclusivity.
	cursorSources := 0
	if *f.cursorFlag != "" {
		cursorSources++
	}
	if *f.afterCursorFlag != "" {
		cursorSources++
	}
	if *f.cursorFileFlag != "" {
		cursorSources++
	}
	if *f.since != "" {
		cursorSources++
	}
	if cursorSources > 1 {
		return errors.New("Please specify only one of --since=, --cursor=, --cursor-file=, and --after-cursor=.")
	}

	// Follow/reverse conflict.
	if *f.follow && *f.reverseFlag {
		return errors.New("Please specify either --reverse or --follow, not both.")
	}

	// Oldest-lines conflict.
	if f.linesFlag.set && strings.HasPrefix(f.linesFlag.value, "+") && (*f.reverseFlag || *f.follow) {
		return errors.New("--lines=+N is unsupported when --reverse or --follow is specified.")
	}

	// Boot/merge conflict.
	if (f.boot.set || *f.thisBootFlag || *f.listBoots) && *f.mergeFlag {
		return errors.New("Using --boot or --list-boots with --merge is not supported.")
	}

	// Reject intentionally unsupported options with the portable-mode
	// contract.
	if *f.machineFlag != "" {
		return portableUnsupported("--machine", "requires local container or machine journal access; portable mode never connects to a host or container")
	}
	if *f.rootFlag != "" {
		return portableUnsupported("--root", "requires alternate root filesystem discovery and catalog hierarchy access; portable mode never inspects host rootfs")
	}
	if *f.imageFlag != "" {
		return portableUnsupported("--image", "requires disk image dissection and mounting; portable mode never mounts or inspects images")
	}
	if *f.imagePolicyFlag != "" {
		return portableUnsupported("--image-policy", "only meaningful with --image= which is not portable")
	}
	if *f.namespaceFlag != "" {
		return portableUnsupported("--namespace", "requires systemd journal namespaces; portable mode never discovers host namespaces")
	}
	if f.synchronizeOnExitFlag.set && !isFalsey(f.synchronizeOnExitFlag.value) {
		return portableUnsupported("--synchronize-on-exit", "requires journald Varlink synchronization on signal exit")
	}
	if *f.sync {
		return portableUnsupported("--sync", "daemon-only journal synchronization; no journald in portable mode")
	}
	if *f.flush {
		return portableUnsupported("--flush", "daemon-only runtime-to-persistent flush; no journald in portable mode")
	}
	if *f.rotate {
		return portableUnsupported("--rotate", "daemon-only journald rotation request; use --vacuum-* with explicit --directory= instead")
	}
	if *f.relinquish {
		return portableUnsupported("--relinquish-var", "daemon-only journald storage transition; no journald in portable mode")
	}
	if *f.smartRelinquishVarFlag {
		return portableUnsupported("--smart-relinquish-var", "daemon-only journald storage transition plus host mount inspection")
	}
	if *f.listNamespacesFlag {
		return portableUnsupported("--list-namespaces", "requires host journal namespace discovery")
	}
	if *f.listCatalogFlag {
		return portableUnsupported("--list-catalog", "host catalog database action; portable commands do not read host catalog databases")
	}
	if *f.dumpCatalogFlag {
		return portableUnsupported("--dump-catalog", "host catalog database action; portable commands do not read host catalog databases")
	}
	if *f.updateCatalogFlag {
		return portableUnsupported("--update-catalog", "host catalog database mutation; portable commands do not mutate host catalog databases")
	}
	if *f.setupKeysFlag {
		return portableUnsupported("--setup-keys", "FSS key pair generation requires journald integration; portable mode has no host journald")
	}
	if *f.diskUsageFlag && len(f.file.values) == 0 && *f.directory == "" {
		return portableUnsupported("--disk-usage", "requires host journal directory; pass --file or --directory to compute disk usage for explicit input")
	}
	if *f.vacuumSizeFlag != "" || *f.vacuumFilesFlag != "" || *f.vacuumTimeFlag != "" {
		if *f.directory == "" {
			return portableUnsupported("--vacuum-*", "vacuum actions require explicit --directory= input")
		}
	}

	if *f.head < 0 {
		return errors.New("--head must be a non-negative integer")
	}
	if *f.tail < 0 {
		return errors.New("--tail must be a non-negative integer")
	}
	return nil
}

func portableUnsupported(feature, reason string) error {
	return fmt.Errorf("journalctl portable mode does not support %s: %s", feature, reason)
}

func isFalsey(value string) bool {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "", "false", "no", "0", "off":
		return true
	}
	return false
}

type cliInput struct {
	directory string
	files     []string
}

func (i cliInput) openJournal() (cliJournal, error) {
	if i.directory != "" {
		return journal.SdJournalOpenDirectory(i.directory, 0)
	}
	return journal.SdJournalOpenFiles(i.files, 0)
}

func (i cliInput) journalFiles(context string) ([]string, bool, error) {
	if i.directory == "" {
		return append([]string(nil), i.files...), false, nil
	}
	files, err := collectJournalFilesForVerify(i.directory)
	if err != nil {
		return nil, true, fmt.Errorf("%s: read directory: %w", context, err)
	}
	return files, true, nil
}

func (f *cliFlags) input() (cliInput, error) {
	if *f.directory != "" {
		return cliInput{directory: *f.directory}, nil
	}
	files, err := resolveFileInputs(f.file.Values())
	if err != nil {
		return cliInput{}, err
	}
	if len(files) > 0 {
		return cliInput{files: files}, nil
	}
	return cliInput{}, portableUnsupported(
		"default journal source",
		"default host journal discovery is not portable; pass --file or --directory",
	)
}

func resolveFileInputs(values []string) ([]string, error) {
	var files []string
	for _, value := range values {
		if value == "-" {
			return nil, portableUnsupported(
				"--file=-",
				"stdin-backed journals require seekable mmap-capable file descriptors and are not supported in portable mode",
			)
		}
		matches, err := filepath.Glob(value)
		if err != nil {
			return nil, fmt.Errorf("failed to add paths: %w", err)
		}
		if len(matches) == 0 {
			files = append(files, value)
			continue
		}
		files = append(files, matches...)
	}
	return files, nil
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

func validatePathMatchArguments(matches []string) error {
	for _, item := range matches {
		if item != "+" && !strings.Contains(item, "=") {
			return portableUnsupported(
				"path match argument",
				"portable mode supports FIELD=VALUE matches and '+' disjunctions only; path matches require host filesystem metadata inspection",
			)
		}
	}
	return nil
}

func effectiveBootFlag(flags *cliFlags) optionalStringFlag {
	if flags.boot.set {
		return flags.boot
	}
	if *flags.thisBootFlag {
		return optionalStringFlag{set: true}
	}
	return optionalStringFlag{}
}

func applyCLIMatches(j cliJournal, flags *cliFlags) error {
	if err := addJournalctlUnitMatches(j, flags.unitFlag.Values(), flags.userUnitFlag.Values()); err != nil {
		return err
	}

	if *flags.dmesgFlag {
		if err := addFieldMatches(j, "_TRANSPORT", []string{"kernel"}); err != nil {
			return err
		}
	}
	if identifiers := flags.identifierFlag.Values(); len(identifiers) > 0 {
		if err := addFieldMatches(j, "SYSLOG_IDENTIFIER", identifiers); err != nil {
			return err
		}
	}
	priorities, err := parsePriorityFilter(flags.priorityFlag.Values())
	if err != nil {
		return err
	}
	if len(priorities) > 0 {
		values := make([]string, 0, len(priorities))
		for _, priority := range priorities {
			values = append(values, strconv.Itoa(int(priority)))
		}
		if err := addFieldMatches(j, "PRIORITY", values); err != nil {
			return err
		}
	}
	facilities, err := parseFacilityFilter(flags.facilityFlag.Values())
	if err != nil {
		return err
	}
	if len(facilities) > 0 {
		values := make([]string, 0, len(facilities))
		for _, facility := range facilities {
			values = append(values, strconv.Itoa(int(facility)))
		}
		if err := addFieldMatches(j, "SYSLOG_FACILITY", values); err != nil {
			return err
		}
	}
	return nil
}

func addFieldMatches(j cliJournal, field string, values []string) error {
	if len(values) == 0 {
		return nil
	}
	for _, value := range values {
		match, err := journal.ParseMatchString(field + "=" + value)
		if err != nil {
			return err
		}
		j.AddMatch(match)
	}
	j.AddConjunction()
	return nil
}

func addInvocationMatches(j cliJournal, id string) error {
	fields := []string{
		"_SYSTEMD_INVOCATION_ID",
		"OBJECT_SYSTEMD_INVOCATION_ID",
		"INVOCATION_ID",
		"USER_INVOCATION_ID",
	}
	for i, field := range fields {
		if i > 0 {
			j.AddDisjunction()
		}
		if err := addMatchPair(j, field, id); err != nil {
			return err
		}
	}
	j.AddConjunction()
	return nil
}

func addMatchPair(j cliJournal, field, value string) error {
	match, err := journal.ParseMatchString(field + "=" + value)
	if err != nil {
		return err
	}
	j.AddMatch(match)
	return nil
}

func applyBootMatch(j cliJournal, flags *cliFlags) error {
	boot := effectiveBootFlag(flags)
	if !boot.set || strings.TrimSpace(boot.value) == "all" {
		return nil
	}
	bootID, err := resolveBootID(j, strings.TrimSpace(boot.value))
	if err != nil {
		return err
	}
	if bootID == "" {
		return nil
	}
	match, err := journal.ParseMatchString("_BOOT_ID=" + bootID)
	if err != nil {
		return err
	}
	j.AddMatch(match)
	j.AddConjunction()
	return nil
}

func addMatchGroup(j cliJournal, pairs [][2]string) error {
	for _, pair := range pairs {
		if err := addMatchPair(j, pair[0], pair[1]); err != nil {
			return err
		}
	}
	return nil
}

func addImpossibleMatch(j cliJournal, reason string) error {
	if err := addMatchPair(j, "__JOURNALCTL_NEVER_MATCH", reason); err != nil {
		return err
	}
	j.AddConjunction()
	return nil
}

func addJournalctlUnitMatches(j cliJournal, systemUnits, userUnits []string) error {
	if len(systemUnits) == 0 && len(userUnits) == 0 {
		return nil
	}

	added := false
	expandedSystemUnits, err := expandUnitSpecs(j, systemUnits, systemUnitFieldsFull)
	if err != nil {
		return err
	}
	for _, unit := range expandedSystemUnits {
		if err := addSystemUnitMatchGroups(j, unit); err != nil {
			return err
		}
		added = true
	}

	expandedUserUnits, err := expandUnitSpecs(j, userUnits, userUnitFieldsFull)
	if err != nil {
		return err
	}
	uid, uidOK := currentUIDString()
	for _, unit := range expandedUserUnits {
		if err := addUserUnitMatchGroups(j, unit, uid, uidOK); err != nil {
			return err
		}
		added = true
	}

	if !added {
		return addImpossibleMatch(j, "unit-glob")
	}
	j.AddConjunction()
	return nil
}

func addSystemUnitMatchGroups(j cliJournal, unit string) error {
	if err := addMatchGroup(j, [][2]string{{"_SYSTEMD_UNIT", unit}}); err != nil {
		return err
	}
	j.AddDisjunction()

	if err := addMatchGroup(j, [][2]string{{"_SYSTEMD_CGROUP", "/init.scope"}, {"UNIT", unit}}); err != nil {
		return err
	}
	j.AddDisjunction()

	if err := addMatchGroup(j, [][2]string{{"_UID", "0"}, {"OBJECT_SYSTEMD_UNIT", unit}}); err != nil {
		return err
	}
	j.AddDisjunction()

	if err := addMatchGroup(j, [][2]string{{"MESSAGE_ID", coredumpMessageID}, {"COREDUMP_UNIT", unit}}); err != nil {
		return err
	}

	if strings.HasSuffix(unit, ".slice") {
		j.AddDisjunction()
		if err := addMatchGroup(j, [][2]string{{"_SYSTEMD_SLICE", unit}}); err != nil {
			return err
		}
	}

	j.AddDisjunction()
	return nil
}

func addUserUnitMatchGroups(j cliJournal, unit, uid string, uidOK bool) error {
	if err := addUserUnitMatchGroup(j, [][2]string{{"_SYSTEMD_USER_UNIT", unit}}, uid, uidOK, false); err != nil {
		return err
	}
	j.AddDisjunction()

	if err := addUserUnitMatchGroup(j, [][2]string{{"USER_UNIT", unit}}, uid, uidOK, false); err != nil {
		return err
	}
	j.AddDisjunction()

	if err := addUserUnitMatchGroup(j, [][2]string{{"OBJECT_SYSTEMD_USER_UNIT", unit}}, uid, uidOK, true); err != nil {
		return err
	}
	j.AddDisjunction()

	if err := addUserUnitMatchGroup(j, [][2]string{{"COREDUMP_USER_UNIT", unit}}, uid, uidOK, true); err != nil {
		return err
	}

	if strings.HasSuffix(unit, ".slice") {
		j.AddDisjunction()
		if err := addUserUnitMatchGroup(j, [][2]string{{"_SYSTEMD_USER_SLICE", unit}}, uid, uidOK, false); err != nil {
			return err
		}
	}

	j.AddDisjunction()
	return nil
}

func addUserUnitMatchGroup(j cliJournal, pairs [][2]string, uid string, uidOK, includeRootUID bool) error {
	if err := addMatchGroup(j, pairs); err != nil {
		return err
	}
	if uidOK {
		if err := addMatchPair(j, "_UID", uid); err != nil {
			return err
		}
		if includeRootUID {
			if err := addMatchPair(j, "_UID", "0"); err != nil {
				return err
			}
		}
	}
	return nil
}

func expandUnitSpecs(j cliJournal, specs []string, fields []string) ([]string, error) {
	var out []string
	seen := make(map[string]struct{})
	var patterns []string

	for _, spec := range specs {
		unit := mangleUnitName(spec)
		if isGlobPattern(unit) {
			patterns = append(patterns, unit)
			continue
		}
		if _, ok := seen[unit]; !ok {
			seen[unit] = struct{}{}
			out = append(out, unit)
		}
	}

	if len(patterns) == 0 {
		return out, nil
	}

	for _, field := range fields {
		err := j.VisitUnique(field, func(value []byte) error {
			unit := string(value)
			if !matchesAnyGlob(patterns, unit) {
				return nil
			}
			if _, ok := seen[unit]; ok {
				return nil
			}
			seen[unit] = struct{}{}
			out = append(out, unit)
			return nil
		})
		if err != nil {
			return nil, fmt.Errorf("query possible units for %s: %w", field, err)
		}
	}

	return out, nil
}

func mangleUnitName(value string) string {
	value = strings.TrimSpace(value)
	for _, suffix := range unitSuffixes {
		if strings.HasSuffix(value, suffix) {
			return value
		}
	}
	return value + ".service"
}

func isGlobPattern(value string) bool {
	return strings.ContainsAny(value, "*?[")
}

func matchesAnyGlob(patterns []string, value string) bool {
	for _, pattern := range patterns {
		if globPatternMatches(pattern, value) {
			return true
		}
	}
	return false
}

func globPatternMatches(pattern, value string) bool {
	re, err := regexp.Compile(globPatternToRegex(pattern))
	return err == nil && re.MatchString(value)
}

func globPatternToRegex(pattern string) string {
	var b strings.Builder
	b.WriteString("^")
	runes := []rune(pattern)
	for i := 0; i < len(runes); i++ {
		switch runes[i] {
		case '*':
			b.WriteString(".*")
		case '?':
			b.WriteByte('.')
		case '[':
			class := []rune{'['}
			closed := false
			if i+1 < len(runes) && (runes[i+1] == '!' || runes[i+1] == '^') {
				i++
				class = append(class, '^')
			}
			for i+1 < len(runes) {
				i++
				class = append(class, runes[i])
				if runes[i] == ']' {
					closed = true
					break
				}
			}
			if closed {
				b.WriteString(string(class))
			} else {
				b.WriteString(`\[`)
				b.WriteString(regexp.QuoteMeta(string(class[1:])))
			}
		default:
			b.WriteString(regexp.QuoteMeta(string(runes[i])))
		}
	}
	b.WriteString("$")
	return b.String()
}

type cliPostFilters struct {
	grep *regexp.Regexp
}

func newCLIPostFilters(flags *cliFlags) (*cliPostFilters, error) {
	grep, err := compileGrepFilter(*flags.grepFlag, flags.caseSensitiveFlag)
	if err != nil {
		return nil, err
	}
	// systemd v260.1 parses --exclude-identifier and stores the values,
	// but the file-backed show path never consults them. Keep the option
	// as a parsed no-op for baseline parity.
	return &cliPostFilters{grep: grep}, nil
}

func (f *cliPostFilters) matches(entry *journal.Entry) bool {
	if f == nil {
		return true
	}
	if f.grep != nil {
		matched := false
		for _, value := range entryValues(entry, "MESSAGE") {
			if f.grep.MatchString(string(value)) {
				matched = true
				break
			}
		}
		if !matched {
			return false
		}
	}
	return true
}

func entryValues(entry *journal.Entry, field string) [][]byte {
	if entry == nil {
		return nil
	}
	if len(entry.FieldValues[field]) > 0 {
		return entry.FieldValues[field]
	}
	if value, ok := entry.Fields[field]; ok {
		return [][]byte{value}
	}
	return nil
}

func compileGrepFilter(pattern string, caseSensitive optionalStringFlag) (*regexp.Regexp, error) {
	if pattern == "" {
		return nil, nil
	}
	sensitive := hasUppercase(pattern)
	if caseSensitive.set {
		if caseSensitive.value == "" {
			sensitive = true
		} else {
			parsed, err := parseBoolOption("--case-sensitive", caseSensitive.value)
			if err != nil {
				return nil, err
			}
			sensitive = parsed
		}
	}
	if !sensitive {
		pattern = "(?i)" + pattern
	}
	re, err := regexp.Compile(pattern)
	if err != nil {
		return nil, fmt.Errorf("Bad pattern %q: %w", pattern, err)
	}
	return re, nil
}

func hasUppercase(value string) bool {
	for _, ch := range value {
		if unicode.IsUpper(ch) {
			return true
		}
	}
	return false
}

func parseBoolOption(option, value string) (bool, error) {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "1", "true", "yes", "y", "on":
		return true, nil
	case "", "0", "false", "no", "n", "off":
		return false, nil
	default:
		return false, fmt.Errorf("Bad %s= argument %q", option, value)
	}
}

func parsePriorityFilter(values []string) ([]uint8, error) {
	if len(values) == 0 {
		return nil, nil
	}
	value := values[len(values)-1]
	if strings.Contains(value, "..") {
		parts := strings.SplitN(value, "..", 2)
		from, err := parsePriorityLevel(parts[0])
		if err != nil {
			return nil, err
		}
		to, err := parsePriorityLevel(parts[1])
		if err != nil {
			return nil, err
		}
		if from > to {
			from, to = to, from
		}
		out := make([]uint8, 0, int(to-from)+1)
		for priority := from; priority <= to; priority++ {
			out = append(out, priority)
		}
		return out, nil
	}
	highest, err := parsePriorityLevel(value)
	if err != nil {
		return nil, err
	}
	out := make([]uint8, 0, int(highest)+1)
	for priority := uint8(0); priority <= highest; priority++ {
		out = append(out, priority)
	}
	return out, nil
}

func parsePriorityLevel(value string) (uint8, error) {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "emerg", "panic":
		return 0, nil
	case "alert":
		return 1, nil
	case "crit", "critical":
		return 2, nil
	case "err", "error":
		return 3, nil
	case "warning", "warn":
		return 4, nil
	case "notice":
		return 5, nil
	case "info":
		return 6, nil
	case "debug":
		return 7, nil
	}
	number, err := strconv.Atoi(value)
	if err == nil && number >= 0 && number <= 7 {
		return uint8(number), nil
	}
	return 0, fmt.Errorf("Unknown log level %s", value)
}

var facilityNames = map[string]uint8{
	"kern": 0, "user": 1, "mail": 2, "daemon": 3,
	"auth": 4, "syslog": 5, "lpr": 6, "news": 7,
	"uucp": 8, "cron": 9, "authpriv": 10, "ftp": 11,
	"local0": 16, "local1": 17, "local2": 18, "local3": 19,
	"local4": 20, "local5": 21, "local6": 22, "local7": 23,
}

var facilityHelpNames = []string{
	"kern", "user", "mail", "daemon", "auth", "syslog", "lpr", "news",
	"uucp", "cron", "authpriv", "ftp", "12", "13", "14", "15",
	"local0", "local1", "local2", "local3", "local4", "local5", "local6", "local7",
}

func facilityHelpRequested(values []string) bool {
	for _, value := range values {
		for _, item := range strings.Split(value, ",") {
			if strings.TrimSpace(item) == "help" {
				return true
			}
		}
	}
	return false
}

func printFacilityHelp(stdout io.Writer, quiet bool) {
	if !quiet {
		fmt.Fprintln(stdout, "Available facilities:")
	}
	for _, name := range facilityHelpNames {
		fmt.Fprintln(stdout, name)
	}
}

func parseFacilityFilter(values []string) ([]uint8, error) {
	seen := make(map[uint8]struct{})
	var facilities []uint8
	for _, value := range values {
		for _, item := range strings.Split(value, ",") {
			item = strings.TrimSpace(item)
			if item == "" || item == "help" {
				continue
			}
			facility, err := parseFacility(item)
			if err != nil {
				return nil, err
			}
			if _, ok := seen[facility]; ok {
				continue
			}
			seen[facility] = struct{}{}
			facilities = append(facilities, facility)
		}
	}
	sort.Slice(facilities, func(i, j int) bool { return facilities[i] < facilities[j] })
	return facilities, nil
}

func parseFacility(value string) (uint8, error) {
	if number, err := strconv.Atoi(value); err == nil && number >= 0 && number <= 23 {
		return uint8(number), nil
	}
	if facility, ok := facilityNames[value]; ok {
		return facility, nil
	}
	return 0, fmt.Errorf("Bad --facility= argument %q.", value)
}

func (f *cliFlags) dispatch(j cliJournal, sinceUsec, untilUsec *uint64, stdout io.Writer, postFilters *cliPostFilters, cursorControl cursorControl, outputOptions outputOptions) error {
	if f.linesFlag.set {
		limit, err := parseLinesLimitValue(f.linesFlag.value)
		if err != nil {
			return err
		}
		if limit.set && !limit.all {
			if limit.oldest {
				return showForward(j, limit.count, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
			}
			return showTail(j, limit.count, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
		}
	}

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
		return showForward(j, *f.head, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)

	case *f.tail > 0:
		if *f.reverseFlag {
			return showReverse(j, *f.tail, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
		}
		return showTail(j, *f.tail, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)

	default:
		if *f.pagerEndFlag {
			return showTail(j, 1000, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
		}
		if *f.reverseFlag {
			return showReverse(j, 0, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
		}
		return showForward(j, *f.head, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
	}
}

func (f *cliFlags) effectiveQuiet() bool {
	if *f.quietFlag {
		return true
	}
	switch *f.output {
	case "export", "json", "json-pretty", "json-sse", "json-seq", "cat":
		return true
	default:
		return false
	}
}

func openFilteredJournal(input cliInput, matches []string, flags *cliFlags, outputMode string) (cliJournal, error) {
	j, err := input.openJournal()
	if err != nil {
		return nil, fmt.Errorf("open journal: %w", err)
	}
	ok := false
	defer func() {
		if !ok {
			_ = j.Close()
		}
	}()

	invocationID, useInvocation, err := resolveInvocationFilter(input, flags)
	if err != nil {
		return nil, err
	}
	if useInvocation {
		if err := addInvocationMatches(j, invocationID); err != nil {
			return nil, err
		}
	} else {
		if err := applyBootMatch(j, flags); err != nil {
			return nil, err
		}
		if err := applyCLIMatches(j, flags); err != nil {
			return nil, err
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
	return parseDurationUsecMode(value, false)
}

func parseDurationUsecAllowZero(value string) (uint64, error) {
	return parseDurationUsecMode(value, true)
}

func parseDurationUsecMode(value string, allowZero bool) (uint64, error) {
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
	seen := false
	for _, match := range durationTokenRe.FindAllStringSubmatchIndex(value, -1) {
		if match[0] != pos {
			return 0, fmt.Errorf("failed to parse duration: %s", value)
		}
		seen = true
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
	if pos != len(value) || !seen || (!allowZero && total == 0) {
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

type invocationInfo struct {
	id        string
	firstUsec uint64
	lastUsec  uint64
}

func effectiveInvocationDescriptor(flags *cliFlags) (string, bool) {
	if !flags.invocationSet {
		return "", false
	}
	if *flags.invocationFlag != "" {
		return strings.TrimSpace(*flags.invocationFlag), true
	}
	return "0", true
}

func parseInvocationDescriptor(descriptor string) (string, int, bool, error) {
	if descriptor == "all" {
		return "", 0, false, nil
	}
	id, offset, err := parseBootDescriptor(descriptor)
	if err != nil {
		return "", 0, false, fmt.Errorf("failed to parse invocation descriptor: %s", descriptor)
	}
	return id, offset, true, nil
}

func resolveInvocationFilter(input cliInput, flags *cliFlags) (string, bool, error) {
	descriptor, set := effectiveInvocationDescriptor(flags)
	if !set {
		return "", false, nil
	}
	id, offset, active, err := parseInvocationDescriptor(descriptor)
	if err != nil {
		return "", false, err
	}
	if !active {
		return "", false, nil
	}

	j, err := input.openJournal()
	if err != nil {
		return "", false, fmt.Errorf("open journal: %w", err)
	}
	defer j.Close()

	if err := applyBootMatch(j, flags); err != nil {
		return "", false, err
	}
	if id == "" || offset != 0 {
		if err := applySingleInvocationUnit(j, flags, "-I/--invocation= with an offset"); err != nil {
			return "", false, err
		}
	}

	infos, err := collectInvocations(j)
	if err != nil {
		return "", false, err
	}
	target := -1
	if id != "" {
		for i, info := range infos {
			if info.id == id {
				target = i + offset
				break
			}
		}
	} else if offset > 0 {
		target = offset - 1
	} else {
		target = len(infos) - 1 + offset
	}
	if target < 0 || target >= len(infos) {
		return "", false, fmt.Errorf("No journal entry found for the invocation (%s%+d).", id, offset)
	}
	return infos[target].id, true, nil
}

func applySingleInvocationUnit(j cliJournal, flags *cliFlags, optionName string) error {
	systemUnits, userUnits, err := singleInvocationUnit(j, flags, optionName)
	if err != nil {
		return err
	}
	return addJournalctlUnitMatches(j, systemUnits, userUnits)
}

func singleInvocationUnit(j cliJournal, flags *cliFlags, optionName string) ([]string, []string, error) {
	systemSpecs := flags.unitFlag.Values()
	userSpecs := flags.userUnitFlag.Values()
	count := len(systemSpecs) + len(userSpecs)
	if count == 0 {
		return nil, nil, fmt.Errorf("Using %s requires a unit. Please specify a unit name with -u/--unit=/--user-unit=.", optionName)
	}
	if count > 1 {
		return nil, nil, fmt.Errorf("Using %s with multiple units is not supported.", optionName)
	}
	if len(systemSpecs) == 1 {
		units, err := expandUnitSpecs(j, systemSpecs, systemUnitFieldsFull)
		if err != nil {
			return nil, nil, err
		}
		if len(units) == 0 {
			return nil, nil, fmt.Errorf("No matching unit found for '%s' in journal.", mangleUnitName(systemSpecs[0]))
		}
		if len(units) > 1 {
			return nil, nil, fmt.Errorf("Multiple matching units found for '%s' in journal.", mangleUnitName(systemSpecs[0]))
		}
		return units, nil, nil
	}
	units, err := expandUnitSpecs(j, userSpecs, userUnitFieldsFull)
	if err != nil {
		return nil, nil, err
	}
	if len(units) == 0 {
		return nil, nil, fmt.Errorf("No matching unit found for '%s' in journal.", mangleUnitName(userSpecs[0]))
	}
	if len(units) > 1 {
		return nil, nil, fmt.Errorf("Multiple matching units found for '%s' in journal.", mangleUnitName(userSpecs[0]))
	}
	return nil, units, nil
}

func collectInvocations(j cliJournal) ([]invocationInfo, error) {
	if err := j.SeekHead(); err != nil {
		return nil, err
	}
	byID := make(map[string]*invocationInfo)
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
		id, hasInvocation := entryInvocationID(entry)
		if !hasInvocation {
			continue
		}
		info := byID[id]
		if info == nil {
			byID[id] = &invocationInfo{id: id, firstUsec: entry.Realtime, lastUsec: entry.Realtime}
			continue
		}
		if entry.Realtime < info.firstUsec {
			info.firstUsec = entry.Realtime
		}
		if entry.Realtime > info.lastUsec {
			info.lastUsec = entry.Realtime
		}
	}
	out := make([]invocationInfo, 0, len(byID))
	for _, info := range byID {
		out = append(out, *info)
	}
	sort.Slice(out, func(i, k int) bool {
		if out[i].firstUsec != out[k].firstUsec {
			return out[i].firstUsec < out[k].firstUsec
		}
		return out[i].id < out[k].id
	})
	return out, nil
}

func entryInvocationID(entry *journal.Entry) (string, bool) {
	for _, field := range []string{
		"_SYSTEMD_INVOCATION_ID",
		"OBJECT_SYSTEMD_INVOCATION_ID",
		"INVOCATION_ID",
		"USER_INVOCATION_ID",
	} {
		for _, value := range entryValues(entry, field) {
			id, err := journal.ParseUUID(string(value))
			if err == nil && strings.Trim(id.String(), "0") != "" {
				return id.String(), true
			}
		}
	}
	return "", false
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

func nextMatchingEntries(j cliJournal, sinceUsec, untilUsec *uint64, postFilters *cliPostFilters, cursorSeek *cursorSeek, fn func(*journal.Entry) error) error {
	if cursorSeek != nil {
		entry, ok, err := seekCursorStart(j, cursorSeek, false)
		if err != nil {
			return err
		}
		if !ok {
			return nil
		}
		if entryInTimeRange(entry, sinceUsec, untilUsec) && postFilters.matches(entry) {
			if err := fn(entry); err != nil {
				return err
			}
		}
	} else if sinceUsec != nil {
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
		if entryInTimeRange(entry, sinceUsec, untilUsec) && postFilters.matches(entry) {
			if err := fn(entry); err != nil {
				return err
			}
		}
	}
}

func showForward(j cliJournal, limit int, sinceUsec, untilUsec *uint64, stdout io.Writer, showCursor bool, quiet bool, postFilters *cliPostFilters, cursorControl cursorControl, outputOptions outputOptions) error {
	count := 0
	var lastCursor string
	renderer := newOutputRenderer(outputOptions)
	err := nextMatchingEntries(j, sinceUsec, untilUsec, postFilters, cursorControl.seek, func(entry *journal.Entry) error {
		if limit > 0 && count >= limit {
			return errStopIteration
		}
		out, err := renderer.render(entry)
		if err != nil {
			return err
		}
		fmt.Fprint(stdout, out)
		lastCursor = entry.Cursor
		count++
		return nil
	})
	if errors.Is(err, errStopIteration) {
		err = nil
	}
	if err != nil {
		return err
	}
	printNoEntries(stdout, count, quiet)
	return finishCursorOutput(stdout, showCursor, cursorControl.updateFile, lastCursor)
}

func showTail(j cliJournal, limit int, sinceUsec, untilUsec *uint64, stdout io.Writer, showCursor bool, quiet bool, postFilters *cliPostFilters, cursorControl cursorControl, outputOptions outputOptions) error {
	var entries []*journal.Entry
	if err := nextMatchingEntries(j, sinceUsec, untilUsec, postFilters, cursorControl.seek, func(entry *journal.Entry) error {
		entries = append(entries, entry)
		return nil
	}); err != nil {
		return err
	}
	start := len(entries) - limit
	if start < 0 {
		start = 0
	}
	var lastCursor string
	renderer := newOutputRenderer(outputOptions)
	for _, entry := range entries[start:] {
		out, err := renderer.render(entry)
		if err != nil {
			return err
		}
		fmt.Fprint(stdout, out)
		lastCursor = entry.Cursor
	}
	printNoEntries(stdout, len(entries[start:]), quiet)
	return finishCursorOutput(stdout, showCursor, cursorControl.updateFile, lastCursor)
}

func showReverse(j cliJournal, limit int, sinceUsec, untilUsec *uint64, stdout io.Writer, showCursor bool, quiet bool, postFilters *cliPostFilters, cursorControl cursorControl, outputOptions outputOptions) error {
	count := 0
	var lastCursor string
	renderer := newOutputRenderer(outputOptions)
	err := previousMatchingEntries(j, sinceUsec, untilUsec, postFilters, cursorControl.seek, func(entry *journal.Entry) error {
		if limit > 0 && count >= limit {
			return errStopIteration
		}
		out, err := renderer.render(entry)
		if err != nil {
			return err
		}
		fmt.Fprint(stdout, out)
		lastCursor = entry.Cursor
		count++
		return nil
	})
	if errors.Is(err, errStopIteration) {
		err = nil
	}
	if err != nil {
		return err
	}
	printNoEntries(stdout, count, quiet)
	return finishCursorOutput(stdout, showCursor, cursorControl.updateFile, lastCursor)
}

func printNoEntries(stdout io.Writer, count int, quiet bool) {
	if count == 0 && !quiet {
		fmt.Fprintln(stdout, "-- No entries --")
	}
}

func previousMatchingEntries(j cliJournal, sinceUsec, untilUsec *uint64, postFilters *cliPostFilters, cursorSeek *cursorSeek, fn func(*journal.Entry) error) error {
	if cursorSeek != nil {
		entry, ok, err := seekCursorStart(j, cursorSeek, true)
		if err != nil {
			return err
		}
		if !ok {
			return nil
		}
		if entryInTimeRange(entry, sinceUsec, untilUsec) && postFilters.matches(entry) {
			if err := fn(entry); err != nil {
				return err
			}
		}
	} else if untilUsec != nil {
		if err := j.SeekRealtimeUsec(*untilUsec); err != nil {
			return err
		}
	} else if err := j.SeekTail(); err != nil {
		return err
	}
	for {
		n, err := j.Previous()
		if err != nil {
			return err
		}
		if n == 0 {
			return nil
		}
		entry, err := j.GetEntry()
		if err != nil {
			return err
		}
		if sinceUsec != nil && entry.Realtime < *sinceUsec {
			return nil
		}
		if entryInTimeRange(entry, sinceUsec, untilUsec) && postFilters.matches(entry) {
			if err := fn(entry); err != nil {
				return err
			}
		}
	}
}

func seekCursorStart(j cliJournal, seek *cursorSeek, reverse bool) (*journal.Entry, bool, error) {
	if err := j.SeekCursor(seek.cursor); err != nil {
		return nil, false, fmt.Errorf("seek cursor: %w", err)
	}
	if seek.after {
		match, err := j.TestCursor(seek.cursor)
		if err != nil {
			if errors.Is(err, journal.ErrNoEntry) || errors.Is(err, journal.ErrEndOfEntries) {
				return nil, false, nil
			}
			return nil, false, fmt.Errorf("test cursor: %w", err)
		}
		if match {
			var n int
			var err error
			if reverse {
				n, err = j.Previous()
			} else {
				n, err = j.Next()
			}
			if err != nil {
				return nil, false, err
			}
			if n == 0 {
				return nil, false, nil
			}
		}
	}

	entry, err := j.GetEntry()
	if err != nil {
		if errors.Is(err, journal.ErrNoEntry) || errors.Is(err, journal.ErrEndOfEntries) {
			return nil, false, nil
		}
		return nil, false, err
	}
	return entry, true, nil
}

func finishCursorOutput(stdout io.Writer, showCursor bool, cursorFile, cursor string) error {
	if cursor == "" {
		return nil
	}
	if showCursor {
		if _, err := fmt.Fprintf(stdout, "-- cursor: %s\n", cursor); err != nil {
			return err
		}
	}
	if cursorFile != "" {
		return writeCursorFileAtomic(cursorFile, cursor)
	}
	return nil
}

func writeCursorFileAtomic(path, cursor string) error {
	dir := filepath.Dir(path)
	base := filepath.Base(path)
	tmp, err := os.CreateTemp(dir, "."+base+".tmp.*")
	if err != nil {
		return fmt.Errorf("Failed to write new cursor to %s: %w", path, err)
	}
	tmpName := tmp.Name()
	ok := false
	defer func() {
		if !ok {
			_ = os.Remove(tmpName)
		}
	}()
	if _, err := tmp.WriteString(cursor + "\n"); err != nil {
		_ = tmp.Close()
		return fmt.Errorf("Failed to write new cursor to %s: %w", path, err)
	}
	if err := tmp.Close(); err != nil {
		return fmt.Errorf("Failed to write new cursor to %s: %w", path, err)
	}
	if err := os.Rename(tmpName, path); err != nil {
		return fmt.Errorf("Failed to write new cursor to %s: %w", path, err)
	}
	ok = true
	return nil
}

type followEntry struct {
	cursor string
	output string
}

func scanFollowSnapshot(input cliInput, matches []string, flags *cliFlags, outputMode string, sinceUsec, untilUsec *uint64, postFilters *cliPostFilters, cursorControl cursorControl, outputOptions outputOptions) []followEntry {
	j, err := openFilteredJournal(input, matches, flags, outputMode)
	if err != nil {
		return nil
	}
	defer j.Close()
	var out []followEntry
	renderer := newOutputRenderer(outputOptions)
	_ = nextMatchingEntries(j, sinceUsec, untilUsec, postFilters, cursorControl.seek, func(entry *journal.Entry) error {
		if entry.Cursor == "" {
			return nil
		}
		processed, err := renderer.render(entry)
		if err != nil {
			return nil
		}
		out = append(out, followEntry{cursor: entry.Cursor, output: processed})
		return nil
	})
	return out
}

func runFollow(input cliInput, matches []string, flags *cliFlags, outputMode string, sinceUsec, untilUsec *uint64, tail int, noTail bool, stdout io.Writer, postFilters *cliPostFilters, cursorControl cursorControl, outputOptions outputOptions) error {
	seen := make(map[string]struct{})
	initial := scanFollowSnapshot(input, matches, flags, outputMode, sinceUsec, untilUsec, postFilters, cursorControl, outputOptions)
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
		for _, entry := range scanFollowSnapshot(input, matches, flags, outputMode, sinceUsec, untilUsec, postFilters, cursorControl, outputOptions) {
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

func runVerify(input cliInput, verifyKey string, hasVerifyKey bool, stdout, stderr io.Writer) error {
	if err := validateVerificationKeyOption(verifyKey, hasVerifyKey, stderr); err != nil {
		return err
	}

	files, directoryInput, err := input.journalFiles("verify")
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

func printNewID128(stdout io.Writer) error {
	var id [16]byte
	if _, err := rand.Read(id[:]); err != nil {
		return fmt.Errorf("generate ID128: %w", err)
	}
	id[6] = (id[6] & 0x0f) | 0x40
	id[8] = (id[8] & 0x3f) | 0x80

	simple := hex.EncodeToString(id[:])
	uuidText := fmt.Sprintf("%s-%s-%s-%s-%s", simple[0:8], simple[8:12], simple[12:16], simple[16:20], simple[20:32])
	macroBytes := make([]string, 0, len(id))
	for _, b := range id {
		macroBytes = append(macroBytes, fmt.Sprintf("%02x", b))
	}

	fmt.Fprintln(stdout, "As string:")
	fmt.Fprintln(stdout, simple)
	fmt.Fprintln(stdout)
	fmt.Fprintln(stdout, "As UUID:")
	fmt.Fprintln(stdout, uuidText)
	fmt.Fprintln(stdout)
	fmt.Fprintln(stdout, "As systemd-id128(1) macro:")
	fmt.Fprintf(stdout, "#define XYZ SD_ID128_MAKE(%s)\n", strings.Join(macroBytes, ","))
	fmt.Fprintln(stdout)
	fmt.Fprintln(stdout, "As Python constant:")
	fmt.Fprintln(stdout, ">>> import uuid")
	fmt.Fprintf(stdout, ">>> XYZ = uuid.UUID('%s')\n", simple)
	return nil
}

func runDiskUsage(input cliInput, stdout io.Writer) error {
	files, _, err := input.journalFiles("disk usage")
	if err != nil {
		return err
	}
	var bytes uint64
	for _, path := range files {
		allocated, err := allocatedFileBytes(path)
		if err != nil {
			return err
		}
		if ^uint64(0)-bytes < allocated {
			bytes = ^uint64(0)
		} else {
			bytes += allocated
		}
	}
	fmt.Fprintf(stdout, "Archived and active journals take up %s in the file system.\n", formatJournalBytes(bytes))
	return nil
}

type vacuumOptions struct {
	maxUse           uint64
	maxFiles         uint64
	maxRetentionUsec uint64
}

type vacuumCandidate struct {
	path      string
	name      string
	usage     uint64
	seqnumID  string
	seqnum    uint64
	realtime  uint64
	haveSeqno bool
}

func hasVacuumFlags(f *cliFlags) bool {
	return *f.vacuumSizeFlag != "" || *f.vacuumFilesFlag != "" || *f.vacuumTimeFlag != ""
}

func runVacuum(inputPath string, flags *cliFlags, stderr io.Writer) error {
	opts, err := parseVacuumOptions(flags)
	if err != nil {
		return err
	}
	if opts.maxUse == 0 && opts.maxFiles == 0 && opts.maxRetentionUsec == 0 {
		return nil
	}
	info, err := os.Stat(inputPath)
	if err != nil {
		return fmt.Errorf("vacuum: %s: %w", inputPath, err)
	}
	if !info.IsDir() {
		return fmt.Errorf("vacuum: %s is not a directory", inputPath)
	}
	return vacuumDirectory(inputPath, opts, *flags.quietFlag, stderr)
}

func parseVacuumOptions(flags *cliFlags) (vacuumOptions, error) {
	var opts vacuumOptions
	if *flags.vacuumSizeFlag != "" {
		value, err := parseVacuumSize(*flags.vacuumSizeFlag)
		if err != nil {
			return vacuumOptions{}, err
		}
		opts.maxUse = value
	}
	if *flags.vacuumFilesFlag != "" {
		value, err := strconv.ParseUint(strings.TrimSpace(*flags.vacuumFilesFlag), 10, 64)
		if err != nil {
			return vacuumOptions{}, fmt.Errorf("failed to parse --vacuum-files value: %s", *flags.vacuumFilesFlag)
		}
		opts.maxFiles = value
	}
	if *flags.vacuumTimeFlag != "" {
		trimmed := strings.TrimSpace(*flags.vacuumTimeFlag)
		value, err := parseDurationUsecAllowZero(trimmed)
		if err != nil {
			return vacuumOptions{}, fmt.Errorf("failed to parse --vacuum-time value: %s", *flags.vacuumTimeFlag)
		}
		opts.maxRetentionUsec = value
	}
	return opts, nil
}

func parseVacuumSize(value string) (uint64, error) {
	match := sizeTokenRe.FindStringSubmatch(value)
	if match == nil {
		return 0, fmt.Errorf("failed to parse --vacuum-size value: %s", value)
	}
	number, err := strconv.ParseFloat(match[1], 64)
	if err != nil {
		return 0, err
	}
	unit := strings.ToLower(match[2])
	multipliers := map[string]uint64{
		"": 1, "b": 1, "byte": 1, "bytes": 1,
		"k": 1024, "kb": 1024, "kib": 1024,
		"m": 1024 * 1024, "mb": 1024 * 1024, "mib": 1024 * 1024,
		"g": 1024 * 1024 * 1024, "gb": 1024 * 1024 * 1024, "gib": 1024 * 1024 * 1024,
		"t": 1024 * 1024 * 1024 * 1024, "tb": 1024 * 1024 * 1024 * 1024, "tib": 1024 * 1024 * 1024 * 1024,
		"p": 1024 * 1024 * 1024 * 1024 * 1024, "pb": 1024 * 1024 * 1024 * 1024 * 1024, "pib": 1024 * 1024 * 1024 * 1024 * 1024,
		"e": 1024 * 1024 * 1024 * 1024 * 1024 * 1024, "eb": 1024 * 1024 * 1024 * 1024 * 1024 * 1024, "eib": 1024 * 1024 * 1024 * 1024 * 1024 * 1024,
	}
	multiplier, ok := multipliers[unit]
	if !ok {
		return 0, fmt.Errorf("failed to parse --vacuum-size value: %s", value)
	}
	if number < 0 || number > float64(^uint64(0))/float64(multiplier) {
		return 0, fmt.Errorf("failed to parse --vacuum-size value: %s", value)
	}
	return uint64(number * float64(multiplier)), nil
}

func vacuumDirectory(dir string, opts vacuumOptions, quiet bool, stderr io.Writer) error {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return fmt.Errorf("vacuum: read directory %s: %w", dir, err)
	}

	var candidates []vacuumCandidate
	var activeFiles, sum, freed uint64
	for _, entry := range entries {
		name := entry.Name()
		path := filepath.Join(dir, name)
		info, err := os.Lstat(path)
		if err != nil || !info.Mode().IsRegular() {
			continue
		}
		usage := allocatedBytes(info)

		candidate, protected := parseVacuumCandidate(path, name, usage)
		if protected {
			activeFiles++
			sum = saturatingAdd(sum, usage)
			continue
		}
		if candidate == nil {
			continue
		}

		empty, err := vacuumJournalFileEmpty(path, info)
		if err != nil {
			continue
		}
		if empty {
			if err := os.Remove(path); err == nil {
				freed = saturatingAdd(freed, usage)
				if !quiet {
					fmt.Fprintf(stderr, "Deleted empty archived journal %s/%s (%s).\n", dir, name, formatJournalBytes(usage))
				}
			} else if !errors.Is(err, os.ErrNotExist) && !quiet {
				fmt.Fprintf(stderr, "Failed to delete empty archived journal %s/%s: %v\n", dir, name, err)
			}
			continue
		}

		patchVacuumRealtime(candidate, info)
		candidates = append(candidates, *candidate)
		sum = saturatingAdd(sum, usage)
	}

	sort.Slice(candidates, func(i, j int) bool {
		return vacuumCandidateLess(candidates[i], candidates[j])
	})

	retentionLimit := uint64(0)
	if opts.maxRetentionUsec > 0 {
		now := currentRealtimeUsec()
		if now > opts.maxRetentionUsec {
			retentionLimit = now - opts.maxRetentionUsec
		}
	}

	for i, candidate := range candidates {
		left := activeFiles + uint64(len(candidates)-i)
		if (opts.maxRetentionUsec == 0 || candidate.realtime >= retentionLimit) &&
			(opts.maxUse == 0 || sum <= opts.maxUse) &&
			(opts.maxFiles == 0 || left <= opts.maxFiles) {
			break
		}

		if err := os.Remove(candidate.path); err == nil {
			freed = saturatingAdd(freed, candidate.usage)
			if candidate.usage < sum {
				sum -= candidate.usage
			} else {
				sum = 0
			}
			if !quiet {
				fmt.Fprintf(stderr, "Deleted archived journal %s/%s (%s).\n", dir, candidate.name, formatJournalBytes(candidate.usage))
			}
		} else if !errors.Is(err, os.ErrNotExist) && !quiet {
			fmt.Fprintf(stderr, "Failed to delete archived journal %s/%s: %v\n", dir, candidate.name, err)
		}
	}

	if !quiet {
		fmt.Fprintf(stderr, "Vacuuming done, freed %s of archived journals from %s.\n", formatJournalBytes(freed), dir)
	}
	return nil
}

func parseVacuumCandidate(path, name string, usage uint64) (*vacuumCandidate, bool) {
	if strings.HasSuffix(name, ".journal") {
		match := archivedJournalNameRe.FindStringSubmatch(name)
		if match == nil {
			return nil, true
		}
		seqnum, err1 := strconv.ParseUint(match[2], 16, 64)
		realtime, err2 := strconv.ParseUint(match[3], 16, 64)
		if err1 != nil || err2 != nil {
			return nil, true
		}
		return &vacuumCandidate{
			path:      path,
			name:      name,
			usage:     usage,
			seqnumID:  strings.ToLower(match[1]),
			seqnum:    seqnum,
			realtime:  realtime,
			haveSeqno: true,
		}, false
	}
	if strings.HasSuffix(name, ".journal~") {
		match := corruptJournalNameRe.FindStringSubmatch(name)
		if match == nil {
			return nil, true
		}
		realtime, err := strconv.ParseUint(match[1], 16, 64)
		if err != nil {
			return nil, true
		}
		return &vacuumCandidate{
			path:     path,
			name:     name,
			usage:    usage,
			realtime: realtime,
		}, false
	}
	return nil, false
}

func vacuumCandidateLess(a, b vacuumCandidate) bool {
	if a.haveSeqno && b.haveSeqno && a.seqnumID == b.seqnumID {
		return a.seqnum < b.seqnum
	}
	if a.realtime != b.realtime {
		return a.realtime < b.realtime
	}
	if a.haveSeqno && b.haveSeqno && a.seqnumID != b.seqnumID {
		return a.seqnumID < b.seqnumID
	}
	return a.name < b.name
}

func vacuumJournalFileEmpty(path string, info os.FileInfo) (bool, error) {
	if info.Size() < journalHeaderSize {
		return true, nil
	}
	file, err := os.Open(path) // nosec G304 - caller explicitly supplied the journal directory.
	if err != nil {
		return false, err
	}
	defer file.Close()
	var buf [8]byte
	if _, err := file.ReadAt(buf[:], journalHeaderNEntriesOffset); err != nil {
		return false, err
	}
	return binary.LittleEndian.Uint64(buf[:]) == 0, nil
}

func patchVacuumRealtime(candidate *vacuumCandidate, info os.FileInfo) {
	usec := timeToUsec(info.ModTime())
	if usec > 0 && usec < candidate.realtime {
		candidate.realtime = usec
	}
}

func currentRealtimeUsec() uint64 {
	return timeToUsec(time.Now())
}

func timeToUsec(t time.Time) uint64 {
	if t.IsZero() || t.Unix() < 0 {
		return 0
	}
	return uint64(t.Unix())*1_000_000 + uint64(t.Nanosecond()/1_000)
}

func saturatingAdd(a, b uint64) uint64 {
	if ^uint64(0)-a < b {
		return ^uint64(0)
	}
	return a + b
}

func runHeader(input cliInput, stdout io.Writer) error {
	files, _, err := input.journalFiles("header")
	if err != nil {
		return err
	}
	for i, path := range files {
		reader, err := journal.OpenFile(path)
		if err != nil {
			return fmt.Errorf("header: %s: %w", path, err)
		}
		header := reader.Header()
		closeErr := reader.Close()
		if i > 0 {
			fmt.Fprintln(stdout)
		}
		usage, err := allocatedFileBytes(path)
		if err != nil {
			return err
		}
		printHeader(stdout, path, header, usage)
		if closeErr != nil {
			return closeErr
		}
	}
	return nil
}

func printHeader(stdout io.Writer, path string, h interface {
	FileID() journal.UUID
	MachineID() journal.UUID
	TailEntryBootID() journal.UUID
	SeqnumID() journal.UUID
	State() uint8
	CompatibleFlags() uint32
	IncompatibleFlags() uint32
	HeaderSize() uint64
	ArenaSize() uint64
	DataHashTableSize() uint64
	FieldHashTableSize() uint64
	HeadEntrySeqnum() uint64
	TailEntrySeqnum() uint64
	HeadEntryRealtime() uint64
	TailEntryRealtime() uint64
	TailEntryMonotonic() uint64
	NObjects() uint64
	NEntries() uint64
	NData() uint64
	NFields() uint64
	NTags() uint64
	NEntryArrays() uint64
	DataHashChainDepth() uint64
	FieldHashChainDepth() uint64
}, diskUsage uint64) {
	dataBuckets := h.DataHashTableSize() / hashItemSizeBytes
	fieldBuckets := h.FieldHashTableSize() / hashItemSizeBytes
	fmt.Fprintf(stdout, "File path: %s\n", path)
	fmt.Fprintf(stdout, "File ID: %s\n", h.FileID())
	fmt.Fprintf(stdout, "Machine ID: %s\n", h.MachineID())
	fmt.Fprintf(stdout, "Boot ID: %s\n", h.TailEntryBootID())
	fmt.Fprintf(stdout, "Sequential number ID: %s\n", h.SeqnumID())
	fmt.Fprintf(stdout, "State: %s\n", headerStateName(h.State()))
	fmt.Fprintf(stdout, "Compatible flags:%s\n", compatibleFlagText(h.CompatibleFlags()))
	fmt.Fprintf(stdout, "Incompatible flags:%s\n", incompatibleFlagText(h.IncompatibleFlags()))
	fmt.Fprintf(stdout, "Header size: %d\n", h.HeaderSize())
	fmt.Fprintf(stdout, "Arena size: %d\n", h.ArenaSize())
	fmt.Fprintf(stdout, "Data hash table size: %d\n", dataBuckets)
	fmt.Fprintf(stdout, "Field hash table size: %d\n", fieldBuckets)
	fmt.Fprintf(stdout, "Rotate suggested: %s\n", yesNo(headerRotateSuggested(h, dataBuckets, fieldBuckets)))
	fmt.Fprintf(stdout, "Head sequential number: %d (%x)\n", h.HeadEntrySeqnum(), h.HeadEntrySeqnum())
	fmt.Fprintf(stdout, "Tail sequential number: %d (%x)\n", h.TailEntrySeqnum(), h.TailEntrySeqnum())
	fmt.Fprintf(stdout, "Head realtime timestamp: %s (%x)\n", formatHeaderTimestamp(h.HeadEntryRealtime()), h.HeadEntryRealtime())
	fmt.Fprintf(stdout, "Tail realtime timestamp: %s (%x)\n", formatHeaderTimestamp(h.TailEntryRealtime()), h.TailEntryRealtime())
	fmt.Fprintf(stdout, "Tail monotonic timestamp: %s (%x)\n", formatHeaderTimespan(h.TailEntryMonotonic()), h.TailEntryMonotonic())
	fmt.Fprintf(stdout, "Objects: %d\n", h.NObjects())
	fmt.Fprintf(stdout, "Entry objects: %d\n", h.NEntries())
	if headerContains(h.HeaderSize(), 216) {
		fmt.Fprintf(stdout, "Data objects: %d\n", h.NData())
		fmt.Fprintf(stdout, "Data hash table fill: %.1f%%\n", fillPercent(h.NData(), dataBuckets))
	}
	if headerContains(h.HeaderSize(), 224) {
		fmt.Fprintf(stdout, "Field objects: %d\n", h.NFields())
		fmt.Fprintf(stdout, "Field hash table fill: %.1f%%\n", fillPercent(h.NFields(), fieldBuckets))
	}
	if headerContains(h.HeaderSize(), 232) {
		fmt.Fprintf(stdout, "Tag objects: %d\n", h.NTags())
	}
	if headerContains(h.HeaderSize(), 240) {
		fmt.Fprintf(stdout, "Entry array objects: %d\n", h.NEntryArrays())
	}
	if headerContains(h.HeaderSize(), 256) {
		fmt.Fprintf(stdout, "Deepest field hash chain: %d\n", h.FieldHashChainDepth())
	}
	if headerContains(h.HeaderSize(), 248) {
		fmt.Fprintf(stdout, "Deepest data hash chain: %d\n", h.DataHashChainDepth())
	}
	fmt.Fprintf(stdout, "Disk usage: %s\n", formatJournalBytes(diskUsage))
}

func runListInvocations(input cliInput, flags *cliFlags, stdout io.Writer) error {
	j, err := input.openJournal()
	if err != nil {
		return fmt.Errorf("open journal: %w", err)
	}
	defer j.Close()
	if err := applyBootMatch(j, flags); err != nil {
		return err
	}
	if err := applySingleInvocationUnit(j, flags, "--list-invocations"); err != nil {
		return err
	}
	infos, err := collectInvocations(j)
	if err != nil {
		return err
	}
	if len(infos) == 0 {
		return errors.New("No invocation ID found.")
	}
	display, firstIndex, err := selectInvocationRows(infos, flags)
	if err != nil {
		return err
	}
	if !*flags.quietFlag {
		fmt.Fprintln(stdout, "IDX INVOCATION ID                    FIRST ENTRY                 LAST ENTRY")
	}
	idxWidth := 1
	if *flags.quietFlag {
		for i := range display {
			idxText := fmt.Sprintf("%d", firstIndex+i)
			if len(idxText) > idxWidth {
				idxWidth = len(idxText)
			}
		}
	}
	for i, info := range display {
		if *flags.quietFlag {
			fmt.Fprintf(
				stdout,
				"%*d %s %s %s\n",
				idxWidth,
				firstIndex+i,
				info.id,
				formatHeaderTimestamp(info.firstUsec),
				formatHeaderTimestamp(info.lastUsec),
			)
		} else {
			fmt.Fprintf(
				stdout,
				"%3d %-32s %s %s\n",
				firstIndex+i,
				info.id,
				formatHeaderTimestamp(info.firstUsec),
				formatHeaderTimestamp(info.lastUsec),
			)
		}
	}
	return nil
}

func headerStateName(state uint8) string {
	switch state {
	case 0:
		return "OFFLINE"
	case 1:
		return "ONLINE"
	case 2:
		return "ARCHIVED"
	default:
		return "UNKNOWN"
	}
}

func compatibleFlagText(flags uint32) string {
	var parts []string
	if flags&compatibleSealed != 0 {
		parts = append(parts, "SEALED")
	}
	if flags&compatibleSealedContinuous != 0 {
		parts = append(parts, "SEALED_CONTINUOUS")
	}
	if flags&compatibleTailEntryBootID != 0 {
		parts = append(parts, "TAIL_ENTRY_BOOT_ID")
	}
	if flags&^(compatibleSealed|compatibleSealedContinuous|compatibleTailEntryBootID) != 0 {
		parts = append(parts, "???")
	}
	if len(parts) == 0 {
		return ""
	}
	return " " + strings.Join(parts, " ")
}

func incompatibleFlagText(flags uint32) string {
	var parts []string
	if flags&incompatibleCompressedXZ != 0 {
		parts = append(parts, "COMPRESSED-XZ")
	}
	if flags&incompatibleCompressedLZ4 != 0 {
		parts = append(parts, "COMPRESSED-LZ4")
	}
	if flags&incompatibleCompressedZSTD != 0 {
		parts = append(parts, "COMPRESSED-ZSTD")
	}
	if flags&incompatibleKeyedHash != 0 {
		parts = append(parts, "KEYED-HASH")
	}
	if flags&incompatibleCompact != 0 {
		parts = append(parts, "COMPACT")
	}
	if flags&^(incompatibleCompressedXZ|incompatibleCompressedLZ4|incompatibleCompressedZSTD|incompatibleKeyedHash|incompatibleCompact) != 0 {
		parts = append(parts, "???")
	}
	if len(parts) == 0 {
		return ""
	}
	return " " + strings.Join(parts, " ")
}

func headerContains(headerSize uint64, end int) bool {
	return headerSize >= uint64(end)
}

func fillPercent(count, buckets uint64) float64 {
	if buckets == 0 {
		return 0
	}
	return 100 * float64(count) / float64(buckets)
}

func headerRotateSuggested(h interface {
	HeaderSize() uint64
	NData() uint64
	NFields() uint64
	DataHashChainDepth() uint64
	FieldHashChainDepth() uint64
}, dataBuckets, fieldBuckets uint64) bool {
	if h.HeaderSize() < journalHeaderSize {
		return true
	}
	if dataBuckets > 0 && h.NData()*4 > dataBuckets*3 {
		return true
	}
	if fieldBuckets > 0 && h.NFields()*4 > fieldBuckets*3 {
		return true
	}
	if h.DataHashChainDepth() > headerChainDepthMax || h.FieldHashChainDepth() > headerChainDepthMax {
		return true
	}
	return h.NData() > 0 && h.NFields() == 0
}

func yesNo(value bool) string {
	if value {
		return "yes"
	}
	return "no"
}

func formatHeaderTimestamp(usec uint64) string {
	if usec == 0 {
		return "n/a"
	}
	return time.Unix(0, int64(usec)*1000).Local().Format("Mon 2006-01-02 15:04:05 MST")
}

func formatHeaderTimespan(usec uint64) string {
	if usec < 1000 {
		return fmt.Sprintf("%dus", usec)
	}
	if usec < 1_000_000 {
		return fmt.Sprintf("%dms", usec/1000)
	}
	if usec < 60_000_000 {
		return fmt.Sprintf("%ds", usec/1_000_000)
	}
	if usec < 3_600_000_000 {
		return fmt.Sprintf("%dmin", usec/60_000_000)
	}
	if usec < 86_400_000_000 {
		return fmt.Sprintf("%dh", usec/3_600_000_000)
	}
	return fmt.Sprintf("%dd", usec/86_400_000_000)
}

func selectInvocationRows(infos []invocationInfo, flags *cliFlags) ([]invocationInfo, int, error) {
	if !flags.linesFlag.set {
		return infos, 1 - len(infos), nil
	}
	limit, err := parseLinesLimitValue(flags.linesFlag.value)
	if err != nil {
		return nil, 0, err
	}
	if limit.all || limit.count >= len(infos) {
		if limit.oldest {
			return infos, 1, nil
		}
		return infos, 1 - len(infos), nil
	}
	if limit.count <= 0 {
		return nil, 0, nil
	}
	if limit.oldest {
		return infos[:limit.count], 1, nil
	}
	out := infos[len(infos)-limit.count:]
	return out, 1 - len(out), nil
}

func allocatedFileBytes(path string) (uint64, error) {
	info, err := os.Stat(path)
	if err != nil {
		return 0, fmt.Errorf("disk usage: %s: %w", path, err)
	}
	return allocatedBytes(info), nil
}

func formatJournalBytes(bytes uint64) string {
	type unit struct {
		suffix string
		factor uint64
	}
	units := []unit{
		{"E", 1024 * 1024 * 1024 * 1024 * 1024 * 1024},
		{"P", 1024 * 1024 * 1024 * 1024 * 1024},
		{"T", 1024 * 1024 * 1024 * 1024},
		{"G", 1024 * 1024 * 1024},
		{"M", 1024 * 1024},
		{"K", 1024},
	}
	for i, unit := range units {
		if bytes >= unit.factor {
			var remainder uint64
			if i != len(units)-1 {
				lowerFactor := units[i+1].factor
				remainder = (bytes / lowerFactor * 10 / 1024) % 10
			} else {
				remainder = (bytes * 10 / unit.factor) % 10
			}
			if remainder > 0 {
				return fmt.Sprintf("%d.%d%s", bytes/unit.factor, remainder, unit.suffix)
			}
			return fmt.Sprintf("%d%s", bytes/unit.factor, unit.suffix)
		}
	}
	return fmt.Sprintf("%dB", bytes)
}

func validateVerificationKeyOption(verifyKey string, hasVerifyKey bool, stderr io.Writer) error {
	if !hasVerifyKey || validVerificationKey(verifyKey) {
		return nil
	}
	fmt.Fprintln(stderr, "Failed to parse seed.")
	return errors.New("failed to parse seed")
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
