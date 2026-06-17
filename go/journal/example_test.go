package journal_test

import (
	"log"
	"path/filepath"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

func ExampleCreate() {
	path := filepath.Join("/var/log/journal", "example.journal")

	machineID := journal.UUID{0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f}
	bootID := journal.UUID{0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f}

	w, err := journal.Create(path, journal.Options{MachineID: machineID, BootID: bootID})
	if err != nil {
		log.Fatal(err)
	}
	defer w.Close()

	err = w.Append([]journal.Field{
		journal.StringField("MESSAGE", "plugin started"),
		journal.StringField("PRIORITY", "6"),
		journal.StringField("SYSLOG_IDENTIFIER", "example-plugin"),
	}, journal.EntryOptions{MonotonicUsec: 1})
	if err != nil {
		log.Fatal(err)
	}
}
