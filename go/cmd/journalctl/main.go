//go:build cgo

package main

import (
	"bytes"
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
	"sync"
	"time"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

var errSilentNoData = errors.New("no matching entries")

func noDataError(quiet bool, message string) error {
	if quiet {
		return errSilentNoData
	}
	return errors.New(message)
}

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

	archivedJournalNameRe = sync.OnceValue(func() *regexp.Regexp {
		return regexp.MustCompile(`^.+@([0-9A-Fa-f]{32})-([0-9A-Fa-f]{16})-([0-9A-Fa-f]{16})\.journal$`)
	})
	corruptJournalNameRe = sync.OnceValue(func() *regexp.Regexp { return regexp.MustCompile(`^.+@([0-9A-Fa-f]{16})-([0-9A-Fa-f]{16})\.journal~$`) })
	unitSuffixes         = []string{
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

const officialOptionSurfaceHelp = `Official systemd v262 option surface:
  --system --user --machine --merge --directory --file --root --image --image-policy --namespace
  --since --until --cursor --after-cursor --cursor-file --boot --this-boot --unit --user-unit --invocation
  --identifier --exclude-identifier --priority --facility --grep --case-sensitive --dmesg
  --output --output-fields --lines --reverse --show-cursor --utc --catalog --no-hostname --no-full --full
  --all --follow --no-tail --truncate-newline --quiet --synchronize-on-exit --no-pager --pager-end
  --verify-key --interval --force --setup-keys
  --help --version --new-id128 --fields --field --list-boots --list-invocations --list-namespaces
  --disk-usage --vacuum-size --vacuum-files --vacuum-time --verify --sync --relinquish-var
  --smart-relinquish-var --flush --rotate --header --list-catalog --dump-catalog --update-catalog
`

var invocationIDFields = []string{
	"_SYSTEMD_INVOCATION_ID",
	"OBJECT_SYSTEMD_INVOCATION_ID",
	"INVOCATION_ID",
	"USER_INVOCATION_ID",
}

type cliJournal interface {
	Close() error
	AddMatch([]byte)
	AddDisjunction() error
	AddConjunction() error
	FlushMatches()
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
	diagnostics          io.Writer
	unitsReady           bool
	units                unitSelection
	unitsError           error
	unitMatchesReady     bool
	unitMatches          unitSelection
	unitMatchesError     error
	invocationReady      bool
	invocationID         string
	invocationActive     bool
	invocationError      error
	action               cliAction
	parsed               bool
	seen                 map[string]bool
	sinceUsec, untilUsec *uint64
	fullWidth            bool
	timestamps           timestampParser
	file                 multiStringFlag
	directory            *string
	output               *string
	listBoots            *bool
	noTail               *bool
	follow               *bool
	boot                 optionalStringFlag
	fields               *bool
	field                *string
	head                 *int
	tail                 *int
	since                *string
	until                *string
	sync                 *bool
	flush                *bool
	rotate               *bool
	relinquish           *bool
	verify               *bool
	verifyOnly           *bool
	verifyKey            *string

	// Parser-recognized v262 options. Each long option is registered
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
	grepSet               bool
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
		if !errors.Is(err, errSilentNoData) {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		}
		os.Exit(1)
	}
}

func run(args []string, stdin io.Reader, stdout, stderr io.Writer) error {
	fs, flags := newCLIFlagSet(stderr)
	operands, terminal, err := parseCLIOptions(args, fs, flags, stdout)
	if err != nil || terminal {
		return err
	}
	if *flags.noTail {
		flags.linesFlag = optionalStringFlag{set: true, value: "all"}
	}
	if err := flags.validateParserInteractions(); err != nil {
		return err
	}

	hasVerifyKey := flags.seen["verify-key"]
	fieldWasSet := flags.action == "field"
	if err := flags.validateActionArguments(operands, hasVerifyKey, fieldWasSet); err != nil {
		return err
	}
	boot := effectiveBootFlag(flags)
	if (boot.set && boot.value != "all" || flags.action == "list-boots") && *flags.mergeFlag {
		return errors.New("Using --boot or --list-boots with --merge is not supported.")
	}
	postFilters, err := newCLIPostFilters(flags)
	if err != nil {
		return err
	}
	defer postFilters.grep.Close()
	if err := flags.validatePostAction(); err != nil {
		return err
	}
	if *flags.newID128Flag {
		return printNewID128(stdout)
	}
	input, err := flags.input()
	if err != nil {
		return err
	}
	if err := validatePathMatchArguments(operands); err != nil {
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
	if flags.action == "verify" || flags.action == "verify-only" {
		return runVerify(input, *flags.verifyKey, hasVerifyKey, flags.effectiveQuiet(), stdout, stderr)
	}
	if *flags.listBoots {
		return runListBoots(input, flags, stdout)
	}

	sinceUsec, untilUsec, err := flags.timeBounds()
	if err != nil {
		return err
	}
	cursorControl, err := newCursorControl(flags)
	if err != nil {
		return err
	}

	outputOptions := newOutputOptions(flags, *flags.outputFieldsFlag != "", flags.fullWidth)
	postFilters.captureHighlight = (outputOptions.colors.level != 0 || outputOptions.urlify) && (outputOptions.mode == "cat" || outputSkipsMissingMessage(outputOptions.mode))

	if *flags.follow && flags.action == actionShow {
		tail, all := 0, true
		if !flags.seen["since"] {
			tail, all = 10, false
		} else if *flags.pagerEndFlag {
			tail, all = 1000, false
		}
		if flagWasSet(fs, "tail") {
			tail, all = *flags.tail, false
		}
		if flags.linesFlag.set {
			limit, err := parseLinesLimitValue(flags.linesFlag.value)
			if err != nil {
				return err
			}
			if limit.all {
				tail, all = 0, true
			} else {
				tail, all = limit.count, false
			}
		}
		all = all || *flags.noTail
		return runFollow(input, operands, flags, *flags.output, sinceUsec, untilUsec, tail, all, stdout, postFilters, cursorControl, outputOptions)
	}

	j, err := openFilteredJournal(input, operands, flags, *flags.output, optionalStringFlag{})
	if err != nil {
		return err
	}
	defer j.Close()

	err = flags.dispatch(j, sinceUsec, untilUsec, stdout, postFilters, cursorControl, outputOptions)
	if err == nil && flags.action == actionShow && flags.grepSet && !postFilters.shown {
		return errSilentNoData
	}
	return err
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
		return linesLimit{}, errors.New("Failed to parse --lines=''.")
	}
	if value == "all" {
		return linesLimit{set: true, all: true}, nil
	}
	oldest := strings.HasPrefix(value, "+")
	stripped := strings.TrimPrefix(value, "+")
	n, err := parseNativeInt32(stripped)
	if err != nil || n < 0 || uint64(n) > 2147483647 {
		return linesLimit{}, fmt.Errorf("Failed to parse --lines='%s'.", value)
	}
	return linesLimit{set: true, oldest: oldest, count: n}, nil
}

type cursorSeek struct {
	cursor string
	after  bool
}

type cursorControl struct {
	seek       *cursorSeek
	updateFile *string
}

func newCursorControl(f *cliFlags) (cursorControl, error) {
	if *f.cursorFlag != "" || f.seen["cursor"] {
		return cursorControl{seek: &cursorSeek{cursor: *f.cursorFlag}}, nil
	}
	if *f.afterCursorFlag != "" || f.seen["after-cursor"] {
		return cursorControl{seek: &cursorSeek{cursor: *f.afterCursorFlag, after: true}}, nil
	}
	if *f.cursorFileFlag == "" && !f.seen["cursor-file"] {
		return cursorControl{}, nil
	}

	content, err := os.ReadFile(*f.cursorFileFlag) // nosec G304 - caller explicitly supplies --cursor-file path.
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return cursorControl{updateFile: f.cursorFileFlag}, nil
		}
		return cursorControl{}, fmt.Errorf("Failed to read cursor file %s: %w", *f.cursorFileFlag, err)
	}
	cursor := strings.SplitN(string(content), "\n", 2)[0]
	cursor = strings.TrimSuffix(cursor, "\r")
	control := cursorControl{updateFile: f.cursorFileFlag}
	if cursor != "" {
		control.seek = &cursorSeek{cursor: cursor, after: true}
	}
	return control, nil
}

