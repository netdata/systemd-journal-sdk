//go:build !unix

package main

import "os"

func openVacuumCandidate(path string) (*os.File, error) {
	return os.Open(path) // nosec G304 - caller explicitly supplied the journal directory.
}
