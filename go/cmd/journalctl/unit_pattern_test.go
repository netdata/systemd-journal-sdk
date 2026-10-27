//go:build cgo

package main

import "testing"

func TestUnitPatternFallbackByteGrammar(t *testing.T) {
	for _, test := range []struct {
		pattern, value string
		want           bool
	}{
		{"[[.a.]]lpha.*", "alpha.service", true},
		{"[[]", "[", true},
		{"[[:alpha:]]lpha.*", "alpha.service", true},
		{"[!b]lpha.*", "blpha.service", false},
		{"[]a]*", "]name", true},
		{"alpha[", "alpha[", true},
		{`alpha\x20*`, `alpha\x20name.service`, true},
		{"bad-?.service", "bad-\xff.service", true},
		{"bad-?.service", "bad-é.service", false},
		{"bad-??.service", "bad-é.service", true},
	} {
		matcher, err := newByteUnitMatcher([]string{test.pattern})
		if err != nil {
			t.Fatal(err)
		}
		got := matcher.Matches([]byte(test.value))
		matcher.Close()
		if got != test.want {
			t.Errorf("%q matches %q: got %v, want %v", test.pattern, test.value, got, test.want)
		}
	}
}
