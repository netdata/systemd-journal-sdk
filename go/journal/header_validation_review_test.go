package journal

import (
	"os"
	"path/filepath"
	"testing"
)

func stableHeaderFixture(t *testing.T, compact bool) (journalHeader, uint64) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "header.journal")
	opts := testOptions()
	opts.Compact = compact
	w, err := Create(path, opts)
	if err != nil {
		t.Fatal(err)
	}
	if err = w.Append([]Field{StringField("MESSAGE", "header")}, EntryOptions{RealtimeUsec: 1_000_000, MonotonicUsec: 1}); err != nil {
		t.Fatal(err)
	}
	if err = w.Close(); err != nil {
		t.Fatal(err)
	}
	h, err := readJournalHeader(path)
	if err != nil {
		t.Fatal(err)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	return h, uint64(info.Size())
}

func TestStableHeaderRejectsPopulationContradictions(t *testing.T) {
	cases := []struct {
		name string
		edit func(*journalHeader)
	}{
		{"objects exceed arena", func(h *journalHeader) { h.nObjects = h.arenaSize/objectHeaderSize + 1 }},
		{"entries exceed objects", func(h *journalHeader) { h.nEntries = h.nObjects + 1 }},
		{"data exceed objects", func(h *journalHeader) { h.nData = h.nObjects + 1 }},
		{"fields exceed objects", func(h *journalHeader) { h.nFields = h.nObjects + 1 }},
		{"tags exceed objects", func(h *journalHeader) { h.nTags = h.nObjects + 1 }},
		{"arrays exceed objects", func(h *journalHeader) { h.nEntryArrays = h.nObjects + 1 }},
		{"aggregate entries exceed objects", func(h *journalHeader) { h.nEntries = h.nObjects }},
		{"aggregate data exceed objects", func(h *journalHeader) { h.nData = h.nObjects }},
		{"aggregate fields exceed objects", func(h *journalHeader) { h.nFields = h.nObjects }},
		{"aggregate tags exceed objects", func(h *journalHeader) { h.nTags = h.nObjects }},
		{"aggregate arrays exceed objects", func(h *journalHeader) { h.nEntryArrays = h.nObjects }},
		{"declared hash tables exceed objects", func(h *journalHeader) { h.nData++ }},
		{"cached count exceeds entries", func(h *journalHeader) { h.tailEntryArrayNEntries = uint32(h.nEntries + 1) }},
		{"missing cached offset", func(h *journalHeader) { h.tailEntryArrayOffset = 0 }},
		{"cached count without arrays", func(h *journalHeader) { h.entryArrayOffset, h.tailEntryArrayOffset = 0, 0 }},
		{"missing cached count", func(h *journalHeader) { h.tailEntryArrayNEntries = 0 }},
		{"missing first array", func(h *journalHeader) { h.entryArrayOffset = 0 }},
		{"tail array before first", func(h *journalHeader) { h.tailEntryArrayOffset = uint32(h.entryArrayOffset - 8) }},
	}
	for _, compact := range []bool{false, true} {
		original, size := stableHeaderFixture(t, compact)
		if _, err := original.validateDeclaredArena(size); err != nil {
			t.Fatal(err)
		}
		for _, tc := range cases {
			t.Run(tc.name, func(t *testing.T) {
				h := original
				tc.edit(&h)
				if _, err := h.validateDeclaredArena(size); err == nil {
					t.Fatalf("accepted contradictory header (compact=%v)", compact)
				}
			})
		}
	}
}

func TestStableHeaderHistoricalPopulationGates(t *testing.T) {
	original, size := stableHeaderFixture(t, false)
	for _, tc := range []struct {
		end  uint64
		edit func(*journalHeader)
	}{
		{216, func(h *journalHeader) { h.nData = h.nObjects }},
		{224, func(h *journalHeader) { h.nFields = h.nObjects }},
		{232, func(h *journalHeader) { h.nTags = h.nObjects }},
		{240, func(h *journalHeader) { h.nEntryArrays = h.nObjects }},
		{264, func(h *journalHeader) { h.tailEntryArrayNEntries = uint32(h.nEntries + 1) }},
	} {
		h := original
		tc.edit(&h)
		h.headerSize = tc.end - 8
		if _, err := h.validateDeclaredArena(size); err != nil {
			t.Fatalf("absent field ending at %d was checked: %v", tc.end, err)
		}
		h.headerSize = tc.end
		if _, err := h.validateDeclaredArena(size); err == nil {
			t.Fatalf("present field ending at %d was not checked", tc.end)
		}
	}
}

func TestStableHeaderAcceptsPopulationBoundaries(t *testing.T) {
	h, size := stableHeaderFixture(t, false)
	h.nObjects = h.arenaSize / objectHeaderSize
	h.nData = h.nObjects - h.nEntries - h.nFields - h.nTags - h.nEntryArrays - 2
	if _, err := h.validateDeclaredArena(size); err != nil {
		t.Fatalf("rejected equal aggregate bound: %v", err)
	}
	h.entryArrayOffset, h.tailEntryArrayOffset, h.tailEntryArrayNEntries = 0, 0, 0
	h.nEntries = 0
	if _, err := h.validateDeclaredArena(size); err != nil {
		t.Fatalf("rejected absent array pair: %v", err)
	}
}

// Both table declarations are in the oldest supported header. Later category
// counters consume the budget only when their complete fields are present.
func TestStableHeaderHashTablePopulationBoundaries(t *testing.T) {
	for _, compact := range []bool{false, true} {
		original, size := stableHeaderFixture(t, compact)
		for _, headerSize := range []uint64{headerMinSize, original.headerSize} {
			for tables := 0; tables < 4; tables++ {
				h := original
				h.headerSize = headerSize
				if headerSize == headerMinSize {
					h.nObjects = 3 // One entry plus the two declared tables.
				}
				if tables&1 == 0 {
					h.dataHashTableOffset, h.dataHashTableSize = 0, 0
					h.nObjects--
				}
				if tables&2 == 0 {
					h.fieldHashTableOffset, h.fieldHashTableSize = 0, 0
					h.nObjects--
				}
				if _, err := h.validateDeclaredArena(size); err != nil {
					t.Fatalf("rejected exact budget: compact=%v header=%d tables=%d: %v", compact, headerSize, tables, err)
				}
				h.nObjects--
				if _, err := h.validateDeclaredArena(size); err == nil {
					t.Errorf("accepted insufficient budget: compact=%v header=%d tables=%d", compact, headerSize, tables)
				}
			}
		}
	}
}
