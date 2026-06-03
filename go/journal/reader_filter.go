package journal

import (
	"bytes"
	"sort"
)

type filterBuilder struct {
	level0  []filterExpr
	level1  []filterExpr
	current [][]byte
}

type filterExpr interface {
	matches(*Entry) bool
}

type matchExpr struct {
	field string
	value []byte
}

type andExpr []filterExpr

type orExpr []filterExpr

type falseExpr struct{}

func (f *filterBuilder) addMatch(data []byte) {
	f.current = append(f.current, append([]byte(nil), data...))
}

func (f *filterBuilder) addDisjunction() {
	f.commitCurrent()
}

func (f *filterBuilder) addConjunction() {
	f.commitCurrent()
	f.commitLevel1()
}

func (f *filterBuilder) matches(entry *Entry) bool {
	expr := f.finalExpr()
	if expr == nil {
		return true
	}
	return expr.matches(entry)
}

func (f *filterBuilder) commitCurrent() {
	if expr := buildCurrentFilterExpr(f.current); expr != nil {
		f.level1 = append(f.level1, expr)
	}
	f.current = nil
}

func (f *filterBuilder) commitLevel1() {
	if expr := buildLevel1FilterExpr(f.level1); expr != nil {
		f.level0 = append(f.level0, expr)
	}
	f.level1 = nil
}

func (f *filterBuilder) finalExpr() filterExpr {
	level0 := append([]filterExpr(nil), f.level0...)
	level1 := append([]filterExpr(nil), f.level1...)
	if expr := buildCurrentFilterExpr(f.current); expr != nil {
		level1 = append(level1, expr)
	}
	if expr := buildLevel1FilterExpr(level1); expr != nil {
		level0 = append(level0, expr)
	}
	if len(level0) == 0 {
		return nil
	}
	if len(level0) == 1 {
		return level0[0]
	}
	return andExpr(level0)
}

func buildLevel1FilterExpr(level1 []filterExpr) filterExpr {
	if len(level1) == 0 {
		return nil
	}
	if len(level1) == 1 {
		return level1[0]
	}
	return orExpr(level1)
}

func buildCurrentFilterExpr(matches [][]byte) filterExpr {
	if len(matches) == 0 {
		return nil
	}
	byField := make(map[string][]filterExpr)
	var fields []string
	for _, item := range matches {
		eq := bytes.IndexByte(item, '=')
		if eq < 0 {
			return falseExpr{}
		}
		field := string(item[:eq])
		if _, ok := byField[field]; !ok {
			fields = append(fields, field)
		}
		byField[field] = append(byField[field], matchExpr{
			field: field,
			value: append([]byte(nil), item[eq+1:]...),
		})
	}
	sort.Strings(fields)

	parts := make([]filterExpr, 0, len(fields))
	for _, field := range fields {
		values := byField[field]
		if len(values) == 1 {
			parts = append(parts, values[0])
		} else {
			parts = append(parts, orExpr(values))
		}
	}
	if len(parts) == 1 {
		return parts[0]
	}
	return andExpr(parts)
}

func (m matchExpr) matches(entry *Entry) bool {
	if entry.FieldValues != nil {
		for _, value := range entry.FieldValues[m.field] {
			if bytes.Equal(value, m.value) {
				return true
			}
		}
		return false
	}
	value, ok := entry.Fields[m.field]
	return ok && bytes.Equal(value, m.value)
}

func (a andExpr) matches(entry *Entry) bool {
	for _, expr := range a {
		if !expr.matches(entry) {
			return false
		}
	}
	return true
}

func (o orExpr) matches(entry *Entry) bool {
	for _, expr := range o {
		if expr.matches(entry) {
			return true
		}
	}
	return false
}

func (falseExpr) matches(*Entry) bool {
	return false
}
