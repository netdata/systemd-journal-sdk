package journal

import (
	"encoding/json"
	"errors"
	"io"
	"strings"
)

var (
	ErrUnsupported    = errors.New("operation not supported in pure-Go reader")
	ErrNoEntry        = errors.New("no matching entry")
	ErrCorruptFile    = errors.New("corrupt journal file")
	ErrInvalidCursor  = errors.New("invalid cursor")
	ErrEndOfEntries   = errors.New("end of entries reached")
	ErrStartOfEntries = errors.New("start of entries reached")
)

type sdJournal struct {
	reader interface {
		io.Closer
		GetRealtimeUsec() (uint64, error)
		GetCursor() (string, error)
		TestCursor(string) (bool, error)
		QueryUnique(string) ([][]byte, error)
		EnumerateFields() (map[string]struct{}, error)
		AddMatch(data []byte)
		AddDisjunction()
		AddConjunction()
		FlushMatches()
		SeekHead() error
		SeekTail() error
		Step() (bool, error)
		StepBack() (bool, error)
		GetEntry() (*Entry, error)
	}
	outputMode string
}

func SdJournalOpen(path string, flags int) (*sdJournal, error) {
	if flags != sdJournalFlag {
		return nil, ErrUnsupported
	}

	var r interface {
		io.Closer
		GetRealtimeUsec() (uint64, error)
		GetCursor() (string, error)
		TestCursor(string) (bool, error)
		QueryUnique(string) ([][]byte, error)
		EnumerateFields() (map[string]struct{}, error)
		AddMatch(data []byte)
		AddDisjunction()
		AddConjunction()
		FlushMatches()
		SeekHead() error
		SeekTail() error
		Step() (bool, error)
		StepBack() (bool, error)
		GetEntry() (*Entry, error)
	}

	if isJournalFileName(path) {
		var err error
		r, err = OpenFile(path)
		if err != nil {
			return nil, err
		}
	} else {
		var err error
		r, err = OpenDirectory(path)
		if err != nil {
			return nil, err
		}
	}

	return &sdJournal{reader: r}, nil
}

const (
	sdJournalFlag        = 0
	sdJournalFlagRuntime = 1 << iota
	sdJournalFlagStorage
	sdJournalFlags
	sdJournalFlagCurrentComputer = 1 << iota
)

func SdJournalAddMatch(j *sdJournal, data []byte) error {
	match, err := ParseMatchString(string(data))
	if err != nil {
		return err
	}
	j.reader.AddMatch(match)
	return nil
}

func SdJournalAddDisjunction(j *sdJournal) error {
	j.reader.AddDisjunction()
	return nil
}

func SdJournalAddConjunction(j *sdJournal) error {
	j.reader.AddConjunction()
	return nil
}

func SdJournalFlushMatches(j *sdJournal) error {
	j.reader.FlushMatches()
	return nil
}

func SdJournalNext(j *sdJournal) (int, error) {
	ok, err := j.reader.Step()
	if err != nil {
		if errors.Is(err, errEndOfEntries) {
			return 0, nil
		}
		return 0, err
	}
	if !ok {
		return 0, nil
	}
	return 1, nil
}

func SdJournalNextSkip(j *sdJournal, skip uint64) (int, error) {
	var n int
	for i := uint64(0); i < skip; i++ {
		ok, err := j.reader.Step()
		if err != nil {
			if errors.Is(err, errEndOfEntries) {
				break
			}
			return n, err
		}
		if !ok {
			break
		}
		n++
	}
	return n, nil
}

func SdJournalPrevious(j *sdJournal) (int, error) {
	ok, err := j.reader.StepBack()
	if err != nil {
		if errors.Is(err, errStartOfEntries) {
			return 0, nil
		}
		return 0, err
	}
	if !ok {
		return 0, nil
	}
	return 1, nil
}

func SdJournalPreviousSkip(j *sdJournal, skip uint64) (int, error) {
	var n int
	for i := uint64(0); i < skip; i++ {
		ok, err := j.reader.StepBack()
		if err != nil {
			if errors.Is(err, errStartOfEntries) {
				break
			}
			return n, err
		}
		if !ok {
			break
		}
		n++
	}
	return n, nil
}

func SdJournalSeekHead(j *sdJournal) error {
	return j.reader.SeekHead()
}

func SdJournalSeekTail(j *sdJournal) error {
	return j.reader.SeekTail()
}

func SdJournalGetRealtimeUsec(j *sdJournal) (uint64, error) {
	return j.reader.GetRealtimeUsec()
}

func SdJournalGetCursor(j *sdJournal) (string, error) {
	return j.reader.GetCursor()
}

func SdJournalTestCursor(j *sdJournal, cursor string) (bool, error) {
	return j.reader.TestCursor(cursor)
}

func SdJournalSetOutputMode(j *sdJournal, mode string) {
	j.outputMode = mode
}

func (j *sdJournal) Close() error {
	return j.reader.Close()
}

func (j *sdJournal) AddMatch(data []byte) {
	j.reader.AddMatch(data)
}

