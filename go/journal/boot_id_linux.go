//go:build linux

package journal

// Host identity discovery is intentionally kept out of core writer code.
// Optional identity helpers should live in a separate public API, not here.
