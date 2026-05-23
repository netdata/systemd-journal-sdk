package journal_test

import (
	"log"
	"path/filepath"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

func ExampleCreate() {
	path := filepath.Join("/var/log/journal", "example.journal")

	w, err := journal.Create(path, journal.Options{})
	if err != nil {
		log.Fatal(err)
	}
	defer w.Close()

	err = w.Append([]journal.Field{
		journal.StringField("MESSAGE", "plugin started"),
		journal.StringField("PRIORITY", "6"),
		journal.StringField("SYSLOG_IDENTIFIER", "example-plugin"),
	}, journal.EntryOptions{})
	if err != nil {
		log.Fatal(err)
	}
}