func newCLIFlagSet(stderr io.Writer) (*flag.FlagSet, *cliFlags) {
	fs := flag.NewFlagSet("journalctl", flag.ContinueOnError)
	fs.SetOutput(stderr)
	flags := &cliFlags{
		diagnostics: stderr,
		directory:   fs.String("directory", "", "journal directory"),
		output:      fs.String("output", "short", "output mode: short, short-full, short-iso, short-iso-precise, short-precise, short-monotonic, short-delta, short-unix, verbose, export, json, json-pretty, json-sse, json-seq, cat, with-unit"),
		listBoots:   fs.Bool("list-boots", false, "list boots"),
		noTail:      fs.Bool("no-tail", false, "show all entries, start from the beginning"),
		follow:      fs.Bool("follow", false, "follow appended entries"),
		fields:      fs.Bool("fields", false, "show field names"),
		field:       fs.String("field", "", "show values for a field"),
		head:        fs.Int("head", 0, "show first N entries"),
		tail:        fs.Int("tail", 0, "show last N entries"),
		since:       fs.String("since", "", "show entries since timestamp"),
		until:       fs.String("until", "", "show entries until timestamp"),
		sync:        fs.Bool("sync", false, "sync journal (portable mode does not support this)"),
		flush:       fs.Bool("flush", false, "flush journal (portable mode does not support this)"),
		rotate:      fs.Bool("rotate", false, "rotate journal (portable mode does not support this)"),
		relinquish:  fs.Bool("relinquish-var", false, "relinquish var (portable mode does not support this)"),
		verify:      fs.Bool("verify", false, "verify journal file"),
		verifyOnly:  fs.Bool("verify-only", false, "verify only"),
		verifyKey:   fs.String("verify-key", "", "FSS verification key"),

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
	fs.StringVar(flags.grepFlag, "g", "", "show entries with MESSAGE matching PATTERN (short)")
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

	fs.Usage = func() { printUsage(fs, stderr) }
	return fs, flags
}

func printUsage(fs *flag.FlagSet, out io.Writer) {
	previous := fs.Output()
	fs.SetOutput(out)
	defer fs.SetOutput(previous)
	fmt.Fprintf(out, "Usage: %s [options]\n", fs.Name())
	fmt.Fprintf(out, "Go systemd journal reader (portable mode, systemd v262 CLI; native PCRE2)\n")
	fmt.Fprintf(out, "\nOptions:\n")
	fs.PrintDefaults()
	fmt.Fprintf(out, "\n%s", officialOptionSurfaceHelp)
}

func (f *cliFlags) validate() error {
	if err := f.validateParserInteractions(); err != nil {
		return err
	}
	return f.validatePostAction()
}

func (f *cliFlags) validateParserInteractions() error {
	// Source exclusivity: --directory=, --file=, --machine=, --root=,
	// --image= are mutually exclusive.
	sources := 0
	if len(f.file.values) > 0 {
		sources++
	}
	if *f.directory != "" || f.seen["directory"] {
		sources++
	}
	if *f.machineFlag != "" || f.seen["machine"] {
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
	sinceUsec, untilUsec, err := f.timeBounds()
	if err != nil {
		return err
	}
	if sinceUsec != nil && untilUsec != nil && *sinceUsec > *untilUsec {
		return errors.New("--since= must be before --until=.")
	}

	// Cursor source exclusivity.
	cursorSources := 0
	if *f.cursorFlag != "" || f.seen["cursor"] {
		cursorSources++
	}
	if *f.afterCursorFlag != "" || f.seen["after-cursor"] {
		cursorSources++
	}
	if *f.cursorFileFlag != "" || f.seen["cursor-file"] {
		cursorSources++
	}
	if *f.since != "" || f.seen["since"] {
		cursorSources++
	}
	if cursorSources > 1 {
		return errors.New("Please specify only one of --since=, --cursor=, --cursor-file=, and --after-cursor=.")
	}

	// Follow/reverse conflict.
	if *f.follow && *f.reverseFlag {
		return errors.New("Please specify either --reverse or --follow, not both.")
	}

	if f.linesFlag.set {
		limit, err := parseLinesLimitValue(f.linesFlag.value)
		if err != nil {
			return err
		}
		if f.isShowAction() && limit.oldest && (*f.reverseFlag || *f.follow) {
			return errors.New("--lines=+N is unsupported when --reverse or --follow is specified.")
		}
	}
	if f.caseSensitiveFlag.set {
		if _, err := parseBoolOption("--case-sensitive", f.caseSensitiveFlag.value); err != nil {
			return err
		}
	}
	boot := effectiveBootFlag(f)
	if boot.set {
		value := strings.TrimSpace(boot.value)
		if value != "all" {
			if _, _, err := parseBootDescriptor(value); err != nil {
				return err
			}
		}
	}

	return nil
}

func (f *cliFlags) isShowAction() bool {
	if f.parsed {
		return f.action == actionShow
	}
	return !*f.versionFlag &&
		!*f.newID128Flag &&
		!*f.fields &&
		*f.field == "" &&
		!*f.listBoots &&
		!*f.listInvocationsFlag &&
		!*f.diskUsageFlag &&
		!hasVacuumFlags(f) &&
		!*f.headerFlag &&
		!*f.verify &&
		!*f.verifyOnly &&
		*f.verifyKey == "" &&
		!*f.sync &&
		!*f.flush &&
		!*f.rotate &&
		!*f.relinquish &&
		!*f.smartRelinquishVarFlag &&
		!*f.listNamespacesFlag &&
		!*f.listCatalogFlag &&
		!*f.dumpCatalogFlag &&
		!*f.updateCatalogFlag &&
		!*f.setupKeysFlag
}

func (f *cliFlags) validateActionArguments(args []string, hasVerifyKey, fieldWasSet bool) error {
	if len(args) == 0 && f.parsed && (f.action == "field" || f.action == "fields") && f.fieldListHasScope() {
		return errors.New("-F/--field= and -N/--fields cannot be combined with options that limit the journal.")
	}
	if len(args) == 0 || !f.actionRejectsArguments(hasVerifyKey, fieldWasSet) {
		return nil
	}
	return fmt.Errorf("Extraneous arguments starting with '%s'", args[0])
}

func (f *cliFlags) actionRejectsArguments(hasVerifyKey, fieldWasSet bool) bool {
	if f.parsed {
		return f.action != actionShow && f.action != "list-catalog" && f.action != "dump-catalog"
	}
	return *f.newID128Flag ||
		*f.setupKeysFlag ||
		*f.updateCatalogFlag ||
		*f.headerFlag ||
		*f.verify ||
		*f.verifyOnly ||
		hasVerifyKey ||
		*f.diskUsageFlag ||
		*f.listBoots ||
		*f.fields ||
		fieldWasSet ||
		*f.listInvocationsFlag ||
		*f.listNamespacesFlag ||
		*f.flush ||
		*f.relinquish ||
		*f.smartRelinquishVarFlag ||
		*f.sync ||
		*f.rotate ||
		hasVacuumFlags(f)
}

func (f *cliFlags) validatePostAction() error {
	// Boot/merge conflict.
	bootConflictsWithMerge := f.boot.set && strings.TrimSpace(f.boot.value) != "all"
	if (bootConflictsWithMerge || *f.thisBootFlag || *f.listBoots) && *f.mergeFlag {
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
	if f.synchronizeOnExitFlag.set {
		enabled, err := parseBoolOption("--synchronize-on-exit", f.synchronizeOnExitFlag.value)
		if err != nil {
			return err
		}
		if enabled {
			return portableUnsupported("--synchronize-on-exit", "requires journald Varlink synchronization on signal exit")
		}
	}
	if *f.sync {
		return portableUnsupported("--sync", "daemon-only journal synchronization; no journald in portable mode")
	}
	if *f.flush {
		return portableUnsupported("--flush", "daemon-only runtime-to-persistent flush; no journald in portable mode")
	}
	if f.action == actionRotateVacuum || !f.parsed && *f.rotate && hasVacuumFlags(f) {
		return portableUnsupported("--rotate with --vacuum-*", "official rotate-and-vacuum action requires journald rotation; portable mode can only vacuum explicit directories without rotation")
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
	if hasVacuumFlags(f) {
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

type cliInput struct {
	directory string
	files     []string
}

type cursorJournal struct {
	cliJournal
	cursor func() (string, error)
}

func currentJournalCursor(j cliJournal) (string, error) {
	if source, ok := j.(*cursorJournal); ok {
		return source.cursor()
	}
	entry, err := j.GetEntry()
	if err != nil {
		return "", err
	}
	return entry.Cursor, nil
}

func (i cliInput) openJournal() (cliJournal, error) {
	return i.openJournalWithCursor(false)
}

func (i cliInput) openJournalWithCursor(capture bool) (cliJournal, error) {
	if i.directory != "" {
		j, err := journal.SdJournalOpenDirectory(i.directory, 0)
		if err != nil {
			return nil, err
		}
		if !capture {
			return j, nil
		}
		return &cursorJournal{j, func() (string, error) { return journal.SdJournalGetCursor(j) }}, nil
	}
	j, err := journal.SdJournalOpenFiles(i.files, 0)
	if err != nil {
		return nil, err
	}
	if !capture {
		return j, nil
	}
	return &cursorJournal{j, func() (string, error) { return journal.SdJournalGetCursor(j) }}, nil
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
	seen := make(map[string]struct{})
	for _, value := range values {
		if value == "-" {
			return nil, portableUnsupported(
				"--file=-",
				"stdin-backed journals require seekable mmap-capable file descriptors and are not supported in portable mode",
			)
		}
		matches, err := globFilePaths(value)
		if err != nil {
			return nil, fmt.Errorf("failed to add paths: %w", err)
		}
		if len(matches) == 0 {
			files = appendUniqueFile(files, seen, value)
			continue
		}
		for _, match := range matches {
			files = appendUniqueFile(files, seen, match)
		}
	}
	return files, nil
}

func appendUniqueFile(files []string, seen map[string]struct{}, path string) []string {
	key := canonicalFileKey(path)
	if _, ok := seen[key]; ok {
		return files
	}
	seen[key] = struct{}{}
	return append(files, path)
}

func canonicalFileKey(path string) string {
	abs, err := filepath.Abs(path)
	if err != nil {
		abs = path
	}
	if resolved, err := filepath.EvalSymlinks(abs); err == nil {
		return resolved
	}
	return abs
}

func (f *cliFlags) timeBounds() (*uint64, *uint64, error) {
	if f.parsed {
		if f.sinceUsec != nil && f.untilUsec != nil && *f.sinceUsec > *f.untilUsec {
			return nil, nil, errors.New("--since= must be before --until=.")
		}
		return f.sinceUsec, f.untilUsec, nil
	}
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
	if err := validateDisjunctions(matches); err != nil {
		return err
	}
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
		return optionalStringFlag{set: true, value: "0"}
	}
	if !*flags.mergeFlag && (*flags.follow || *flags.dmesgFlag || *flags.pagerEndFlag) {
		return optionalStringFlag{set: true, value: "0"}
	}
	return optionalStringFlag{}
}

func applyCLIMatches(j cliJournal, flags *cliFlags) error {
	if err := addJournalctlUnitMatches(j, flags); err != nil {
		return err
	}
	return applyNonUnitMatches(j, flags)
}

func applyNonUnitMatches(j cliJournal, flags *cliFlags) error {
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
	return j.AddConjunction()
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
			if err := j.AddDisjunction(); err != nil {
				return err
			}
		}
		if err := addMatchPair(j, field, id); err != nil {
			return err
		}
	}
	return j.AddConjunction()
}

func addMatchPair(j cliJournal, field, value string) error {
	match, err := journal.ParseMatchString(field + "=" + value)
	if err != nil {
		return err
	}
	j.AddMatch(match)
	return nil
}

func applyBootMatch(j cliJournal, flags *cliFlags, bootOverride optionalStringFlag) error {
	boot := bootOverride
	if !boot.set {
		boot = effectiveBootFlag(flags)
	}
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
	return j.AddConjunction()
}

func addMatchGroup(j cliJournal, pairs [][2]string) error {
	for _, pair := range pairs {
		if err := addMatchPair(j, pair[0], pair[1]); err != nil {
			return err
		}
	}
	return nil
}

func addJournalctlUnitMatches(j cliJournal, flags *cliFlags) error {
	if len(flags.unitFlag.values) == 0 && len(flags.userUnitFlag.values) == 0 {
		return nil
	}
	// Follow retains the initial wildcard expansion, like native journalctl.
	if !flags.unitMatchesReady {
		flags.unitMatchesReady = true
		system, user, err := effectiveUnitSpecs(flags)
		if err == nil {
			flags.unitMatches.system, err = expandUnitSpecs(j, system, systemUnitFieldsFull)
		}
		if err == nil {
			flags.unitMatches.user, err = expandUnitSpecs(j, user, userUnitFieldsFull)
		}
		if err == nil && len(system)+len(user) > 0 && len(flags.unitMatches.system)+len(flags.unitMatches.user) == 0 {
			err = errors.New("Failed to add filter for units: No data available")
		}
		flags.unitMatchesError = err
	}
	if flags.unitMatchesError != nil {
		return flags.unitMatchesError
	}
	systemUnits, userUnits := flags.unitMatches.system, flags.unitMatches.user
	if len(systemUnits) == 0 && len(userUnits) == 0 {
		return nil
	}

	for _, unit := range systemUnits {
		if err := addSystemUnitMatchGroups(j, unit, true); err != nil {
			return err
		}
	}

	uid, uidOK := currentUIDString()
	for _, unit := range userUnits {
		if err := addUserUnitMatchGroups(j, unit, uid, uidOK, true); err != nil {
			return err
		}
	}

	return j.AddConjunction()
}

func addSystemUnitMatchGroups(j cliJournal, unit string, full bool) error {
	if err := addMatchGroup(j, [][2]string{{"_SYSTEMD_UNIT", unit}}); err != nil {
		return err
	}
	if err := j.AddDisjunction(); err != nil {
		return err
	}

	if err := addMatchGroup(j, [][2]string{{"_SYSTEMD_CGROUP", "/init.scope"}, {"UNIT", unit}}); err != nil {
		return err
	}
	if err := j.AddDisjunction(); err != nil {
		return err
	}

	if err := addMatchGroup(j, [][2]string{{"_UID", "0"}, {"OBJECT_SYSTEMD_UNIT", unit}}); err != nil {
		return err
	}
	if err := j.AddDisjunction(); err != nil {
		return err
	}

	if !full {
		return nil
	}
	if err := addMatchGroup(j, [][2]string{{"MESSAGE_ID", coredumpMessageID}, {"COREDUMP_UNIT", unit}}); err != nil {
		return err
	}

	if strings.HasSuffix(unit, ".slice") {
		if err := j.AddDisjunction(); err != nil {
			return err
		}
		if err := addMatchGroup(j, [][2]string{{"_SYSTEMD_SLICE", unit}}); err != nil {
			return err
		}
	}

	return j.AddDisjunction()
}

func addUserUnitMatchGroups(j cliJournal, unit, uid string, uidOK, full bool) error {
	if err := addUserUnitMatchGroup(j, [][2]string{{"_SYSTEMD_USER_UNIT", unit}}, uid, uidOK, false); err != nil {
		return err
	}
	if err := j.AddDisjunction(); err != nil {
		return err
	}

	if err := addUserUnitMatchGroup(j, [][2]string{{"USER_UNIT", unit}}, uid, uidOK, false); err != nil {
		return err
	}
	if err := j.AddDisjunction(); err != nil {
		return err
	}

	if err := addUserUnitMatchGroup(j, [][2]string{{"OBJECT_SYSTEMD_USER_UNIT", unit}}, uid, uidOK, true); err != nil {
		return err
	}
	if err := j.AddDisjunction(); err != nil {
		return err
	}

	if !full {
		return nil
	}
	if err := addUserUnitMatchGroup(j, [][2]string{{"COREDUMP_USER_UNIT", unit}}, uid, uidOK, true); err != nil {
		return err
	}

	if strings.HasSuffix(unit, ".slice") {
		if err := j.AddDisjunction(); err != nil {
			return err
		}
		if err := addUserUnitMatchGroup(j, [][2]string{{"_SYSTEMD_USER_SLICE", unit}}, uid, uidOK, false); err != nil {
			return err
		}
	}

	return j.AddDisjunction()
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
		unit := spec
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
	matcher, err := newUnitPatternMatcher(patterns)
	if err != nil {
		return nil, err
	}
	defer matcher.Close()

	for _, field := range fields {
		err := j.VisitUnique(field, func(value []byte) error {
			if end := bytes.IndexByte(value, 0); end >= 0 {
				value = value[:end]
			}
			if !matcher.Matches(value) {
				return nil
			}
			unit := string(value)
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

func isGlobPattern(value string) bool {
	return strings.ContainsAny(value, "*?[")
}

func globPatternToRegex(pattern string) string {
	var out strings.Builder
	out.WriteString("(?s)^")
	chars := []rune(pattern)
	for i := 0; i < len(chars); i++ {
		switch chars[i] {
		case '*':
			out.WriteString(".*")
		case '?':
			out.WriteByte('.')
		case '[':
			end := i + 1
			if end < len(chars) && (chars[end] == '!' || chars[end] == '^') {
				end++
			}
			if end < len(chars) && chars[end] == ']' {
				end++
			}
			for end < len(chars) && chars[end] != ']' {
				if chars[end] == '[' && end+1 < len(chars) && (chars[end+1] == ':' || chars[end+1] == '.') {
					k := end + 2
					for k+1 < len(chars) && !(chars[k] == chars[end+1] && chars[k+1] == ']') {
						k++
					}
					if k+1 < len(chars) {
						end = k + 2
						continue
					}
				}
				end++
			}
			if end == len(chars) {
				out.WriteString(`\[`)
				continue
			}
			out.WriteByte('[')
			start := i + 1
			if chars[start] == '!' || chars[start] == '^' {
				out.WriteByte('^')
				start++
			}
			for k := start; k < end; k++ {
				if chars[k] == '[' && k+1 < end && (chars[k+1] == ':' || chars[k+1] == '.') {
					marker := chars[k+1]
					n := k + 2
					for n+1 < end && !(chars[n] == marker && chars[n+1] == ']') {
						n++
					}
					if n+1 < end {
						if marker == ':' {
							out.WriteString(string(chars[k : n+2]))
						} else {
							if n != k+3 {
								return "["
							}
							if strings.ContainsRune(`\[]^-`, chars[k+2]) {
								out.WriteByte('\\')
							}
							out.WriteRune(chars[k+2])
						}
						k = n + 1
						continue
					}
				}
				if chars[k] == '\\' || chars[k] == ']' || chars[k] == '[' {
					out.WriteByte('\\')
				}
				out.WriteRune(chars[k])
			}
			out.WriteByte(']')
			i = end
		default:
			out.WriteString(regexp.QuoteMeta(string(chars[i])))
		}
	}
	out.WriteString("$")
	return out.String()
}

type cliPostFilters struct {
	grep               *pcrePattern
	shown              bool
	excludeIdentifiers map[string]struct{}
	captureHighlight   bool
	highlight          matchRange
	bufferedHighlights []matchRange
}

func newCLIPostFilters(flags *cliFlags) (*cliPostFilters, error) {
	var grep *pcrePattern
	if flags.grepSet {
		var err error
		grep, err = compileGrepFilter(*flags.grepFlag, flags.caseSensitiveFlag)
		if err != nil {
			return nil, err
		}
	}
	filters := &cliPostFilters{grep: grep}
	if outputUsesExcludeIdentifier(*flags.output) {
		values := flags.excludeIdentifierFlag.Values()
		if len(values) > 0 {
			filters.excludeIdentifiers = make(map[string]struct{}, len(values))
			for _, value := range values {
				filters.excludeIdentifiers[value] = struct{}{}
			}
		}
	}
	return filters, nil
}

func (f *cliPostFilters) matches(entry *journal.Entry) (bool, error) {
	if f == nil || f.grep == nil {
		return true, nil
	}
	values := entryValues(entry, "MESSAGE")
	if len(values) == 0 {
		return false, nil
	}
	if f.captureHighlight {
		return f.grep.match(values[0], &f.highlight)
	}
	return f.grep.Match(values[0])
}

func (f *cliPostFilters) bufferHighlight() {
	if f != nil && f.captureHighlight && f.grep != nil {
		f.bufferedHighlights = append(f.bufferedHighlights, f.highlight)
	}
}

func (f *cliPostFilters) restoreHighlight() {
	if f != nil && len(f.bufferedHighlights) > 0 {
		f.highlight = f.bufferedHighlights[0]
		f.bufferedHighlights = f.bufferedHighlights[1:]
	}
}

func entryMatches(entry *journal.Entry, since, until *uint64, filters *cliPostFilters) (bool, error) {
	if !entryInTimeRange(entry, since, until) {
		return false, nil
	}
	return filters.matches(entry)
}

func (f *cliPostFilters) excludesEntry(entry *journal.Entry) bool {
	if f == nil || len(f.excludeIdentifiers) == 0 {
		return false
	}
	value, ok := lastEntryValue(entry, "SYSLOG_IDENTIFIER")
	if !ok {
		return false
	}
	_, found := f.excludeIdentifiers[string(cStringBytes(value))]
	return found
}

func outputUsesExcludeIdentifier(mode string) bool {
	switch mode {
	case "short", "short-full", "short-iso", "short-iso-precise", "short-precise",
		"short-monotonic", "short-delta", "short-unix", "with-unit":
		return true
	default:
		return false
	}
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

func parseBoolOption(option, value string) (bool, error) {
	switch strings.ToLower(value) {
	case "1", "true", "yes", "y", "t", "on":
		return true, nil
	case "0", "false", "no", "n", "f", "off":
		return false, nil
	default:
		return false, fmt.Errorf("Bad %s= argument %q", option, value)
	}
}

// Match journalctl's integer grammar, including its fixed 32-bit int range.
func parseNativeNumber(value string) (uint64, bool, error) {
	value = strings.TrimLeft(value, " \t\n\r\v\f")
	base := 0
	if strings.HasPrefix(value, "0b") || strings.HasPrefix(value, "0B") {
		base = 2
	} else if strings.HasPrefix(value, "0o") || strings.HasPrefix(value, "0O") {
		base = 8
	}
	if base != 0 {
		value = strings.TrimLeft(value[2:], " \t\n\r\v\f")
	}
	negative := strings.HasPrefix(value, "-")
	if strings.HasPrefix(value, "+") || negative {
		value = value[1:]
	}
	if base == 0 {
		base = 10
		if strings.HasPrefix(value, "0x") || strings.HasPrefix(value, "0X") {
			base, value = 16, value[2:]
		} else if strings.HasPrefix(value, "0") {
			base = 8
		}
	}
	number, err := strconv.ParseUint(value, base, 64)
	return number, negative, err
}

func parseNativeUint64(value string) (uint64, error) {
	number, negative, err := parseNativeNumber(value)
	if err == nil && negative && number != 0 {
		err = strconv.ErrRange
	}
	return number, err
}

func parseNativeInt32(value string) (int, error) {
	number, negative, err := parseNativeNumber(value)
	if err != nil {
		return 0, strconv.ErrRange
	}
	if negative {
		if number > 1<<31 {
			return 0, strconv.ErrRange
		}
		return int(-int64(number)), nil
	}
	if number > 1<<31-1 {
		return 0, strconv.ErrRange
	}
	return int(number), nil
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
	switch value {
	case "emerg":
		return 0, nil
	case "alert":
		return 1, nil
	case "crit":
		return 2, nil
	case "err":
		return 3, nil
	case "warning":
		return 4, nil
	case "notice":
		return 5, nil
	case "info":
		return 6, nil
	case "debug":
		return 7, nil
	}
	number, err := parseNativeInt32(value)
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
	if number, err := parseNativeInt32(value); err == nil && number >= 0 && number <= 127 {
		return uint8(number), nil
	}
	if facility, ok := facilityNames[value]; ok {
		return facility, nil
	}
	return 0, fmt.Errorf("Bad --facility= argument %q.", value)
}

func validateOutputMode(mode string) error {
	switch mode {
	case "short", "short-full", "short-iso", "short-iso-precise", "short-precise",
		"short-monotonic", "short-delta", "short-unix", "verbose", "export",
		"json", "json-pretty", "json-sse", "json-seq", "cat", "with-unit":
		return nil
	default:
		return fmt.Errorf("Unknown output format %q.", mode)
	}
}

func (f *cliFlags) dispatch(j cliJournal, sinceUsec, untilUsec *uint64, stdout io.Writer, postFilters *cliPostFilters, cursorControl cursorControl, outputOptions outputOptions) error {
	if *f.fields {
		fields, err := j.EnumerateFields()
		if err != nil {
			return fmt.Errorf("enumerate fields: %w", err)
		}
		for _, field := range fields {
			if _, err := fmt.Fprintln(stdout, field); err != nil {
				return err
			}
		}
		return nil
	}
	if f.action == "field" || !f.parsed && *f.field != "" {
		if !validFieldName(*f.field) {
			return fmt.Errorf("Invalid field name: %s", *f.field)
		}
		limit := -1
		if f.linesFlag.set {
			parsed, err := parseLinesLimitValue(f.linesFlag.value)
			if err != nil {
				return err
			}
			if !parsed.all {
				limit = parsed.count
			}
		}
		if limit == 0 {
			return nil
		}
		count := 0
		err := j.VisitUnique(*f.field, func(value []byte) error {
			if count == limit {
				return errStopIteration
			}
			if i := strings.IndexByte(string(value), 0); i >= 0 {
				value = value[:i]
			}
			if _, err := stdout.Write(value); err != nil {
				return err
			}
			if _, err := fmt.Fprintln(stdout); err != nil {
				return err
			}
			count++
			return nil
		})
		if errors.Is(err, errStopIteration) {
			return nil
		}
		return err
	}

	grepTailReverse := f.grepTailImpliesReverse()
	if *f.noTail {
		if *f.reverseFlag {
			return showReverse(j, 0, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
		}
		return showForward(j, 0, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
	}
	if f.linesFlag.set && !*f.noTail {
		limit, err := parseLinesLimitValue(f.linesFlag.value)
		if err != nil {
			return err
		}
		if limit.set && !limit.all {
			if limit.count == 0 {
				return showZero(j, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), cursorControl, !limit.oldest || *f.reverseFlag, *f.reverseFlag || grepTailReverse)
			}
			if limit.oldest {
				return showForward(j, limit.count, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
			}
			if cursorControl.seek != nil && !*f.reverseFlag && !grepTailReverse {
				return showForward(j, limit.count, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
			}
			if grepTailReverse || *f.reverseFlag {
				return showReverse(j, limit.count, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
			}
			return showTail(j, limit.count, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
		}
		if limit.all {
			if *f.reverseFlag {
				return showReverse(j, 0, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
			}
			return showForward(j, 0, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
		}
	}

	switch {
	case *f.head > 0:
		return showForward(j, *f.head, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)

	case *f.tail > 0:
		if *f.reverseFlag {
			return showReverse(j, *f.tail, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
		}
		return showTail(j, *f.tail, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)

	default:
		if *f.pagerEndFlag {
			if *f.reverseFlag || grepTailReverse {
				return showReverse(j, 1000, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
			}
			return showTail(j, 1000, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
		}
		if *f.reverseFlag {
			return showReverse(j, 0, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
		}
		return showForward(j, *f.head, sinceUsec, untilUsec, stdout, *f.showCursorFlag, f.effectiveQuiet(), postFilters, cursorControl, outputOptions)
	}
}

func (f *cliFlags) grepTailImpliesReverse() bool {
	if !f.grepSet || *f.follow || *f.noTail {
		return false
	}
	if !f.linesFlag.set {
		return *f.pagerEndFlag
	}
	limit, err := parseLinesLimitValue(f.linesFlag.value)
	if err != nil {
		return false
	}
	return limit.set && !limit.all && !limit.oldest
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

func openFilteredJournal(input cliInput, matches []string, flags *cliFlags, outputMode string, bootOverride optionalStringFlag) (cliJournal, error) {
	j, err := input.openJournalWithCursor(*flags.showCursorFlag || *flags.cursorFileFlag != "" || flags.seen["cursor-file"])
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
		if err := applyNonUnitMatches(j, flags); err != nil {
			return nil, err
		}
	} else {
		if err := applyBootMatch(j, flags, bootOverride); err != nil {
			return nil, err
		}
		if err := applyCLIMatches(j, flags); err != nil {
			return nil, err
		}
	}

	for _, arg := range matches {
		if arg == "+" {
			if err := j.AddDisjunction(); err != nil {
				return nil, err
			}
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
	return new(timestampParser).parse(value)
}

func parseDurationUsec(value string) (uint64, error) {
	return parseDurationUsecMode(value, false)
}

func parseDurationUsecAllowZero(value string) (uint64, error) {
	return parseDurationUsecMode(value, true)
}

func parseDurationUsecMode(value string, allowZero bool) (uint64, error) {
	result, err := durationValue(value)
	if err != nil {
		return 0, err
	}
	if !allowZero && result == 0 {
		return 0, fmt.Errorf("failed to parse duration: %s", value)
	}
	return result, nil
}

type bootInfo struct {
	bootID     string
	firstEntry uint64
	lastEntry  uint64
	index      int
}

func runListBoots(input cliInput, flags *cliFlags, stdout io.Writer) error {
	j, err := input.openJournal()
	if err != nil {
		return fmt.Errorf("open journal: %w", err)
	}
	defer j.Close()
	boots, err := collectBoots(j)
	if err != nil {
		return fmt.Errorf("list boots: %w", err)
	}
	display, err := selectBootRows(boots, flags)
	if err != nil {
		return err
	}
	if len(display) == 0 {
		return noDataError(flags.effectiveQuiet(), "No boot found.")
	}
	if jsonTableMode(*flags.output) {
		rows := make([]map[string]any, 0, len(display))
		for _, row := range display {
			rows = append(rows, map[string]any{"index": row.index, "boot_id": row.bootID, "first_entry": tableTimestamp(row.firstEntry), "last_entry": tableTimestamp(row.lastEntry)})
		}
		return writeTableJSON(stdout, *flags.output, rows)
	}
	if !*flags.quietFlag {
		fmt.Fprint(stdout, detectOutputColors().tableHeader("BOOT ID"))
	}
	idxWidth := 1
	if *flags.quietFlag {
		for _, boot := range display {
			if width := len(fmt.Sprintf("%d", boot.index)); width > idxWidth {
				idxWidth = width
			}
		}
	} else {
		idxWidth = 3
	}
	for _, boot := range display {
		fmt.Fprintf(
			stdout,
			"%*d %s %s %s\n",
			idxWidth,
			boot.index,
			boot.bootID,
			formatHeaderTimestamp(boot.firstEntry),
			formatHeaderTimestamp(boot.lastEntry),
		)
	}
	return nil
}

func selectBootRows(boots []bootInfo, flags *cliFlags) ([]bootInfo, error) {
	rows := boots
	firstIndex := 1 - len(boots)
	if flags.linesFlag.set {
		limit, err := parseLinesLimitValue(flags.linesFlag.value)
		if err != nil {
			return nil, err
		}
		if limit.set && !limit.all {
			count := limit.count
			if count > len(boots) {
				count = len(boots)
			}
			if limit.oldest {
				rows = boots[:count]
				firstIndex = 1
			} else {
				rows = boots[len(boots)-count:]
				firstIndex = 1 - count
			}
		}
	}
	out := make([]bootInfo, len(rows))
	for i, boot := range rows {
		boot.index = firstIndex + i
		out[i] = boot
	}
	if *flags.reverseFlag {
		for i, j := 0, len(out)-1; i < j; i, j = i+1, j-1 {
			out[i], out[j] = out[j], out[i]
		}
	}
	return out, nil
}

func collectBoots(j cliJournal) ([]bootInfo, error) {
	boots, err := collectBootsIndexed(j)
	if err == nil && len(boots) > 0 {
		return boots, nil
	}
	j.FlushMatches()
	return collectBootsByScan(j)
}

func collectBootsIndexed(j cliJournal) ([]bootInfo, error) {
	seen := make(map[string]struct{})
	var bootIDs []string
	if err := j.VisitUnique("_BOOT_ID", func(value []byte) error {
		id, err := journal.ParseUUID(string(value))
		if err != nil || strings.Trim(id.String(), "0") == "" {
			return nil
		}
		bootID := id.String()
		if _, ok := seen[bootID]; ok {
			return nil
		}
		seen[bootID] = struct{}{}
		bootIDs = append(bootIDs, bootID)
		return nil
	}); err != nil {
		return nil, err
	}

	boots := make([]bootInfo, 0, len(bootIDs))
	for _, bootID := range bootIDs {
		j.FlushMatches()
		if err := addMatchPair(j, "_BOOT_ID", bootID); err != nil {
			return nil, err
		}
		if err := j.AddConjunction(); err != nil {
			return nil, err
		}

		if err := j.SeekHead(); err != nil {
			return nil, err
		}
		ok, err := j.Next()
		if err != nil {
			return nil, err
		}
		if ok == 0 {
			continue
		}
		firstEntry, err := j.GetEntry()
		if err != nil {
			return nil, err
		}

		if err := j.SeekTail(); err != nil {
			return nil, err
		}
		ok, err = j.Previous()
		if err != nil {
			return nil, err
		}
		if ok == 0 {
			continue
		}
		lastEntry, err := j.GetEntry()
		if err != nil {
			return nil, err
		}

		boots = append(boots, bootInfo{
			bootID:     bootID,
			firstEntry: firstEntry.Realtime,
			lastEntry:  lastEntry.Realtime,
		})
	}
	j.FlushMatches()
	sort.Slice(boots, func(i, k int) bool {
		if boots[i].lastEntry != boots[k].lastEntry {
			return boots[i].lastEntry < boots[k].lastEntry
		}
		if boots[i].firstEntry != boots[k].firstEntry {
			return boots[i].firstEntry < boots[k].firstEntry
		}
		return boots[i].bootID < boots[k].bootID
	})
	base := 1 - len(boots)
	for i := range boots {
		boots[i].index = base + i
	}
	return boots, nil
}

func collectBootsByScan(j cliJournal) ([]bootInfo, error) {
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
		if out[i].lastEntry != out[k].lastEntry {
			return out[i].lastEntry < out[k].lastEntry
		}
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
		return "", 0, errors.New("failed to parse boot descriptor: ")
	}
	bootID, rest := "", descriptor
	if len(descriptor) >= 32 {
		if id, err := journal.ParseUUID(descriptor[:32]); err == nil {
			if id != (journal.UUID{}) {
				bootID = id.String()
			}
			rest = descriptor[32:]
		}
		if rest != "" && rest[0] != '+' && rest[0] != '-' {
			return "", 0, fmt.Errorf("failed to parse boot descriptor: %s", descriptor)
		}
	}
	if rest == "" {
		return bootID, 0, nil
	}
	offset, err := parseNativeInt32(rest)
	if err != nil {
		return "", 0, fmt.Errorf("failed to parse boot descriptor: %s", descriptor)
	}
	return bootID, offset, nil
}

type invocationInfo struct {
	id        string
	firstUsec uint64
	lastUsec  uint64
}

type invocationDisplayRow struct {
	index int
	info  invocationInfo
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

func entryInTimeRange(entry *journal.Entry, sinceUsec, untilUsec *uint64) bool {
	if sinceUsec != nil && entry.Realtime < *sinceUsec {
		return false
	}
	if untilUsec != nil && entry.Realtime > *untilUsec {
		return false
	}
	return true
}

func nextEntries(j cliJournal, sinceUsec, untilUsec *uint64, cursorSeek *cursorSeek, checkpoint *string, fn func(*journal.Entry) error) error {
	if cursorSeek != nil {
		entry, ok, err := seekCursorStartTracked(j, cursorSeek, false, checkpoint)
		if err != nil {
			return err
		}
		if !ok {
			return nil
		}
		if checkpoint != nil {
			*checkpoint = entry.Cursor
		}
		if entryInTimeRange(entry, sinceUsec, untilUsec) {
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
		if checkpoint != nil {
			*checkpoint = entry.Cursor
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

func showForward(j cliJournal, limit int, sinceUsec, untilUsec *uint64, stdout io.Writer, showCursor bool, quiet bool, postFilters *cliPostFilters, cursorControl cursorControl, outputOptions outputOptions) error {
	count := 0
	var lastCursor string
	var checkpoint *string
	if showCursor || cursorControl.updateFile != nil {
		checkpoint = &lastCursor
	}
	renderer := newOutputRenderer(outputOptions)
	bootSeparators := newBootSeparatorState(outputOptions.mode, quiet, outputOptions.merge)
	err := nextEntries(j, sinceUsec, untilUsec, cursorControl.seek, checkpoint, func(entry *journal.Entry) error {
		matched, err := renderMatchingEntry(stdout, renderer, &bootSeparators, postFilters, entry)
		if err != nil {
			return err
		}
		if !matched {
			return nil
		}
		count++
		if limit > 0 && count >= limit {
			return errStopIteration
		}
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

func initialCursor(j cliJournal, sinceUsec, untilUsec *uint64, seek *cursorSeek, fromTail, reverse bool) (string, error) {
	var cursor string
	if seek != nil {
		entry, ok, err := seekCursorStartTracked(j, seek, reverse, &cursor)
		if ok {
			cursor = entry.Cursor
		}
		return cursor, err
	}
	var err error
	if fromTail {
		if untilUsec != nil {
			err = j.SeekRealtimeUsec(*untilUsec)
		} else {
			err = j.SeekTail()
		}
	} else if sinceUsec != nil {
		err = j.SeekRealtimeUsec(*sinceUsec)
	} else {
		err = j.SeekHead()
	}
	if err != nil {
		return "", err
	}
	var n int
	if fromTail {
		n, err = j.Previous()
	} else {
		n, err = j.Next()
	}
	if err != nil || n == 0 {
		return "", err
	}
	return currentJournalCursor(j)
}

func showZero(j cliJournal, sinceUsec, untilUsec *uint64, stdout io.Writer, showCursor, quiet bool, control cursorControl, fromTail, reverse bool) error {
	var cursor string
	if showCursor || control.updateFile != nil || control.seek != nil {
		var err error
		cursor, err = initialCursor(j, sinceUsec, untilUsec, control.seek, fromTail, reverse)
		if err != nil {
			return err
		}
	}
	printNoEntries(stdout, 0, quiet)
	return finishCursorOutput(stdout, showCursor, control.updateFile, cursor)
}

func showTail(j cliJournal, limit int, sinceUsec, untilUsec *uint64, stdout io.Writer, showCursor bool, quiet bool, postFilters *cliPostFilters, cursorControl cursorControl, outputOptions outputOptions) error {
	if limit == 0 {
		return showZero(j, sinceUsec, untilUsec, stdout, showCursor, quiet, cursorControl, true, false)
	}
	var lastCursor string
	var checkpoint *string
	if showCursor || cursorControl.updateFile != nil {
		checkpoint = &lastCursor
	}
	var entries []*journal.Entry
	start := 0
	if cursorControl.seek == nil {
		err := previousEntries(j, sinceUsec, untilUsec, nil, func(entry *journal.Entry) error {
			entries = append(entries, entry)
			if len(entries) >= limit {
				return errStopIteration
			}
			return nil
		})
		if errors.Is(err, errStopIteration) {
			err = nil
		}
		if err != nil {
			return err
		}
	} else {
		if err := nextEntries(j, sinceUsec, untilUsec, cursorControl.seek, checkpoint, func(entry *journal.Entry) error {
			if len(entries) < limit {
				entries = append(entries, entry)
				return nil
			}
			entries[start] = entry
			start = (start + 1) % limit
			return nil
		}); err != nil {
			return err
		}
	}
	renderer := newOutputRenderer(outputOptions)
	bootSeparators := newBootSeparatorState(outputOptions.mode, quiet, outputOptions.merge)
	count := 0
	if cursorControl.seek == nil {
		for i := len(entries) - 1; i >= 0; i-- {
			entry := entries[i]
			matched, err := renderMatchingEntry(stdout, renderer, &bootSeparators, postFilters, entry)
			if err != nil {
				return err
			}
			lastCursor = entry.Cursor
			if matched {
				count++
			}
		}
	} else {
		for i := 0; i < len(entries); i++ {
			idx := i
			if len(entries) == limit && limit > 0 {
				idx = (start + i) % limit
			}
			entry := entries[idx]
			matched, err := renderMatchingEntry(stdout, renderer, &bootSeparators, postFilters, entry)
			if err != nil {
				return err
			}
			if matched {
				count++
			}
		}
	}
	if cursorControl.seek == nil && untilUsec != nil && len(entries) < limit && (showCursor || cursorControl.updateFile != nil) {
		if len(entries) > 0 {
			if err := j.SeekCursor(lastCursor); err != nil {
				return err
			}
		} else if err := j.SeekRealtimeUsec(*untilUsec); err != nil {
			return err
		}
		if n, err := j.Next(); err != nil {
			return err
		} else if n > 0 {
			lastCursor, err = currentJournalCursor(j)
			if err != nil {
				return err
			}
		}
	}
	printNoEntries(stdout, count, quiet)
	return finishCursorOutput(stdout, showCursor, cursorControl.updateFile, lastCursor)
}

func showReverse(j cliJournal, limit int, sinceUsec, untilUsec *uint64, stdout io.Writer, showCursor bool, quiet bool, postFilters *cliPostFilters, cursorControl cursorControl, outputOptions outputOptions) error {
	if limit == 0 {
		return streamPreviousOutputEvents(j, sinceUsec, untilUsec, postFilters, cursorControl.seek, stdout, showCursor, quiet, cursorControl, outputOptions)
	}
	renderer := newOutputRenderer(outputOptions)
	events, count, lastCursor, err := previousOutputEvents(j, sinceUsec, untilUsec, postFilters, cursorControl.seek, limit, outputOptions, quiet, showCursor || cursorControl.updateFile != nil)
	if err != nil {
		return err
	}
	for _, event := range events {
		if event.bootID != "" {
			if _, err := fmt.Fprint(stdout, outputOptions.colors.boot(event.bootID)); err != nil {
				return err
			}
			continue
		}
		postFilters.restoreHighlight()
		if err := renderEntry(stdout, renderer, postFilters, event.entry); err != nil {
			return err
		}
	}
	printNoEntries(stdout, count, quiet)
	return finishCursorOutput(stdout, showCursor, cursorControl.updateFile, lastCursor)
}

func streamPreviousOutputEvents(j cliJournal, sinceUsec, untilUsec *uint64, postFilters *cliPostFilters, cursorSeek *cursorSeek, stdout io.Writer, showCursor bool, quiet bool, cursorControl cursorControl, outputOptions outputOptions) error {
	renderer := newOutputRenderer(outputOptions)
	bootContext := newBootSeparatorState(outputOptions.mode, quiet, outputOptions.merge)
	var count int
	var lastCursor string

	visit := func(entry *journal.Entry) (bool, error) {
		lastCursor = entry.Cursor
		if sinceUsec != nil && entry.Realtime < *sinceUsec {
			return false, nil
		}
		if bootContext.enabled {
			bootID := entry.BootID.String()
			if bootContext.previousBoot != "" && bootContext.previousBoot != bootID {
				if _, err := fmt.Fprint(stdout, outputOptions.colors.boot(bootID)); err != nil {
					return false, err
				}
			}
			bootContext.previousBoot = bootID
		}
		matched, matchErr := entryMatches(entry, sinceUsec, untilUsec, postFilters)
		if matchErr != nil {
			err := matchErr
			return false, err
		}
		if matched {
			if err := renderEntry(stdout, renderer, postFilters, entry); err != nil {
				return false, err
			}
			lastCursor = entry.Cursor
			count++
		}
		return true, nil
	}

	if cursorSeek != nil {
		var checkpoint *string
		if showCursor || cursorControl.updateFile != nil {
			checkpoint = &lastCursor
		}
		entry, ok, err := seekCursorStartTracked(j, cursorSeek, true, checkpoint)
		if err != nil {
			return err
		}
		if !ok {
			printNoEntries(stdout, count, quiet)
			return finishCursorOutput(stdout, showCursor, cursorControl.updateFile, lastCursor)
		}
		next, err := visit(entry)
		if err != nil {
			return err
		}
		if !next {
			printNoEntries(stdout, count, quiet)
			return finishCursorOutput(stdout, showCursor, cursorControl.updateFile, lastCursor)
		}
	} else if untilUsec != nil {
		if err := j.SeekRealtimeUsec(*untilUsec); err != nil {
			return err
		}
	} else if err := j.SeekTail(); err != nil {
		return err
	}

	for {
		ok, err := j.Previous()
		if err != nil {
			return err
		}
		if ok == 0 {
			break
		}
		entry, err := j.GetEntry()
		if err != nil {
			return err
		}
		next, err := visit(entry)
		if err != nil {
			return err
		}
		if !next {
			break
		}
	}
	printNoEntries(stdout, count, quiet)
	return finishCursorOutput(stdout, showCursor, cursorControl.updateFile, lastCursor)
}

type reverseOutputEvent struct {
	bootID string
	entry  *journal.Entry
}

func previousOutputEvents(j cliJournal, sinceUsec, untilUsec *uint64, postFilters *cliPostFilters, cursorSeek *cursorSeek, limit int, outputOptions outputOptions, quiet, captureCursor bool) ([]reverseOutputEvent, int, string, error) {
	bootContext := newBootSeparatorState(outputOptions.mode, quiet, outputOptions.merge)
	var events []reverseOutputEvent
	var count int
	var lastCursor string

	visit := func(entry *journal.Entry) (bool, error) {
		lastCursor = entry.Cursor
		if sinceUsec != nil && entry.Realtime < *sinceUsec {
			return false, nil
		}
		if bootContext.enabled {
			bootID := entry.BootID.String()
			if bootContext.previousBoot != "" && bootContext.previousBoot != bootID {
				events = append(events, reverseOutputEvent{bootID: bootID})
			}
			bootContext.previousBoot = bootID
		}
		matched, matchErr := entryMatches(entry, sinceUsec, untilUsec, postFilters)
		if matchErr != nil {
			err := matchErr
			return false, err
		}
		if matched {
			events = append(events, reverseOutputEvent{entry: entry})
			postFilters.bufferHighlight()
			lastCursor = entry.Cursor
			count++
			if limit > 0 && count >= limit {
				return false, nil
			}
		}
		return true, nil
	}

	if cursorSeek != nil {
		var checkpoint *string
		if captureCursor {
			checkpoint = &lastCursor
		}
		entry, ok, err := seekCursorStartTracked(j, cursorSeek, true, checkpoint)
		if err != nil || !ok {
			return events, count, lastCursor, err
		}
		next, err := visit(entry)
		if err != nil || !next {
			return events, count, lastCursor, err
		}
	} else if untilUsec != nil {
		if err := j.SeekRealtimeUsec(*untilUsec); err != nil {
			return nil, 0, "", err
		}
	} else if err := j.SeekTail(); err != nil {
		return nil, 0, "", err
	}

	for {
		ok, err := j.Previous()
		if err != nil {
			return nil, 0, "", err
		}
		if ok == 0 {
			break
		}
		entry, err := j.GetEntry()
		if err != nil {
			return nil, 0, "", err
		}
		next, err := visit(entry)
		if err != nil {
			return nil, 0, "", err
		}
		if !next {
			return events, count, lastCursor, nil
		}
	}
	return events, count, lastCursor, nil
}

func printNoEntries(stdout io.Writer, count int, quiet bool) {
	if count == 0 && !quiet {
		fmt.Fprintln(stdout, "-- No entries --")
	}
}

func previousEntries(j cliJournal, sinceUsec, untilUsec *uint64, cursorSeek *cursorSeek, fn func(*journal.Entry) error) error {
	if cursorSeek != nil {
		entry, ok, err := seekCursorStart(j, cursorSeek, true)
		if err != nil {
			return err
		}
		if !ok {
			return nil
		}
		if entryInTimeRange(entry, sinceUsec, untilUsec) {
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
		if entryInTimeRange(entry, sinceUsec, untilUsec) {
			if err := fn(entry); err != nil {
				return err
			}
		}
	}
}

func seekCursorStart(j cliJournal, seek *cursorSeek, reverse bool) (*journal.Entry, bool, error) {
	return seekCursorStartTracked(j, seek, reverse, nil)
}

func seekCursorStartTracked(j cliJournal, seek *cursorSeek, reverse bool, checkpoint *string) (*journal.Entry, bool, error) {
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
			if checkpoint != nil {
				cursor, err := currentJournalCursor(j)
				if err == nil {
					*checkpoint = cursor
				} else if !errors.Is(err, journal.ErrNoEntry) && !errors.Is(err, journal.ErrEndOfEntries) {
					return nil, false, err
				}
			}

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
		if _, positionErr := j.TestCursor(seek.cursor); errors.Is(positionErr, journal.ErrNoEntry) || errors.Is(positionErr, journal.ErrEndOfEntries) {
			return nil, false, nil
		}
		return nil, false, err
	}
	return entry, true, nil
}

func finishCursorOutput(stdout io.Writer, showCursor bool, cursorFile *string, cursor string) error {
	if cursor == "" {
		return nil
	}
	if showCursor {
		if _, err := fmt.Fprintf(stdout, "-- cursor: %s\n", cursor); err != nil {
			return err
		}
	}
	if cursorFile != nil {
		return writeCursorFileAtomic(*cursorFile, cursor)
	}
	return nil
}

func updateCursorFile(control cursorControl, cursor string) error {
	if control.updateFile == nil || cursor == "" {
		return nil
	}
	return writeCursorFileAtomic(*control.updateFile, cursor)
}

func writeCursorFileAtomic(path, cursor string) error {
	dir := filepath.Dir(path)
	base := filepath.Base(path)
	tmp, err := os.CreateTemp(dir, "."+base+".tmp.*")
	if err != nil {
		return fmt.Errorf("Failed to write new cursor to %s: %w", path, err)
	}
	tmpName := tmp.Name()
	if err := tmp.Chmod(0o600); err != nil {
		_ = tmp.Close()
		_ = os.Remove(tmpName)
		return fmt.Errorf("Failed to write new cursor to %s: %w", path, err)
	}
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

type bootSeparatorState struct {
	enabled      bool
	previousBoot string
	colors       outputColors
}

func newBootSeparatorState(mode string, quiet bool, suppressed bool) bootSeparatorState {
	return bootSeparatorState{enabled: !quiet && !suppressed && outputEmitsBootSeparators(mode)}
}

func outputEmitsBootSeparators(mode string) bool {
	switch mode {
	case "short", "short-full", "short-iso", "short-iso-precise", "short-precise",
		"short-monotonic", "short-delta", "short-unix", "verbose", "with-unit":
		return true
	default:
		return false
	}
}

func (bootSeparators *bootSeparatorState) beforeEntry(stdout io.Writer, entry *journal.Entry) error {
	if bootSeparators.enabled {
		bootID := entry.BootID.String()
		if bootSeparators.previousBoot != "" && bootSeparators.previousBoot != bootID {
			if _, err := fmt.Fprint(stdout, bootSeparators.colors.boot(bootID)); err != nil {
				return err
			}
		}
		bootSeparators.previousBoot = bootID
	}
	return nil
}

func renderMatchingEntry(stdout io.Writer, renderer *outputRenderer, boots *bootSeparatorState, postFilters *cliPostFilters, entry *journal.Entry) (bool, error) {
	boots.colors = renderer.options.colors
	if err := boots.beforeEntry(stdout, entry); err != nil {
		return false, err
	}
	matched, err := postFilters.matches(entry)
	if err != nil || !matched {
		return false, err
	}
	return true, renderEntry(stdout, renderer, postFilters, entry)
}

func renderEntry(stdout io.Writer, renderer *outputRenderer, postFilters *cliPostFilters, entry *journal.Entry) error {
	if postFilters.excludesEntry(entry) {
		if postFilters != nil {
			postFilters.shown = true
		}
		return nil
	}
	if outputSkipsMissingMessage(renderer.options.mode) && len(entryValues(entry, "MESSAGE")) == 0 {
		if postFilters != nil {
			postFilters.shown = true
		}
		return nil
	}
	if postFilters != nil && postFilters.captureHighlight {
		renderer.highlight = postFilters.highlight
	}
	out, err := renderer.render(entry)
	if err != nil {
		return err
	}
	_, err = fmt.Fprint(stdout, out)
	if err == nil && postFilters != nil {
		postFilters.shown = true
	}
	return err
}

func resolveFollowBootFlag(input cliInput, flags *cliFlags) (optionalStringFlag, error) {
	boot := effectiveBootFlag(flags)
	if !boot.set || strings.TrimSpace(boot.value) == "all" {
		return optionalStringFlag{}, nil
	}
	j, err := input.openJournal()
	if err != nil {
		return optionalStringFlag{}, fmt.Errorf("open journal: %w", err)
	}
	defer j.Close()
	bootID, err := resolveBootID(j, strings.TrimSpace(boot.value))
	if err != nil {
		return optionalStringFlag{}, err
	}
	if bootID == "" {
		return optionalStringFlag{}, nil
	}
	return optionalStringFlag{set: true, value: bootID}, nil
}

func forEachFollowEntry(input cliInput, matches []string, flags *cliFlags, outputMode string, bootOverride optionalStringFlag, sinceUsec, untilUsec *uint64, cursorSeek *cursorSeek, fn func(*journal.Entry) error) error {
	j, err := openFilteredJournal(input, matches, flags, outputMode, bootOverride)
	if err != nil {
		return err
	}
	defer j.Close()
	return nextEntries(j, sinceUsec, untilUsec, cursorSeek, nil, fn)
}

func runFollow(input cliInput, matches []string, flags *cliFlags, outputMode string, sinceUsec, untilUsec *uint64, tail int, noTail bool, stdout io.Writer, postFilters *cliPostFilters, cursorControl cursorControl, outputOptions outputOptions) error {
	var lastSeenCursor string
	var lastVisitedCursor string
	renderer := newOutputRenderer(outputOptions)
	bootSeparators := newBootSeparatorState(outputOptions.mode, flags.effectiveQuiet(), outputOptions.merge)
	bootOverride, err := resolveFollowBootFlag(input, flags)
	if err != nil {
		return err
	}
	if noTail || cursorControl.seek != nil {
		if err := forEachFollowEntry(input, matches, flags, outputMode, bootOverride, sinceUsec, untilUsec, cursorControl.seek, func(entry *journal.Entry) error {
			lastVisitedCursor = entry.Cursor
			matched, err := renderMatchingEntry(stdout, renderer, &bootSeparators, postFilters, entry)
			if err != nil {
				return err
			}
			if !matched {
				return nil
			}
			if entry.Cursor != "" {
				lastSeenCursor = entry.Cursor
				return updateCursorFile(cursorControl, lastSeenCursor)
			}
			return nil
		}); err != nil {
			return err
		}
	} else {
		var selected []*journal.Entry
		start := 0
		if err := forEachFollowEntry(input, matches, flags, outputMode, bootOverride, sinceUsec, untilUsec, cursorControl.seek, func(entry *journal.Entry) error {
			if entry.Cursor != "" {
				lastVisitedCursor = entry.Cursor
			}
			if tail == 0 {
				return nil
			}
			if len(selected) < tail {
				selected = append(selected, entry)
				return nil
			}
			selected[start] = entry
			start = (start + 1) % tail
			return nil
		}); err != nil {
			return err
		}
		for i := 0; i < len(selected); i++ {
			idx := i
			if len(selected) == tail {
				idx = (start + i) % tail
			}
			matched, err := renderMatchingEntry(stdout, renderer, &bootSeparators, postFilters, selected[idx])
			if err != nil {
				return err
			}
			if matched {
				lastSeenCursor = selected[idx].Cursor
			}
		}
		if err := updateCursorFile(cursorControl, lastSeenCursor); err != nil {
			return err
		}
	}
	if cursorControl.updateFile != nil && lastVisitedCursor != "" && lastVisitedCursor != lastSeenCursor {
		if err := updateCursorFile(cursorControl, lastVisitedCursor); err != nil {
			return err
		}
		lastSeenCursor = lastVisitedCursor
	}
	pollInterval := 100 * time.Millisecond
	const maxPollInterval = time.Second
	for {
		time.Sleep(pollInterval)
		seek := cursorControl.seek
		if lastVisitedCursor != "" {
			seek = &cursorSeek{cursor: lastVisitedCursor, after: true}
		}
		emitted := 0
		if err := forEachFollowEntry(input, matches, flags, outputMode, bootOverride, sinceUsec, untilUsec, seek, func(entry *journal.Entry) error {
			lastVisitedCursor = entry.Cursor
			matched, err := renderMatchingEntry(stdout, renderer, &bootSeparators, postFilters, entry)
			if err != nil {
				return err
			}
			if !matched {
				return nil
			}
			emitted++
			if entry.Cursor != "" {
				lastSeenCursor = entry.Cursor
				return updateCursorFile(cursorControl, lastSeenCursor)
			}
			return nil
		}); err != nil {
			return err
		}
		if cursorControl.updateFile != nil && lastVisitedCursor != "" && lastVisitedCursor != lastSeenCursor {
			if err := updateCursorFile(cursorControl, lastVisitedCursor); err != nil {
				return err
			}
			lastSeenCursor = lastVisitedCursor
		}
		if emitted > 0 {
			pollInterval = 100 * time.Millisecond
		} else {
			pollInterval *= 2
			if pollInterval > maxPollInterval {
				pollInterval = maxPollInterval
			}
		}
	}
}

func runVerify(input cliInput, verifyKey string, hasVerifyKey, quiet bool, stdout, stderr io.Writer) error {
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
		if err := verifyOneFile(path, verifyKey, hasVerifyKey, directoryInput, quiet, stderr); err != nil {
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
	if f.parsed {
		return f.action == actionVacuum || f.action == actionRotateVacuum
	}
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
		value, err := parseNativeUint64(*flags.vacuumFilesFlag)
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
	invalid := func() (uint64, error) { return 0, fmt.Errorf("failed to parse --vacuum-size value: %s", value) }
	units := [...]string{"E", "P", "T", "G", "M", "K", "B", ""}
	factors := [...]uint64{1 << 60, 1 << 50, 1 << 40, 1 << 30, 1 << 20, 1 << 10, 1, 1}
	remaining, firstUnit := value, 0
	var total uint64
	for {
		remaining = strings.TrimLeft(remaining, " \t\n\r")
		if strings.HasPrefix(remaining, "-") {
			return invalid()
		}
		number := strings.TrimLeft(remaining, " \t\n\r\v\f")
		negative := strings.HasPrefix(number, "-")
		if strings.HasPrefix(number, "+") || negative {
			number = number[1:]
		}
		end := 0
		for end < len(number) && number[end] >= '0' && number[end] <= '9' {
			end++
		}
		if end == 0 {
			return invalid()
		}
		whole, err := strconv.ParseUint(number[:end], 10, 64)
		if err != nil {
			return invalid()
		}
		if negative {
			whole = -whole
		}
		remaining = number[end:]
		fraction := float64(0)
		if strings.HasPrefix(remaining, ".") {
			remaining = remaining[1:]
			end = 0
			for end < len(remaining) && remaining[end] >= '0' && remaining[end] <= '9' {
				end++
			}
			if end > 0 {
				part, err := strconv.ParseUint(remaining[:end], 10, 64)
				if err != nil {
					return invalid()
				}
				fraction = float64(part)
				for range end {
					fraction /= 10
				}
				remaining = remaining[end:]
			}
		}
		remaining = strings.TrimLeft(remaining, " \t\n\r")
		unit := firstUnit
		for unit < len(units) && !strings.HasPrefix(remaining, units[unit]) {
			unit++
		}
		if unit == len(units) {
			return invalid()
		}
		factor := factors[unit]
		ceiling := whole
		if fraction > 0 {
			if ceiling == ^uint64(0) {
				return invalid()
			}
			ceiling++
		}
		if ceiling > ^uint64(0)/factor {
			return invalid()
		}
		part := whole*factor + uint64(fraction*float64(factor))
		if part > ^uint64(0)-total {
			return invalid()
		}
		total += part
		remaining = remaining[len(units[unit]):]
		if remaining == "" {
			return total, nil
		}
		firstUnit = unit + 1
	}
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

	retentionLimit := uint64(0)
	if opts.maxRetentionUsec > 0 {
		now := currentRealtimeUsec()
		if now > opts.maxRetentionUsec {
			retentionLimit = now - opts.maxRetentionUsec
		}
	}
	patchVacuumCreationTimes(candidates, opts, retentionLimit, sum, activeFiles)
	sort.Slice(candidates, func(i, j int) bool {
		return vacuumCandidateLess(candidates[i], candidates[j])
	})

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
		match := archivedJournalNameRe().FindStringSubmatch(name)
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
		match := corruptJournalNameRe().FindStringSubmatch(name)
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
	file, err := openVacuumCandidate(path)
	if err != nil {
		return false, err
	}
	defer file.Close()
	openedInfo, err := file.Stat()
	if err != nil {
		return false, err
	}
	if !os.SameFile(info, openedInfo) {
		return false, fmt.Errorf("journal file changed while checking emptiness: %s", path)
	}
	var buf [8]byte
	if _, err := file.ReadAt(buf[:], journalHeaderNEntriesOffset); err != nil {
		return false, err
	}
	return binary.LittleEndian.Uint64(buf[:]) == 0, nil
}

func patchVacuumRealtime(candidate *vacuumCandidate, info os.FileInfo) {
	realtime := earlierVacuumUsec(candidate.realtime, timeToUsec(info.ModTime()))
	candidate.realtime = vacuumMetadataRealtime(info, realtime)
}

func patchVacuumCreationTimes(candidates []vacuumCandidate, opts vacuumOptions, retentionLimit, usage, activeFiles uint64) {
	if len(candidates) == 0 || retentionLimit == 0 &&
		(opts.maxUse == 0 || usage <= opts.maxUse) &&
		(opts.maxFiles == 0 || activeFiles+uint64(len(candidates)) <= opts.maxFiles) {
		return
	}
	sequenceOnly := candidates[0].haveSeqno
	for _, candidate := range candidates[1:] {
		if !candidate.haveSeqno || candidate.seqnumID != candidates[0].seqnumID {
			sequenceOnly = false
			break
		}
	}
	for i := range candidates {
		// A shared sequence ID fixes order; only unresolved age can need I/O.
		if sequenceOnly && (retentionLimit == 0 || candidates[i].realtime < retentionLimit) {
			continue
		}
		candidates[i].realtime = vacuumCreationRealtime(candidates[i].path, candidates[i].realtime)
	}
}

func earlierVacuumUsec(current, candidate uint64) uint64 {
	if candidate > 0 && candidate < current {
		return candidate
	}
	return current
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
	infos, err := collectInvocationsFromInput(input, flags, "--list-invocations")
	if err != nil {
		return err
	}
	display, err := selectInvocationRows(infos, flags)
	if err != nil {
		return err
	}
	if len(display) == 0 {
		return noDataError(flags.effectiveQuiet(), "No invocation ID found.")
	}
	if jsonTableMode(*flags.output) {
		rows := make([]map[string]any, 0, len(display))
		for _, row := range display {
			rows = append(rows, map[string]any{"index": row.index, "invocation_id": row.info.id, "first_entry": tableTimestamp(row.info.firstUsec), "last_entry": tableTimestamp(row.info.lastUsec)})
		}
		return writeTableJSON(stdout, *flags.output, rows)
	}
	if !*flags.quietFlag {
		fmt.Fprint(stdout, detectOutputColors().tableHeader("INVOCATION ID"))
	}
	idxWidth := 1
	if *flags.quietFlag {
		for _, row := range display {
			idxText := fmt.Sprintf("%d", row.index)
			if len(idxText) > idxWidth {
				idxWidth = len(idxText)
			}
		}
	}
	for _, row := range display {
		info := row.info
		if *flags.quietFlag {
			fmt.Fprintf(
				stdout,
				"%*d %s %s %s\n",
				idxWidth,
				row.index,
				info.id,
				formatHeaderTimestamp(info.firstUsec),
				formatHeaderTimestamp(info.lastUsec),
			)
		} else {
			fmt.Fprintf(
				stdout,
				"%3d %-32s %s %s\n",
				row.index,
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
	return formatCalendarTimestamp(usec, false, "Mon 2006-01-02 15:04:05 MST", true, false)
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

func selectInvocationRows(infos []invocationInfo, flags *cliFlags) ([]invocationDisplayRow, error) {
	selected := infos
	firstIndex := 1 - len(infos)
	if !flags.linesFlag.set {
		return buildInvocationDisplayRows(selected, firstIndex, *flags.reverseFlag), nil
	}
	limit, err := parseLinesLimitValue(flags.linesFlag.value)
	if err != nil {
		return nil, err
	}
	if limit.all || limit.count >= len(infos) {
		if limit.oldest {
			firstIndex = 1
		}
		return buildInvocationDisplayRows(selected, firstIndex, *flags.reverseFlag), nil
	}
	if limit.count <= 0 {
		return nil, nil
	}
	if limit.oldest {
		selected = infos[:limit.count]
		firstIndex = 1
	} else {
		selected = infos[len(infos)-limit.count:]
		firstIndex = 1 - len(selected)
	}
	return buildInvocationDisplayRows(selected, firstIndex, *flags.reverseFlag), nil
}

func buildInvocationDisplayRows(infos []invocationInfo, firstIndex int, reverse bool) []invocationDisplayRow {
	out := make([]invocationDisplayRow, len(infos))
	for i, info := range infos {
		out[i] = invocationDisplayRow{index: firstIndex + i, info: info}
	}
	if reverse {
		for i, j := 0, len(out)-1; i < j; i, j = i+1, j-1 {
			out[i], out[j] = out[j], out[i]
		}
	}
	return out
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

func verifyOneFile(path, verifyKey string, hasVerifyKey, directoryInput, quiet bool, stderr io.Writer) error {
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
	if !quiet {
		fmt.Fprintf(stderr, "PASS: %s\n", path)
	}
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
