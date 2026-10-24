//go:build cgo

package main

import (
	"errors"
	"fmt"
	"math"
	"strconv"
	"strings"
	"time"
	_ "time/tzdata"
)

const timestampMax = uint64(253402214399000000)

type timestampParser struct {
	localGuess, utcGuess int64
	local                *timestampZone
}

func durationValue(value string) (uint64, error) {
	original := value
	value = strings.TrimLeft(value, " \t\r\n\v\f")
	if strings.TrimRight(value, " \t\r\n\v\f") == "infinity" {
		return math.MaxUint64, nil
	}
	units := []struct {
		name       string
		multiplier uint64
	}{
		{"seconds", 1000000}, {"second", 1000000}, {"sec", 1000000}, {"s", 1000000},
		{"minutes", 60000000}, {"minute", 60000000}, {"min", 60000000},
		{"months", 2629800000000}, {"month", 2629800000000}, {"M", 2629800000000},
		{"msec", 1000}, {"ms", 1000}, {"m", 60000000},
		{"hours", 3600000000}, {"hour", 3600000000}, {"hr", 3600000000}, {"h", 3600000000},
		{"days", 86400000000}, {"day", 86400000000}, {"d", 86400000000},
		{"weeks", 604800000000}, {"week", 604800000000}, {"w", 604800000000},
		{"years", 31557600000000}, {"year", 31557600000000}, {"y", 31557600000000},
		{"usec", 1}, {"us", 1}, {"μs", 1}, {"µs", 1},
	}
	var total uint64
	seen := false
	bad := func() (uint64, error) { return 0, fmt.Errorf("failed to parse duration: %s", original) }
	for {
		value = strings.TrimLeft(value, " \t\r\n\v\f")
		if value == "" {
			if !seen {
				return bad()
			}
			return total, nil
		}
		if value[0] == '-' {
			return bad()
		}
		n := 0
		if value[0] == '+' {
			n++
		}
		digits := n
		for n < len(value) && value[n] >= '0' && value[n] <= '9' {
			n++
		}
		var integer uint64
		if n > digits {
			parsed, err := strconv.ParseUint(value[digits:n], 10, 63)
			if err != nil {
				return bad()
			}
			integer = parsed
		} else if n > 0 || n >= len(value) || value[n] != '.' {
			return bad()
		}
		fraction := ""
		if n < len(value) && value[n] == '.' {
			n++
			start := n
			for n < len(value) && value[n] >= '0' && value[n] <= '9' {
				n++
			}
			if n == start {
				return bad()
			}
			fraction = value[start:n]
		}
		afterNumber := value[n:]
		suffix := strings.TrimLeft(afterNumber, " \t\r\n\v\f")
		multiplier := uint64(1000000)
		matched := false
		for _, unit := range units {
			if strings.HasPrefix(suffix, unit.name) {
				multiplier = unit.multiplier
				suffix = suffix[len(unit.name):]
				matched = true
				break
			}
		}
		if !matched && len(afterNumber) == len(suffix) && suffix != "" {
			return bad()
		}
		if integer >= math.MaxUint64/multiplier {
			return bad()
		}
		amount := integer * multiplier
		if amount >= math.MaxUint64-total {
			return bad()
		}
		total += amount
		weight := multiplier / 10
		for _, digit := range []byte(fraction) {
			amount = uint64(digit-'0') * weight
			if amount >= math.MaxUint64-total {
				return bad()
			}
			total += amount
			weight /= 10
		}
		value = suffix
		seen = true
	}
}

func parseUTCOffset(value string) (int64, bool) {
	if value == "Z" {
		return 0, true
	}
	if len(value) != 3 && len(value) != 5 && len(value) != 6 {
		return 0, false
	}
	if value[0] != '+' && value[0] != '-' {
		return 0, false
	}
	parse2 := func(s string) (int64, bool) {
		if len(s) != 2 || s[0] < '0' || s[0] > '9' || s[1] < '0' || s[1] > '9' {
			return 0, false
		}
		return int64(s[0]-'0')*10 + int64(s[1]-'0'), true
	}
	h, ok := parse2(value[1:3])
	if !ok {
		return 0, false
	}
	var m int64
	if len(value) > 3 {
		if len(value) == 6 {
			if value[3] != ':' {
				return 0, false
			}
			m, ok = parse2(value[4:])
		} else {
			m, ok = parse2(value[3:])
		}
		if !ok || m >= 60 {
			return 0, false
		}
	}
	offset := h*3600 + m*60
	if offset > 86400 {
		return 0, false
	}
	if value[0] == '-' {
		offset = -offset
	}
	return offset, true
}

