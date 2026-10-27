//go:build cgo && !(linux || darwin || freebsd)

package main

type unitPatternMatcher = byteUnitMatcher

func newUnitPatternMatcher(patterns []string) (*unitPatternMatcher, error) {
	return newByteUnitMatcher(patterns)
}
