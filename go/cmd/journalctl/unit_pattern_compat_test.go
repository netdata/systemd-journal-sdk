//go:build cgo && (linux || darwin || freebsd)

package main

import "testing"

func TestUnitPatternCompatibility(t *testing.T) {
	for _, locale := range []string{"C", "en_US.UTF-8"} {
		t.Run(locale, func(t *testing.T) {
			t.Setenv("LC_ALL", locale)
			for _, tc := range []struct {
				pattern, value string
				want           bool
			}{
				{"[[:alpha:]]", "é", locale != "C"},
				{"[[=a=]]", "à", locale != "C"},
				{"[[=a=]]", "A", locale != "C"},
				{"[[=a=]]?", "à", locale != "C"},
				{"[[=a=]-z]", "-", true},
				{"[[:alpha:]]lpha.*", "alpha.service", true},
				{"[[:alpha:]]lpha.*", "1lpha.service", false},
				{"[![:alpha:]]*", "1.service", true},
				{"[![:alpha:]]*", "alpha.service", false},
				{"[[.a.]]lpha.*", "alpha.service", true},
				{"[[=a=]]lpha.*", "alpha.service", true},
				{"[[:alpha:]]*", "a\xff.service", true},
				{"[![:alpha:]]?", "é", true},
				{"*[[:alpha:]]*", "\xffa\xff", true},
				{"[[:alpha:]\\]", "\\", true},
				{"[[:alpha:]]alpha[", "aalpha[", true},
				{"[[:alpha:]]$", "a$", true},
				{"[[:alpha:]]?", "a\n", true},
				{"[[:alpha:]]", "a\n", false},
				{"[[:unknown:]]*", "a", false},
				{"[[:<:]]*", "a", false},
				{"[[.period.]]*", ".", false},
				{"[[:alpha:]", "a", false},
				{"alpha.*", "alpha.service", true},
				{"[[[:bogus:]]", "[", true},
				{"[a-[:alpha:]]", "a]", true},
				{"[[:alpha:]z-a]", "a", true},
				{"[[[:<:]]", "[", false},
				{"[[:-", "[[:-", false},
				{"[[[.a]", "[", false},
				{"[[[=a", "[[[=a", false},
			} {
				m, err := newUnitPatternMatcherWithCompatibility([]string{tc.pattern}, true)
				if err != nil {
					t.Fatal(err)
				}
				got := m.Matches([]byte(tc.value))
				m.Close()
				if got != tc.want {
					t.Errorf("pattern %q value %q: got %v, want %v", tc.pattern, tc.value, got, tc.want)
				}
			}
		})
	}
	t.Setenv("LC_ALL", "en_US.UTF-8")
	native, err := newUnitPatternMatcherWithCompatibility([]string{"[a-z]"}, false)
	if err != nil {
		t.Fatal(err)
	}
	defer native.Close()
	compat, err := newUnitPatternMatcherWithCompatibility([]string{"[[:digit:]a-z]"}, true)
	if err != nil {
		t.Fatal(err)
	}
	defer compat.Close()
	for _, value := range []string{"a", "A", "é", "É", "à", "z", "\xff"} {
		if compat.Matches([]byte(value)) != native.Matches([]byte(value)) {
			t.Errorf("range differs from native matching for %q", value)
		}
	}

}
