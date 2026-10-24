//go:build cgo && !unix

package main

func currentUIDString() (string, bool) {
	return "", false
}
