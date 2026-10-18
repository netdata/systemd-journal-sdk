//go:build cgo

package main

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"strconv"
	"strings"
)

type cliAction string

const (
	actionShow         cliAction = "show"
	actionVacuum       cliAction = "vacuum"
	actionRotateVacuum cliAction = "rotate-and-vacuum"
)

var shortOptions = map[byte]string{
	'h': "help", 'e': "pager-end", 'f': "follow", 'o': "output", 'a': "all", 'l': "full",
	'n': "lines", 'q': "quiet", 'm': "merge", 'b': "boot", 'k': "dmesg", 'D': "directory",
	'p': "priority", 'g': "grep", 'c': "cursor", 'S': "since", 'U': "until", 't': "identifier",
	'T': "exclude-identifier", 'u': "unit", 'I': "I", 'N': "fields", 'F': "field", 'x': "catalog",
	'r': "reverse", 'M': "machine", 'i': "file", 'W': "no-hostname",
}

func (f *cliFlags) selectAction(action cliAction) {
	if action == "rotate" && f.action == actionVacuum || action == actionVacuum && f.action == "rotate" {
		action = actionRotateVacuum
	}
	f.action = action
	for name, dest := range map[cliAction]*bool{
		"list-boots": f.listBoots, "fields": f.fields, "sync": f.sync, "flush": f.flush,
		"rotate": f.rotate, "relinquish-var": f.relinquish, "verify": f.verify,
		"verify-only": f.verifyOnly, "new-id128": f.newID128Flag,
		"list-invocations": f.listInvocationsFlag, "list-namespaces": f.listNamespacesFlag,
		"disk-usage": f.diskUsageFlag, "header": f.headerFlag, "list-catalog": f.listCatalogFlag,
		"dump-catalog": f.dumpCatalogFlag, "update-catalog": f.updateCatalogFlag,
		"setup-keys": f.setupKeysFlag, "smart-relinquish-var": f.smartRelinquishVarFlag,
	} {
		*dest = name == action
	}
	if action != "field" {
		*f.field = ""
	}
}

// parseCLIOptions preserves argv order and never rewrites a required option value.
func parseCLIOptions(argv []string, fs *flag.FlagSet, f *cliFlags, stdout io.Writer) ([]string, bool, error) {
	f.fullWidth = true
	f.parsed = true
	f.action = actionShow
	f.seen = make(map[string]bool)
	var operands []string
	for i := 0; i < len(argv); i++ {
		arg := argv[i]
		if arg == "--" {
			operands = append(operands, argv[i+1:]...)
			break
		}
		if arg == "-" || !strings.HasPrefix(arg, "-") {
			operands = append(operands, arg)
			continue
		}
		long := strings.HasPrefix(arg, "--")
		token := arg[1:]
		if long {
			token = arg[2:]
		}
		for token != "" {
			var name, value string
			var attached bool
			if long {
				name, value, attached = strings.Cut(token, "=")
				var err error
				name, err = resolveLongOption(name, fs)
				if err != nil {
					return nil, false, err
				}
				token = ""
			} else {
				var ok bool
				name, ok = shortOptions[token[0]]
				if !ok {
					return nil, false, fmt.Errorf("invalid option -- '%c'", token[0])
				}
				token = token[1:]
			}
			optional := name == "boot" || name == "lines" || name == "case-sensitive"
			noArg := name == "help" || name == "introspect-cli"
			if opt := fs.Lookup(name); opt != nil {
				if b, ok := opt.Value.(interface{ IsBoolFlag() bool }); ok {
					noArg = b.IsBoolFlag() && !optional
				}
			}
			if noArg {
				if attached {
					return nil, false, fmt.Errorf("option '--%s' doesn't allow an argument", name)
				}
				value = "true"
			} else {
				if !long && token != "" {
					value, attached, token = token, true, ""
				}
				if !attached {
					if optional {
						switch name {
						case "case-sensitive":
							value = "true"
						case "boot":
							value = "0"
						case "lines":
							value = "10"
						}
						if i+1 < len(argv) && name != "case-sensitive" {
							candidate := argv[i+1]
							valid := false
							if name == "boot" {
								_, _, err := parseBootDescriptor(candidate)
								valid = err == nil || candidate == "all"
							}
							if name == "lines" {
								_, err := parseLinesLimitValue(candidate)
								valid = err == nil
							}
							if valid {
								i++
								value = candidate
							}
						}
					} else {
						if i+1 >= len(argv) {
							return nil, false, fmt.Errorf("option '--%s' requires an argument", name)
						}
						i++
						value = argv[i]
					}
				}
			}
			terminal, err := applyCLIOption(name, value, fs, f, stdout)
			if err != nil || terminal {
				return nil, terminal, err
			}
		}
	}
	return operands, false, nil
}

func resolveLongOption(name string, fs *flag.FlagSet) (string, error) {
	if name == "help" || name == "introspect-cli" || fs.Lookup(name) != nil && len(name) > 1 {
		return name, nil
	}
	var found string
	ambiguous := false
	visit := func(candidate string) {
		if len(candidate) > 1 && strings.HasPrefix(candidate, name) {
			if found != "" {
				ambiguous = true
			}
			found = candidate
		}
	}
	fs.VisitAll(func(f *flag.Flag) { visit(f.Name) })
	visit("help")
	visit("introspect-cli")
	if ambiguous {
		return "", fmt.Errorf("option '--%s' is ambiguous", name)
	}
	if found == "" {
		return "", fmt.Errorf("unrecognized option '--%s'", name)
	}
	return found, nil
}