func (p *timestampParser) parse(value string) (uint64, error) {
	if strings.HasPrefix(value, "@") {
		return durationValue(value[1:])
	}
	if p.local == nil {
		p.local = localTimestampZone()
	}
	if len(value) > 2 && strings.HasSuffix(value, "Z") {
		if result, err := p.parseInZone(value, value[:len(value)-1], p.local, true, -1, 0, true); err == nil {
			return result, nil
		}
	}
	if len(value) > 7 && value[len(value)-7] != ' ' && (value[len(value)-6] == '+' || value[len(value)-6] == '-') {
		if offset, ok := parseUTCOffset(value[len(value)-6:]); ok {
			return p.parseInZone(value, value[:len(value)-6], p.local, true, -1, offset, true)
		}
	}
	if n := strings.LastIndexByte(value, ' '); n >= 0 {
		prefix, suffix := value[:n], value[n+1:]
		if suffix == "UTC" {
			return p.parseInZone(value, prefix, p.local, true, -1, 0, true)
		}
		if offset, ok := parseUTCOffset(suffix); ok {
			return p.parseInZone(value, prefix, p.local, true, -1, offset, true)
		}
		if dst, ok := p.local.abbreviation(suffix); ok {
			return p.parseInZone(value, prefix, p.local, false, dst, 0, true)
		}
		if zone, ok := namedTimestampZone(suffix); ok {
			result, err := p.parseInZone(value, prefix, zone, false, -1, 0, true)
			p.local.restoreAfterNamed(zone)
			return result, err
		}
	}
	return p.parseInZone(value, value, p.local, false, -1, 0, false)
}

func (p *timestampParser) parseInZone(value, text string, zone *timestampZone, utc bool, isdst int, offset int64, withZone bool) (uint64, error) {
	bad := func() (uint64, error) { return 0, fmt.Errorf("failed to parse timestamp: %s", value) }
	now := time.Now()
	if !withZone {
		var delta uint64
		var err error
		relative := false
		negative := false
		switch {
		case value == "now":
			return uint64(now.UnixMicro()), nil
		case strings.HasPrefix(value, "+"):
			delta, err = durationValue(value[1:])
			relative = true
		case strings.HasPrefix(value, "-"):
			delta, err = durationValue(value[1:])
			relative = true
			negative = true
		case strings.HasSuffix(value, " ago"):
			delta, err = durationValue(value[:len(value)-4])
			relative = true
			negative = true
		case strings.HasSuffix(value, " left"):
			delta, err = durationValue(value[:len(value)-5])
			relative = true
		}
		if relative {
			if err != nil {
				return bad()
			}
			base := uint64(now.UnixMicro())
			if negative {
				if delta > base {
					return bad()
				}
				base -= delta
			} else {
				if delta > timestampMax-base {
					return bad()
				}
				base += delta
			}
			if base > timestampMax {
				return bad()
			}
			return base, nil
		}
	}
	seed, _, _ := zone.civil(now.Unix(), utc)
	var civil time.Time
	var second int
	var fraction uint64
	var weekday = -1
	switch text {
	case "today", "yesterday", "tomorrow":
		d := seed.Day()
		if text == "yesterday" {
			d--
		}
		if text == "tomorrow" {
			d++
		}
		civil = time.Date(seed.Year(), seed.Month(), d, 0, 0, 0, 0, time.UTC)
	default:
		for i, name := range []string{"Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"} {
			for _, prefix := range []string{name, name[:3]} {
				if len(text) > len(prefix) && strings.EqualFold(text[:len(prefix)], prefix) && text[len(prefix)] == ' ' {
					weekday = i
					text = text[len(prefix)+1:]
					break
				}
			}
			if weekday >= 0 {
				break
			}
		}
		var ok bool
		civil, second, fraction, ok = parseCalendar(text, seed)
		if !ok {
			return bad()
		}
	}
	// strptime checks the civil date's weekday before mktime normalizes it.
	if weekday >= 0 && int(civil.Weekday()) != weekday {
		return bad()
	}
	guess := &p.localGuess
	if utc {
		guess = &p.utcGuess
	}
	seconds, err := inverseCivil(civil, zone, utc, isdst, guess)
	if err != nil {
		return bad()
	}
	seconds += int64(second) - int64(civil.Second())
	seconds -= offset
	if seconds < 0 {
		return bad()
	}
	usec := uint64(seconds)*1000000 + fraction
	if usec > timestampMax {
		return bad()
	}
	return usec, nil
}

