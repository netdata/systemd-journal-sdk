//go:build !unix

package main

func currentUIDString() (string, bool) {
	return "", false
}