func (j *sdJournal) AddDisjunction() {
	j.reader.AddDisjunction()
}

func (j *sdJournal) AddConjunction() {
	j.reader.AddConjunction()
}

func (j *sdJournal) FlushMatches() {
	j.reader.FlushMatches()
}

func (j *sdJournal) Next() (int, error) {
	return SdJournalNext(j)
}

func (j *sdJournal) Previous() (int, error) {
	return SdJournalPrevious(j)
}

func (j *sdJournal) SeekHead() error {
	return SdJournalSeekHead(j)
}

func (j *sdJournal) SeekTail() error {
	return SdJournalSeekTail(j)
}

func (j *sdJournal) SetOutputMode(mode string) {
	SdJournalSetOutputMode(j, mode)
}

func (j *sdJournal) ProcessOutput(entry *Entry) (string, error) {
	return SdJournalProcessOutput(j, entry)
}

func (j *sdJournal) GetEntry() (*Entry, error) {
	return SdJournalGetEntry(j)
}

func (j *sdJournal) ListBoots() ([]BootInfo, error) {
	return SdJournalListBoots(j)
}

func (j *sdJournal) EnumerateFields() ([]string, error) {
	return SdJournalEnumerateFields(j)
}

func SdJournalQueryUnique(j *sdJournal, field string) ([][]string, error) {
	values, err := j.reader.QueryUnique(field)
	if err != nil {
		return nil, err
	}
	result := make([][]string, len(values))
	for i, v := range values {
		result[i] = []string{field, string(v)}
	}
	return result, nil
}

func SdJournalEnumerateUnique(j *sdJournal, field string) ([][]byte, error) {
	return j.reader.QueryUnique(field)
}

func SdJournalEnumerateFields(j *sdJournal) ([]string, error) {
	fields, err := j.reader.EnumerateFields()
	if err != nil {
		return nil, err
	}
	result := make([]string, 0, len(fields))
	for f := range fields {
		result = append(result, f)
	}
	return result, nil
}

func SdJournalGetEntry(j *sdJournal) (*Entry, error) {
	return j.reader.GetEntry()
}

func SdJournalGetEntryWithRealtime(j *sdJournal, realtime uint64) (*Entry, error) {
	originalCursor, originalErr := j.reader.GetCursor()
	if err := j.reader.SeekHead(); err != nil {
		return nil, err
	}
	defer func() {
		if originalErr == nil {
			_ = SdJournalSeekCursor(j, originalCursor)
			return
		}
		_ = j.reader.SeekHead()
	}()

	for {
		ok, err := j.reader.Step()
		if err != nil {
			if errors.Is(err, errEndOfEntries) {
				return nil, ErrNoEntry
			}
			return nil, err
		}
		if !ok {
			return nil, ErrNoEntry
		}

		entry, err := j.reader.GetEntry()
		if err != nil {
			return nil, err
		}

		if entry.Realtime == realtime {
			return entry, nil
		}
	}
}

func SdJournalGetCursorWithRealtime(j *sdJournal, realtime uint64) (string, error) {
	entry, err := SdJournalGetEntryWithRealtime(j, realtime)
	if err != nil {
		return "", err
	}
	return entry.Cursor, nil
}

func SdJournalSeekCursor(j *sdJournal, cursor string) error {
	wantSeqnumID, wantBootID, wantRealtime, wantSeqnum, err := ParseCursor(cursor)
	if err != nil {
		return ErrInvalidCursor
	}

	if err := j.reader.SeekHead(); err != nil {
		return err
	}

	for {
		ok, err := j.reader.Step()
		if err != nil {
			if errors.Is(err, errEndOfEntries) {
				return ErrNoEntry
			}
			return err
		}
		if !ok {
			return ErrNoEntry
		}

		entry, err := j.reader.GetEntry()
		if err != nil {
			return err
		}

		gotSeqnumID, gotBootID, gotRealtime, gotSeqnum, err := ParseCursor(entry.Cursor)
		if err != nil {
			return err
		}
		if gotSeqnumID == wantSeqnumID &&
			gotBootID == wantBootID &&
			gotRealtime == wantRealtime &&
			gotSeqnum == wantSeqnum {
			return nil
		}
	}
}

func SdJournalProcessOutput(j *sdJournal, entry *Entry) (string, error) {
	switch j.outputMode {
	case "export":
		return ExportEntry(entry), nil
	case "json":
		result, err := JSONEntry(entry)
		if err != nil {
			return "", err
		}
		encoded, err := json.Marshal(result)
		if err != nil {
			return "", err
		}
		return string(encoded) + "\n", nil
	default:
		return formatEntryText(entry), nil
	}
}

func formatEntryText(entry *Entry) string {
	var b strings.Builder

	if msg, ok := entry.Fields["MESSAGE"]; ok {
		b.Write(msg)
	}
	b.WriteByte('\n')

	return b.String()
}

func SdJournalListBoots(j *sdJournal) ([]BootInfo, error) {
	if dr, ok := j.reader.(*DirectoryReader); ok {
		return dr.ListBoots()
	}
	return nil, ErrUnsupported
}
