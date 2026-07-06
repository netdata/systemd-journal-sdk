//go:build linux

package journalhost

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

const (
	testContainerMachineID = "00112233445566778899aabbccddeeff"
	testHostMachineID      = "ffeeddccbbaa99887766554433221100"
	testDBusMachineID      = "0123456789abcdef0123456789abcdef"
)

func TestLinuxMachineIDUsesContainerPathsByDefault(t *testing.T) {
	dir := t.TempDir()
	writeMachineID(t, dir, "etc/machine-id", testContainerMachineID)
	writeMachineID(t, dir, "host/etc/machine-id", testHostMachineID)

	id, source, err := loadLinuxMachineIDFromRoot(dir, "")
	if err != nil {
		t.Fatalf("loadLinuxMachineIDFromRoot() error = %v", err)
	}
	assertMachineID(t, id, testContainerMachineID)
	if source != "linux:/etc/machine-id" {
		t.Fatalf("source = %q, want linux:/etc/machine-id", source)
	}
}

func TestLinuxMachineIDPrefersExplicitHostPrefix(t *testing.T) {
	dir := t.TempDir()
	writeMachineID(t, dir, "etc/machine-id", testContainerMachineID)
	writeMachineID(t, dir, "host/etc/machine-id", testHostMachineID)

	id, source, err := loadLinuxMachineIDFromRoot(dir, "/host")
	if err != nil {
		t.Fatalf("loadLinuxMachineIDFromRoot() error = %v", err)
	}
	assertMachineID(t, id, testHostMachineID)
	if source != "linux:/host/etc/machine-id" {
		t.Fatalf("source = %q, want linux:/host/etc/machine-id", source)
	}
}

func TestLinuxMachineIDFallsBackWhenHostPrefixAbsent(t *testing.T) {
	dir := t.TempDir()
	writeMachineID(t, dir, "var/lib/dbus/machine-id", testDBusMachineID)

	id, source, err := loadLinuxMachineIDFromRoot(dir, "/host")
	if err != nil {
		t.Fatalf("loadLinuxMachineIDFromRoot() error = %v", err)
	}
	assertMachineID(t, id, testDBusMachineID)
	if source != "linux:/var/lib/dbus/machine-id" {
		t.Fatalf("source = %q, want linux:/var/lib/dbus/machine-id", source)
	}
}

func TestLinuxMachineIDChecksHostDBusBeforeContainerPaths(t *testing.T) {
	dir := t.TempDir()
	writeMachineID(t, dir, "etc/machine-id", testContainerMachineID)
	writeMachineID(t, dir, "host/var/lib/dbus/machine-id", testDBusMachineID)

	id, source, err := loadLinuxMachineIDFromRoot(dir, "/host")
	if err != nil {
		t.Fatalf("loadLinuxMachineIDFromRoot() error = %v", err)
	}
	assertMachineID(t, id, testDBusMachineID)
	if source != "linux:/host/var/lib/dbus/machine-id" {
		t.Fatalf("source = %q, want linux:/host/var/lib/dbus/machine-id", source)
	}
}

func TestLinuxMachineIDErrorsOnInvalidExplicitHostPrefixFile(t *testing.T) {
	dir := t.TempDir()
	writeMachineID(t, dir, "etc/machine-id", testContainerMachineID)
	writeText(t, dir, "host/etc/machine-id", "not-a-machine-id\n")

	_, _, err := loadLinuxMachineIDFromRoot(dir, "/host")
	if err == nil {
		t.Fatal("loadLinuxMachineIDFromRoot() error = nil, want invalid host machine-id error")
	}
	if !strings.Contains(err.Error(), "linux:/host/etc/machine-id") {
		t.Fatalf("error = %q, want host source path", err)
	}
}

func TestLinuxMachineIDErrorsOnInvalidFirstHostFileEvenWhenHostDBusExists(t *testing.T) {
	dir := t.TempDir()
	writeText(t, dir, "host/etc/machine-id", "not-a-machine-id\n")
	writeMachineID(t, dir, "host/var/lib/dbus/machine-id", testDBusMachineID)

	_, _, err := loadLinuxMachineIDFromRoot(dir, "/host")
	if err == nil {
		t.Fatal("loadLinuxMachineIDFromRoot() error = nil, want invalid host machine-id error")
	}
	if !strings.Contains(err.Error(), "linux:/host/etc/machine-id") {
		t.Fatalf("error = %q, want host source path", err)
	}
}

func TestLinuxMachineIDRejectsAllZeroHostMachineID(t *testing.T) {
	dir := t.TempDir()
	writeMachineID(t, dir, "host/etc/machine-id", "00000000000000000000000000000000")

	_, _, err := loadLinuxMachineIDFromRoot(dir, "/host")
	if err == nil {
		t.Fatal("loadLinuxMachineIDFromRoot() error = nil, want zero host machine-id error")
	}
	if !strings.Contains(err.Error(), "all zeros") {
		t.Fatalf("error = %q, want all-zero machine-id error", err)
	}
}

func TestLinuxMachineIDEmptyHostPrefixKeepsContainerDefault(t *testing.T) {
	dir := t.TempDir()
	writeMachineID(t, dir, "etc/machine-id", testContainerMachineID)
	writeMachineID(t, dir, "host/etc/machine-id", testHostMachineID)

	id, source, err := loadLinuxMachineIDFromRoot(dir, "")
	if err != nil {
		t.Fatalf("loadLinuxMachineIDFromRoot() error = %v", err)
	}
	assertMachineID(t, id, testContainerMachineID)
	if source != "linux:/etc/machine-id" {
		t.Fatalf("source = %q, want linux:/etc/machine-id", source)
	}
}

func writeMachineID(t *testing.T, root, rel, value string) {
	writeText(t, root, rel, value+"\n")
}

func writeText(t *testing.T, root, rel, value string) {
	t.Helper()
	path := filepath.Join(root, rel)
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll(%s) error = %v", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, []byte(value), 0o644); err != nil {
		t.Fatalf("WriteFile(%s) error = %v", path, err)
	}
}

func assertMachineID(t *testing.T, got journal.UUID, wantText string) {
	t.Helper()
	want, err := journal.ParseUUID(wantText)
	if err != nil {
		t.Fatalf("ParseUUID(%q) error = %v", wantText, err)
	}
	if got != want {
		t.Fatalf("machine id = %s, want %s", got, want)
	}
}
