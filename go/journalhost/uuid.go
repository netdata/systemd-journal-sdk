package journalhost

import (
	"fmt"
	"strings"

	"github.com/netdata/systemd-journal-sdk/go/journal"
)

// parseUUIDText parses a 32-character or dashed 36-character UUID string.
func parseUUIDText(text string) (journal.UUID, error) {
	id, err := journal.ParseUUID(text)
	if err != nil {
		return journal.UUID{}, fmt.Errorf("parse UUID: %w", err)
	}
	return id, nil
}

func isZeroUUID(id journal.UUID) bool {
	return id == journal.UUID{}
}

func trimTrailingNul(s string) string {
	return strings.TrimRight(s, "\x00")
}
