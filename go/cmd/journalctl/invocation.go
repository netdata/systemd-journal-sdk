//go:build cgo

package main

import (
	"bytes"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

type invocationJournal struct {
	cliJournal
	data     func(string) ([]byte, error)
	realtime func() (uint64, error)
	cursor   func() (string, error)
}

func (input cliInput) openInvocationJournal() (*invocationJournal, error) {
	if input.directory != "" {
		j, err := journal.SdJournalOpenDirectory(input.directory, 0)
		if err != nil {
			return nil, err
		}
		return &invocationJournal{j, func(field string) ([]byte, error) { return journal.SdJournalGetData(j, field) }, j.GetRealtimeUsec, func() (string, error) { return journal.SdJournalGetCursor(j) }}, nil
	}
	j, err := journal.SdJournalOpenFiles(input.files, 0)
	if err != nil {
		return nil, err
	}
	return &invocationJournal{j, func(field string) ([]byte, error) { return journal.SdJournalGetData(j, field) }, j.GetRealtimeUsec, func() (string, error) { return journal.SdJournalGetCursor(j) }}, nil
}

func singleInvocationUnit(j cliJournal, flags *cliFlags, optionName string) ([]string, []string, error) {
	raw := rawUnitSpecs(flags)
	count := len(raw.system) + len(raw.user)
	if count == 0 {
		return nil, nil, fmt.Errorf("Using %s requires a unit. Please specify a unit name with -u/--unit=/--user-unit=.", optionName)
	}
	if count > 1 {
		return nil, nil, fmt.Errorf("Using %s with multiple units is not supported.", optionName)
	}
	system, user, err := effectiveUnitSpecs(flags)
	if err != nil {
		return nil, nil, err
	}
	specs, fields := system, systemUnitFieldsFull[:3]
	if len(user) > 0 {
		specs, fields = user, userUnitFieldsFull[:3]
	}
	units, err := expandUnitSpecs(j, specs, fields)
	if err != nil {
		return nil, nil, err
	}
	if len(units) == 0 {
		return nil, nil, fmt.Errorf("No matching unit found for '%s' in journal.", specs[0])
	}
	if len(units) > 1 {
		return nil, nil, fmt.Errorf("Multiple matching units found for '%s' in journal.", specs[0])
	}
	if len(user) > 0 {
		return nil, units, nil
	}
	return units, nil, nil
}

func invocationBootID(j cliJournal, flags *cliFlags) (string, error) {
	boot := effectiveBootFlag(flags)
	if !boot.set || strings.TrimSpace(boot.value) == "all" {
		return "", nil
	}
	return resolveBootID(j, strings.TrimSpace(boot.value))
}

func addInvocationBoot(j cliJournal, id string) error {
	if id == "" {
		return nil
	}
	return addFieldMatches(j, "_BOOT_ID", []string{id})
}

// Discovery reads only eligible indexed entries; a second reader supplies global
// ID bounds without changing the discovery reader's predicate or cursor state.
type invocationWalker struct {
	scan, bounds *invocationJournal
	bootID       string
	older        bool
	done         bool
	anchor       string
	previous     string
	units        unitSelection
	eligibility  [][]byte
	unrestricted bool
}

func newInvocationWalker(input cliInput, flags *cliFlags, option string, older, show bool) (*invocationWalker, error) {
	scan, err := input.openInvocationJournal()
	if err != nil {
		return nil, err
	}
	w := &invocationWalker{scan: scan, older: older}
	ok := false
	defer func() {
		if !ok {
			w.Close()
		}
	}()
	// Show validates boot before unit; invocation tables acquire unit first.
	if show {
		w.bootID, err = invocationBootID(scan, flags)
		if err != nil {
			return nil, err
		}
	}
	var system, user []string
	if option != "" {
		system, user, err = singleInvocationUnit(scan, flags, option)
		if err != nil {
			return nil, err
		}
	}
	if !show {
		w.bootID, err = invocationBootID(scan, flags)
		if err != nil {
			return nil, err
		}
	}
	w.units = unitSelection{system, user}
	if option != "" {
		w.eligibility, err = invocationEligibility(scan)
		if err != nil {
			return nil, err
		}
		w.done = len(w.eligibility) == 0
	}
	if w.done {
		ok = true
		return w, nil
	}
	if err = w.resetScan(false); err != nil {
		return nil, err
	}
	if older {
		err = scan.SeekTail()
	} else {
		err = scan.SeekHead()
	}
	if err != nil {
		return nil, err
	}
	w.bounds, err = input.openInvocationJournal()
	if err != nil {
		return nil, err
	}
	ok = true
	return w, nil
}

func (w *invocationWalker) Close() {
	if w.scan != nil {
		_ = w.scan.Close()
	}
	if w.bounds != nil {
		_ = w.bounds.Close()
	}
}

func parseInvocationIDBytes(value []byte) ([16]byte, bool) {
	var id [16]byte
	if len(value) > 37 {
		return id, false
	}
	if end := bytes.IndexByte(value, 0); end >= 0 {
		value = value[:end]
	}
	if len(value) == 36 {
		if value[8] != '-' || value[13] != '-' || value[18] != '-' || value[23] != '-' {
			return id, false
		}
		value = bytes.ReplaceAll(value, []byte{'-'}, nil)
	}
	if len(value) != 32 {
		return id, false
	}
	if _, err := hex.Decode(id[:], value); err != nil || id == [16]byte{} {
		return id, false
	}
	return id, true
}

func parseInvocationIDValue(value []byte) string {
	id, ok := parseInvocationIDBytes(value)
	if !ok {
		return ""
	}
	return hex.EncodeToString(id[:])
}

func invocationEligibility(j cliJournal) ([][]byte, error) {
	var matches [][]byte
	for _, field := range invocationIDFields {
		if err := j.VisitUnique(field, func(value []byte) error {
			if _, ok := parseInvocationIDBytes(value); ok {
				matches = append(matches, append([]byte(field+"="), value...))
			}
			return nil
		}); err != nil {
			return nil, err
		}
	}
	return matches, nil
}

func (w *invocationWalker) addUnitScope(j cliJournal) error {
	for _, unit := range w.units.system {
		if err := addSystemUnitMatchGroups(j, unit, false); err != nil {
			return err
		}
	}
	uid, uidOK := currentUIDString()
	for _, unit := range w.units.user {
		if err := addUserUnitMatchGroups(j, unit, uid, uidOK, false); err != nil {
			return err
		}
	}
	if len(w.units.system)+len(w.units.user) > 0 {
		return j.AddConjunction()
	}
	return nil
}

func (w *invocationWalker) resetScan(unrestricted bool) error {
	j := w.scan
	j.FlushMatches()
	if !unrestricted {
		if err := addInvocationBoot(j, w.bootID); err != nil {
			return err
		}
		if err := w.addUnitScope(j); err != nil {
			return err
		}
	}
	for i, payload := range w.eligibility {
		if i > 0 {
			if err := j.AddDisjunction(); err != nil {
				return err
			}
		}
		j.AddMatch(payload)
	}
	if len(w.eligibility) > 0 {
		if err := j.AddConjunction(); err != nil {
			return err
		}
	}
	w.unrestricted = unrestricted
	return nil
}

func primaryInvocationID(j *invocationJournal) (string, error) {
	for _, field := range invocationIDFields {
		payload, err := j.data(field)
		if errors.Is(err, journal.ErrNoEntry) {
			continue
		}
		if err != nil {
			return "", err
		}
		if len(payload) > len(field) {
			if id := parseInvocationIDValue(payload[len(field)+1:]); id != "" {
				return id, nil
			}
		}
	}
	return "", nil
}

func (w *invocationWalker) bound(id string) (invocationInfo, string, bool, error) {
	j := w.bounds
	if j == nil {
		return invocationInfo{}, "", false, nil
	}
	j.FlushMatches()
	if err := addInvocationBoot(j, w.bootID); err != nil {
		return invocationInfo{}, "", false, err
	}
	var matchErr error
	if id == "" {
		matchErr = w.addUnitScope(j)
	} else {
		matchErr = addInvocationMatches(j, id)
	}
	if matchErr != nil {
		return invocationInfo{}, "", false, matchErr
	}
	if err := j.SeekHead(); err != nil {
		return invocationInfo{}, "", false, err
	}
	n, err := j.Next()
	if err != nil || n == 0 {
		return invocationInfo{}, "", false, err
	}
	first, err := j.realtime()
	if err != nil {
		return invocationInfo{}, "", false, err
	}
	anchor := ""
	if w.older {
		anchor, err = j.cursor()
		if err != nil {
			return invocationInfo{}, "", false, err
		}
	}
	if err := j.SeekTail(); err != nil {
		return invocationInfo{}, "", false, err
	}
	n, err = j.Previous()
	if err != nil || n == 0 {
		return invocationInfo{}, "", false, err
	}
	last, err := j.realtime()
	if err != nil {
		return invocationInfo{}, "", false, err
	}
	if !w.older {
		anchor, err = j.cursor()
		if err != nil {
			return invocationInfo{}, "", false, err
		}
	}
	return invocationInfo{id, first, last}, anchor, true, nil
}

func (w *invocationWalker) step() (bool, error) {
	if w.anchor != "" {
		cursor := w.anchor
		w.anchor = ""
		direction := journal.DirectionForward
		if w.older {
			direction = journal.DirectionBackward
		}
		if err := w.scan.SeekCursorDirection(cursor, direction); err != nil {
			return false, err
		}
		exact, err := w.scan.TestCursor(cursor)
		if errors.Is(err, journal.ErrEndOfEntries) || errors.Is(err, journal.ErrNoEntry) {
			return false, nil
		}
		if err != nil {
			return false, err
		}
		if !exact {
			// A cursor mismatch also represents EOF in the native facade.
			_, err := w.scan.cursor()
			if errors.Is(err, journal.ErrEndOfEntries) || errors.Is(err, journal.ErrNoEntry) {
				return false, nil
			}
			return err == nil, err
		}
	}
	var n int
	var err error
	if w.older {
		n, err = w.scan.Previous()
	} else {
		n, err = w.scan.Next()
	}
	return n != 0, err
}

func (w *invocationWalker) next() (invocationInfo, bool, error) {
	if w.done {
		return invocationInfo{}, false, nil
	}
	if w.unrestricted {
		if err := w.resetScan(false); err != nil {
			return invocationInfo{}, false, err
		}
	}
	broken := make(map[string]bool)
	for {
		ok, err := w.step()
		if err != nil || !ok {
			w.done = true
			return invocationInfo{}, false, err
		}
		id, err := primaryInvocationID(w.scan)
		if err != nil {
			return invocationInfo{}, false, err
		}
		if id == "" || id == w.previous || broken[id] {
			continue
		}
		info, anchor, ok, err := w.bound(id)
		if err != nil {
			return invocationInfo{}, false, err
		}
		if !ok {
			broken[id] = true
			_, anchor, found, err := w.bound(w.previous)
			if err != nil {
				return invocationInfo{}, false, err
			}
			if !found {
				return invocationInfo{}, false, fmt.Errorf("Cannot seek to the previous invocation: No data available")
			}
			w.anchor = anchor
			// Native discovery resumes without unit/boot matches after a broken ID.
			if err := w.resetScan(true); err != nil {
				return invocationInfo{}, false, err
			}
			continue
		}
		w.anchor, w.previous = anchor, id
		return info, true, nil
	}
}

func collectInvocationsFromInput(input cliInput, flags *cliFlags, unitOption string) ([]invocationInfo, error) {
	older, max := false, int(^uint(0)>>1)
	if flags.linesFlag.set {
		limit, err := parseLinesLimitValue(flags.linesFlag.value)
		if err != nil {
			return nil, err
		}
		if !limit.all {
			older, max = !limit.oldest, limit.count
		}
	}
	if max == 0 {
		j, err := input.openJournal()
		if err != nil {
			return nil, err
		}
		defer j.Close()
		if unitOption != "" {
			if _, _, err := singleInvocationUnit(j, flags, unitOption); err != nil {
				return nil, err
			}
		}
		_, err = invocationBootID(j, flags)
		return nil, err
	}
	w, err := newInvocationWalker(input, flags, unitOption, older, false)
	if err != nil {
		return nil, err
	}
	defer w.Close()
	var out []invocationInfo
	seen := make(map[string]bool)
	for len(out) < max {
		info, ok, err := w.next()
		if err != nil {
			return nil, err
		}
		if !ok || seen[info.id] {
			break
		}
		seen[info.id] = true
		out = append(out, info)
	}
	if older {
		for i, k := 0, len(out)-1; i < k; i, k = i+1, k-1 {
			out[i], out[k] = out[k], out[i]
		}
	}
	return out, nil
}

func resolveInvocationFilter(input cliInput, flags *cliFlags) (string, bool, error) {
	if !flags.invocationSet {
		return "", false, nil
	}
	if !flags.invocationReady {
		flags.invocationReady = true
		flags.invocationID, flags.invocationActive, flags.invocationError = findInvocationFilter(input, flags)
	}
	return flags.invocationID, flags.invocationActive, flags.invocationError
}

func findInvocationFilter(input cliInput, flags *cliFlags) (string, bool, error) {
	descriptor, set := effectiveInvocationDescriptor(flags)
	if !set {
		return "", false, nil
	}
	id, offset, active, err := parseInvocationDescriptor(descriptor)
	if err != nil || !active {
		return "", false, err
	}
	option := ""
	if id == "" || offset != 0 {
		option = "-I/--invocation= with an offset"
	}
	w, err := newInvocationWalker(input, flags, option, offset <= 0, true)
	if err != nil {
		return "", false, err
	}
	defer w.Close()
	notFound := func() (string, bool, error) {
		return "", false, fmt.Errorf("No journal entry found for the invocation (%s%+d).", id, offset)
	}
	start := 1
	if offset <= 0 {
		start = 0
	}
	if id != "" {
		_, anchor, ok, err := w.bound(id)
		if err != nil {
			return "", false, err
		}
		if !ok {
			return notFound()
		}
		if offset == 0 {
			return id, true, nil
		}
		w.anchor, w.previous = anchor, id
		if offset < 0 {
			start = -1
		}
	}
	for off := start; ; {
		info, ok, err := w.next()
		if err != nil {
			return "", false, err
		}
		if !ok {
			return notFound()
		}
		if off == offset {
			return info.id, true, nil
		}
		if w.older {
			off--
		} else {
			off++
		}
	}
}