func parseCalendar(value string, seed time.Time) (time.Time, int, uint64, bool) {
	formats := []string{"%y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%b %d %H:%M:%S", "%y-%m-%d %H:%M", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M", "%y-%m-%d", "%Y-%m-%d", "%H:%M:%S", "%H:%M"}
	for _, format := range formats {
		year, month, day := seed.Year(), int(seed.Month()), seed.Day()
		hour, minute, second := seed.Hour(), seed.Minute(), seed.Second()
		s := value
		valid := true
		hasTime := strings.Contains(format, "%H")
		hasSeconds := strings.Contains(format, "%S")
		if !hasTime {
			hour, minute, second = 0, 0, 0
		} else if !hasSeconds {
			second = 0
		}
		for i := 0; i < len(format) && valid; i++ {
			c := format[i]
			if c == ' ' {
				s = strings.TrimLeft(s, " \t\r\n\v\f")
				continue
			}
			if c != '%' {
				if len(s) == 0 || s[0] != c {
					valid = false
				} else {
					s = s[1:]
				}
				continue
			}
			i++
			c = format[i]
			if c == 'b' {
				found := false
				for m, name := range []string{"January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"} {
					for _, prefix := range []string{name, name[:3]} {
						if len(s) >= len(prefix) && strings.EqualFold(s[:len(prefix)], prefix) {
							month = m + 1
							s = s[len(prefix):]
							found = true
							break
						}
					}
					if found {
						break
					}
				}
				valid = found
				continue
			}
			s = strings.TrimLeft(s, " \t\r\n\v\f")
			width := 2
			if c == 'Y' {
				width = 4
			}
			n := 0
			for n < len(s) && n < width && s[n] >= '0' && s[n] <= '9' {
				n++
			}
			if n == 0 {
				valid = false
				break
			}
			number, _ := strconv.Atoi(s[:n])
			s = s[n:]
			switch c {
			case 'y':
				if number >= 69 {
					year = 1900 + number
				} else {
					year = 2000 + number
				}
			case 'Y':
				year = number
			case 'm':
				month = number
				valid = number >= 1 && number <= 12
			case 'd':
				day = number
				valid = number >= 1 && number <= 31
			case 'H':
				hour = number
				valid = number <= 23
			case 'M':
				minute = number
				valid = number <= 59
			case 'S':
				second = number
				valid = number <= 61
			}
		}
		var fractional uint64
		if valid && hasSeconds && strings.HasPrefix(s, ".") {
			s = s[1:]
			if s == "" {
				valid = false
			}
			for i, c := range []byte(s) {
				if c < '0' || c > '9' {
					valid = false
					break
				}
				if i < 6 {
					fractional = fractional*10 + uint64(c-'0')
				} else if i == 6 && c >= '5' {
					fractional++
				}
			}
			if len(s) < 6 {
				for i := len(s); i < 6; i++ {
					fractional *= 10
				}
			}
			s = ""
		}
		if valid && s == "" {
			clamped := second
			if clamped > 59 {
				clamped = 59
			}
			return time.Date(year, time.Month(month), day, hour, minute, clamped, 0, time.UTC), second, fractional, true
		}
	}
	return time.Time{}, 0, 0, false
}

// Invert UTC-to-local probes with the cached-offset and oscillation policy used
// by glibc mktime. A fixed earlier/later DST preference cannot reproduce it.
func inverseCivil(civil time.Time, zone *timestampZone, utc bool, isdst int, guess *int64) (int64, error) {
	target := civil.Unix()
	t := target + *guess
	t1, t2 := t, t
	previousDST := -1
	matched, oscillated := false, false
	for probe := 0; probe < 6; probe++ {
		local, dst, _ := zone.civil(t, utc)
		actual := local.Unix()
		diff := target - actual
		if diff == 0 {
			matched = true
			break
		}
		if t == t1 && t != t2 && (dst < 0 || isdst < 0 && previousDST <= dst || isdst >= 0 && dst != isdst) {
			matched = true
			oscillated = true
			break
		}
		t1, t2 = t2, t
		previousDST = dst
		t += diff
	}
	if !matched {
		return 0, errors.New("timestamp conversion did not converge")
	}
	_, dst, _ := zone.civil(t, utc)
	if isdst >= 0 && dst != isdst && !oscillated {
		found := false
		for delta := int64(601200); delta < 457243209/2+601200 && !found; delta += 601200 {
			for _, direction := range []int64{-1, 1} {
				candidate := t + direction*delta
				probe, otherDST, _ := zone.civil(candidate, utc)
				if otherDST != isdst {
					continue
				}
				t = candidate + target - probe.Unix()
				found = true
				break
			}
		}
		if !found {
			t += int64(dst-isdst) * 3600
		}
		zone.civil(t, utc)
	}
	*guess = t - target
	// An inserted second can falsely match the following minute's zero second.
	if civil.Second() == 0 && zone.leapHit(t) {
		actual, _, _ := zone.civil(t, utc)
		if actual.Second() == 0 {
			t++
			zone.civil(t, utc)
		}
	}
	return t, nil
}
