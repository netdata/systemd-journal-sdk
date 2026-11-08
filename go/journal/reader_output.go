package journal

import (
	"errors"
	"fmt"
	"strconv"
	"strings"
)

func ParseMatchString(s string) ([]byte, error) {
	field, err := parseMatchField(s)
	if err != nil {
		return nil, err
	}
	if err := validateMatchFieldName(field); err != nil {
		return nil, err
	}
	return []byte(s), nil
}

func parseMatchField(s string) (string, error) {
	switch {
	case s == "":
		return "", errors.New("empty match string")
	case s == "=":
		return "", errors.New("invalid match: missing field name")
	case strings.HasPrefix(s, "="):
		return "", errors.New("invalid match: field name cannot start with =")
	}
	eq := strings.IndexByte(s, '=')
	if eq < 0 {
		return "", errors.New("invalid match: missing '=' separator")
	}
	return s[:eq], nil
}

func validateMatchFieldName(field string) error {
	if field == "" {
		return errors.New("invalid match: empty field name")
	}

	if len(field) > 64 {
		return errors.New("invalid match: field name too long")
	}

	if field[0] >= '0' && field[0] <= '9' {
		return fmt.Errorf("invalid field name %q", field)
	}
	for _, c := range field {
		if c == '_' || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') {
			continue
		}
		return fmt.Errorf("invalid field name %q", field)
	}

	return nil
}

type parsedCursorLocation struct {
	seqnumID     string
	seqnum       uint64
	seqnumSet    bool
	bootID       string
	monotonic    uint64
	monotonicSet bool
	realtime     uint64
	realtimeSet  bool
	xorHash      uint64
	xorHashSet   bool
}

func ParseCursor(cursor string) (seqnumID string, bootID string, realtime uint64, seqnum uint64, err error) {
	location, err := parseCursorLocation(cursor, true)
	if err != nil {
		return "", "", 0, 0, err
	}
	return location.seqnumID, location.bootID, location.realtime, location.seqnum, nil
}

func parseCursorLocation(cursor string, requireSeekComponent bool) (parsedCursorLocation, error) {
	parts := strings.Split(cursor, ";")
	values := make(map[string]string, len(parts))
	for _, part := range parts {
		key, value, ok := strings.Cut(part, "=")
		if !ok || key == "" || value == "" {
			return parsedCursorLocation{}, errors.New("invalid cursor format")
		}
		values[key] = value
	}

	var location parsedCursorLocation
	location.seqnumID = normalizeCursorID(values["s"])

	if values["j"] != "" || values["c"] != "" || values["n"] != "" {
		location.bootID = normalizeCursorID(values["j"])
		if location.seqnumID == "" || location.bootID == "" || values["c"] == "" || values["n"] == "" {
			return parsedCursorLocation{}, errors.New("invalid cursor format")
		}
		realtime, err := strconv.ParseUint(values["c"], 16, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad realtime")
		}
		seqnum, err := strconv.ParseUint(values["n"], 10, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad seqnum")
		}
		location.realtime = realtime
		location.realtimeSet = true
		location.seqnum = seqnum
		location.seqnumSet = true
		return location, nil
	}

	location.bootID = normalizeCursorID(values["b"])
	if values["t"] != "" {
		realtime, err := strconv.ParseUint(values["t"], 16, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad realtime")
		}
		location.realtime = realtime
		location.realtimeSet = true
	}
	if values["i"] != "" {
		seqnum, err := strconv.ParseUint(values["i"], 16, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad seqnum")
		}
		location.seqnum = seqnum
		location.seqnumSet = true
	}
	if values["m"] != "" {
		monotonic, err := strconv.ParseUint(values["m"], 16, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad monotonic")
		}
		location.monotonic = monotonic
		location.monotonicSet = true
	}
	if values["x"] != "" {
		xorHash, err := strconv.ParseUint(values["x"], 16, 64)
		if err != nil {
			return parsedCursorLocation{}, errors.New("invalid cursor format: bad xor hash")
		}
		location.xorHash = xorHash
		location.xorHashSet = true
	}

	hasSeqnumCursor := location.seqnumID != "" && location.seqnumSet
	hasMonotonicCursor := location.bootID != "" && location.monotonicSet
	hasRealtimeCursor := location.realtimeSet
	if requireSeekComponent && !hasSeqnumCursor && !hasMonotonicCursor && !hasRealtimeCursor {
		return parsedCursorLocation{}, errors.New("invalid cursor format")
	}
	if !requireSeekComponent &&
		location.seqnumID == "" &&
		!location.seqnumSet &&
		location.bootID == "" &&
		!location.monotonicSet &&
		!location.realtimeSet &&
		!location.xorHashSet {
		return parsedCursorLocation{}, errors.New("invalid cursor format")
	}

	return location, nil
}

func normalizeCursorID(value string) string {
	return strings.ToLower(strings.ReplaceAll(value, "-", ""))
}

func cursorLocationMatches(got, want parsedCursorLocation) bool {
	matched := false
	if want.seqnumID != "" {
		if got.seqnumID != want.seqnumID {
			return false
		}
		matched = true
	}
	if want.seqnumSet {
		if !got.seqnumSet || got.seqnum != want.seqnum {
			return false
		}
		matched = true
	}
	if want.bootID != "" {
		if got.bootID != want.bootID {
			return false
		}
		matched = true
	}
	if want.monotonicSet {
		if !got.monotonicSet || got.monotonic != want.monotonic {
			return false
		}
		matched = true
	}
	if want.realtimeSet {
		if !got.realtimeSet || got.realtime != want.realtime {
			return false
		}
		matched = true
	}
	if want.xorHashSet {
		if !got.xorHashSet || got.xorHash != want.xorHash {
			return false
		}
		matched = true
	}
	return matched
}

func cursorLocationAtOrAfter(got, want parsedCursorLocation) bool {
	if want.seqnumID != "" && want.seqnumSet && got.seqnumID == want.seqnumID {
		if got.seqnum != want.seqnum {
			return got.seqnum > want.seqnum
		}
	}
	if want.bootID != "" && want.monotonicSet && got.bootID == want.bootID {
		if got.monotonic != want.monotonic {
			return got.monotonic > want.monotonic
		}
	}
	if want.realtimeSet {
		if got.realtime != want.realtime {
			return got.realtime > want.realtime
		}
	}
	// systemd uses x= for exact cursor tests, but seek-cursor positioning
	// treats sequence/monotonic/realtime components as the ordering key.
	// A cursor with a mismatched x= still seeks to the row matching the
	// other components; TestCursor remains exact and checks x= above.
	return true
}