func applyCLIOption(name, value string, fs *flag.FlagSet, f *cliFlags, stdout io.Writer) (bool, error) {
	switch name {
	case "help":
		printUsage(fs, stdout)
		return true, nil
	case "version":
		fmt.Fprintln(stdout, "journalctl (systemd-journal-sdk Go rewrite)")
		fmt.Fprintln(stdout, "CLI compatibility: systemd v262 (8cc40e0c5e92)")
		fmt.Fprintln(stdout, "portable file-backed mode; PCRE2 "+pcreVersion())
		return true, nil
	case "introspect-cli":
		return true, printCLIIntrospection(stdout, *f.output)
	case "output":
		if value == "help" {
			printOutputModeHelp(stdout)
			return true, nil
		}
		if err := validateOutputMode(value); err != nil {
			return false, err
		}
	case "facility":
		for _, part := range strings.Split(value, ",") {
			if part == "" {
				continue
			}
			if part == "help" {
				printFacilityHelp(stdout, *f.quietFlag)
				return true, nil
			}
			if _, err := parseFacility(part); err != nil {
				return false, err
			}
		}
	case "priority":
		if _, err := parsePriorityFilter([]string{value}); err != nil {
			return false, err
		}
	case "lines":
		if _, err := parseLinesLimitValue(value); err != nil {
			return false, err
		}
	case "boot":
		if value != "all" {
			if _, _, err := parseBootDescriptor(value); err != nil {
				return false, err
			}
		}
		*f.thisBootFlag = false
	case "this-boot":
		f.boot = optionalStringFlag{set: true, value: "0"}
	case "invocation":
		if _, _, _, err := parseInvocationDescriptor(value); err != nil {
			return false, err
		}
		*f.invocationShortFlag = false
		f.invocationSet = true
	case "I":
		*f.invocationFlag = "0"
		f.invocationSet = true
	case "case-sensitive", "synchronize-on-exit":
		if _, err := parseBoolOption("--"+name, value); err != nil {
			return false, err
		}
	case "since", "until":
		parsed, err := f.timestamps.parse(value)
		if err != nil {
			return false, fmt.Errorf("Failed to parse timestamp: %s", value)
		}
		if name == "since" {
			f.sinceUsec = &parsed
		} else {
			f.untilUsec = &parsed
		}
	case "vacuum-size":
		if _, err := parseVacuumSize(value); err != nil {
			return false, err
		}
	case "vacuum-files":
		if _, err := parseNativeUint64(value); err != nil {
			return false, fmt.Errorf("Failed to parse vacuum files: %s", value)
		}
	case "vacuum-time", "interval":
		parsed, err := parseDurationUsecAllowZero(value)
		if err != nil || name == "interval" && parsed == 0 {
			return false, fmt.Errorf("Failed to parse %s: %s", name, value)
		}
	case "head", "tail":
		if n, err := strconv.Atoi(value); err != nil || n < 0 {
			return false, fmt.Errorf("--%s must be a non-negative integer", name)
		}
	}
	if name == "output-fields" {
		fields := parseOutputFields(*f.outputFieldsFlag)
		seen := make(map[string]bool, len(fields))
		for _, field := range fields {
			seen[field] = true
		}
		for _, field := range parseOutputFields(value) {
			if !seen[field] {
				fields = append(fields, field)
				seen[field] = true
			}
		}
		value = strings.Join(fields, ",")
	}
	switch name {
	case "field", "fields", "list-boots", "list-invocations", "list-namespaces", "disk-usage", "header",
		"list-catalog", "dump-catalog", "update-catalog", "setup-keys", "new-id128", "verify", "verify-only",
		"sync", "relinquish-var", "smart-relinquish-var", "flush", "rotate":
		f.selectAction(cliAction(name))
	case "vacuum-size", "vacuum-files", "vacuum-time":
		f.selectAction(actionVacuum)
	case "verify-key":
		f.selectAction("verify")
		*f.mergeFlag = false
	}
	if err := fs.Set(name, value); err != nil {
		return false, err
	}
	f.seen[name] = true
	if name == "grep" {
		f.grepSet = true
	}
	if name == "full" {
		*f.noFullFlag = false
		f.fullWidth = true
	}
	if name == "no-full" {
		*f.fullFlag = false
		f.fullWidth = false
	}
	if name == "output" && (*f.output == "cat" || *f.output == "export" || strings.HasPrefix(*f.output, "json")) {
		*f.quietFlag = true
	}
	return false, nil
}

func (f *cliFlags) fieldListHasScope() bool {
	boot := f.boot.set && f.boot.value != "all"
	invocation := f.invocationSet && (*f.invocationShortFlag || *f.invocationFlag != "all")
	return boot || invocation || *f.dmesgFlag || len(f.unitFlag.values) > 0 || len(f.userUnitFlag.values) > 0 ||
		len(f.identifierFlag.values) > 0 || len(f.excludeIdentifierFlag.values) > 0 || len(f.priorityFlag.values) > 0 ||
		len(f.facilityFlag.values) > 0 || f.seen["since"] || f.seen["until"] || f.seen["cursor"] ||
		f.seen["after-cursor"] || f.seen["cursor-file"] || f.grepSet
}

func validateDisjunctions(args []string) error {
	for i, arg := range args {
		if arg == "+" && (i == 0 || i == len(args)-1 || args[i-1] == "+") {
			return errors.New("\"+\" can only be used between terms.")
		}
	}
	return nil
}
