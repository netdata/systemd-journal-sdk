package journal

import (
	"encoding/json"
	"errors"
	"io"
	"sort"
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

type sdReader interface {
	io.Closer
	GetRealtimeUsec() (uint64, error)
	GetCursor() (string, error)
	TestCursor(string) (bool, error)
	QueryUnique(string) ([][]byte, error)
	VisitUnique(string, func([]byte) error) error
	EnumerateFields() (map[string]struct{}, error)
	AddMatch(data []byte)
	AddDisjunction()
	AddConjunction()
	FlushMatches()
	SeekHead() error
	SeekTail() error
	SeekRealtimeUsec(uint64) error
	Step() (bool, error)
	StepBack() (bool, error)
	GetEntry() (*Entry, error)
	GetEntryPayload([]byte) ([]byte, bool, error)
	EntryDataRestart() error
	EnumerateEntryPayload() ([]byte, bool, error)
	ClearEntryDataState()
}

type sdJournal struct {
	reader      sdReader
	outputMode  string
	dataItems   [][]byte
	dataIndex   int
	readerData  bool
	fieldItems  []string
	fieldIndex  int
	uniqueItems [][]byte
	uniqueIndex int
}

type UniqueValue struct {
	Field string
	Value []byte
}

func SdJournalOpen(path string, flags int) (*sdJournal, error) {
	return SdJournalOpenWithOptions(path, flags, DefaultReaderOptions())
}

func SdJournalOpenWithOptions(path string, flags int, opts ReaderOptions) (*sdJournal, error) {
	if flags != sdJournalFlag {
		return nil, ErrUnsupported
	}

	var r sdReader

	if isJournalFileName(path) {
		var err error
		r, err = OpenFileWithOptions(path, opts)
		if err != nil {
			return nil, err
		}
	} else {
		var err error
		r, err = OpenDirectoryWithOptions(path, opts)
		if err != nil {
			return nil, err
		}
	}

	return newSdJournal(r), nil
}

func SdJournalOpenFile(path string, flags int) (*sdJournal, error) {
	return SdJournalOpenFileWithOptions(path, flags, DefaultReaderOptions())
}

func SdJournalOpenFileWithOptions(path string, flags int, opts ReaderOptions) (*sdJournal, error) {
	if flags != sdJournalFlag {
		return nil, ErrUnsupported
	}
	r, err := OpenFileWithOptions(path, opts)
	if err != nil {
		return nil, err
	}
	return newSdJournal(r), nil
}

func SdJournalOpenDirectory(path string, flags int) (*sdJournal, error) {
	return SdJournalOpenDirectoryWithOptions(path, flags, DefaultReaderOptions())
}

func SdJournalOpenDirectoryWithOptions(path string, flags int, opts ReaderOptions) (*sdJournal, error) {
	if flags != sdJournalFlag {
		return nil, ErrUnsupported
	}
	r, err := OpenDirectoryWithOptions(path, opts)
	if err != nil {
		return nil, err
	}
	return newSdJournal(r), nil
}

func SdJournalOpenFiles(paths []string, flags int) (*sdJournal, error) {
	return SdJournalOpenFilesWithOptions(paths, flags, DefaultReaderOptions())
}

func SdJournalOpenFilesWithOptions(paths []string, flags int, opts ReaderOptions) (*sdJournal, error) {
	if flags != sdJournalFlag {
		return nil, ErrUnsupported
	}
	if len(paths) == 1 {
		return SdJournalOpenFileWithOptions(paths[0], flags, opts)
	}
	r, err := OpenFilesWithOptions(paths, opts)
	if err != nil {
		return nil, err
	}
	return newSdJournal(r), nil
}

func newSdJournal(r sdReader) *sdJournal {
	return &sdJournal{reader: r}
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
	j.resetIterators()
	j.reader.AddMatch(match)
	return nil
}

func SdJournalAddDisjunction(j *sdJournal) error {
	j.resetIterators()
	j.reader.AddDisjunction()
	return nil
}

func SdJournalAddConjunction(j *sdJournal) error {
	j.resetIterators()
	j.reader.AddConjunction()
	return nil
}

func SdJournalFlushMatches(j *sdJournal) error {
	j.resetIterators()
	j.reader.FlushMatches()
	return nil
}

func SdJournalNext(j *sdJournal) (int, error) {
	j.resetIterators()
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
	j.resetIterators()
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
	j.resetIterators()
	return j.reader.SeekHead()
}

func SdJournalSeekTail(j *sdJournal) error {
	j.resetIterators()
	return j.reader.SeekTail()
}

func SdJournalSeekRealtimeUsec(j *sdJournal, usec uint64) error {
	j.resetIterators()
	return j.reader.SeekRealtimeUsec(usec)
}

func SdJournalGetRealtimeUsec(j *sdJournal) (uint64, error) {
	return j.reader.GetRealtimeUsec()
}

func SdJournalGetSeqnum(j *sdJournal) (uint64, UUID, error) {
	entry, err := j.reader.GetEntry()
	if err != nil {
		return 0, UUID{}, err
	}
	seqnumID, _, _, _, err := ParseCursor(entry.Cursor)
	if err != nil {
		return 0, UUID{}, ErrInvalidCursor
	}
	id, err := ParseUUID(seqnumID)
	if err != nil {
		return 0, UUID{}, ErrInvalidCursor
	}
	return entry.Seqnum, id, nil
}

func SdJournalGetMonotonicUsec(j *sdJournal) (uint64, UUID, error) {
	entry, err := j.reader.GetEntry()
	if err != nil {
		return 0, UUID{}, err
	}
	return entry.Monotonic, entry.BootID, nil
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

func SdJournalClose(j *sdJournal) error {
	return j.Close()
}

func (j *sdJournal) resetIterators() {
	j.dataItems = nil
	j.dataIndex = 0
	j.readerData = false
	j.fieldItems = nil
	j.fieldIndex = 0
	j.uniqueItems = nil
	j.uniqueIndex = 0
	j.reader.ClearEntryDataState()
}

func (j *sdJournal) AddMatch(data []byte) {
	j.resetIterators()
	j.reader.AddMatch(data)
}

func (j *sdJournal) AddDisjunction() {
	j.resetIterators()
	j.reader.AddDisjunction()
}

func (j *sdJournal) AddConjunction() {
	j.resetIterators()
	j.reader.AddConjunction()
}

func (j *sdJournal) FlushMatches() {
	j.resetIterators()
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

func (j *sdJournal) SeekRealtimeUsec(usec uint64) error {
	return SdJournalSeekRealtimeUsec(j, usec)
}

func (j *sdJournal) SeekCursor(cursor string) error {
	return SdJournalSeekCursor(j, cursor)
}

func (j *sdJournal) TestCursor(cursor string) (bool, error) {
	return SdJournalTestCursor(j, cursor)
}

func (j *sdJournal) GetRealtimeUsec() (uint64, error) {
	return SdJournalGetRealtimeUsec(j)
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

func (j *sdJournal) RestartData() error {
	return SdJournalRestartData(j)
}

func (j *sdJournal) EnumerateAvailableData() ([]byte, bool, error) {
	return SdJournalEnumerateAvailableData(j)
}

func (j *sdJournal) ListBoots() ([]BootInfo, error) {
	return SdJournalListBoots(j)
}

func (j *sdJournal) EnumerateFields() ([]string, error) {
	return SdJournalEnumerateFields(j)
}

func (j *sdJournal) VisitUnique(field string, visit func([]byte) error) error {
	return j.reader.VisitUnique(field, visit)
}

func SdJournalQueryUnique(j *sdJournal, field string) ([]UniqueValue, error) {
	values, err := j.reader.QueryUnique(field)
	if err != nil {
		return nil, err
	}
	result := make([]UniqueValue, len(values))
	for i, v := range values {
		result[i] = UniqueValue{Field: field, Value: append([]byte(nil), v...)}
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
	sort.Strings(result)
	return result, nil
}

func SdJournalGetEntry(j *sdJournal) (*Entry, error) {
	return j.reader.GetEntry()
}

// SdJournalGetData returns an owned FIELD=value payload copy for field.
func SdJournalGetData(j *sdJournal, field string) ([]byte, error) {
	payload, ok, err := j.reader.GetEntryPayload([]byte(field))
	if err != nil {
		return nil, err
	}
	if !ok {
		return nil, ErrNoEntry
	}
	return payload, nil
}

func SdJournalRestartData(j *sdJournal) error {
	if err := j.reader.EntryDataRestart(); err != nil {
		return err
	}
	j.dataItems = nil
	j.dataIndex = 0
	j.readerData = true
	return nil
}

// SdJournalEnumerateAvailableData returns the next FIELD=value payload for the
// current entry. Returned slices stay valid for the current row after
// end-of-row enumeration and until the journal advances, seeks,
// clears/restarts DATA enumeration, refreshes/remaps the file, or closes. Copy
// the slice when longer ownership is required.
func SdJournalEnumerateAvailableData(j *sdJournal) ([]byte, bool, error) {
	if j.readerData {
		return j.reader.EnumerateEntryPayload()
	}
	if j.dataIndex >= len(j.dataItems) {
		return nil, false, nil
	}
	item := append([]byte(nil), j.dataItems[j.dataIndex]...)
	j.dataIndex++
	return item, true, nil
}

func SdJournalRestartFields(j *sdJournal) error {
	fields, err := SdJournalEnumerateFields(j)
	if err != nil {
		return err
	}
	j.fieldItems = fields
	j.fieldIndex = 0
	return nil
}

func SdJournalEnumerateField(j *sdJournal) (string, bool, error) {
	if j.fieldIndex >= len(j.fieldItems) {
		return "", false, nil
	}
	item := j.fieldItems[j.fieldIndex]
	j.fieldIndex++
	return item, true, nil
}

func SdJournalGetEntryWithRealtime(j *sdJournal, realtime uint64) (*Entry, error) {
	originalCursor, originalErr := j.reader.GetCursor()
	if err := j.reader.SeekRealtimeUsec(realtime); err != nil {
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
		if entry.Realtime > realtime {
			return nil, ErrNoEntry
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
	j.resetIterators()
	want, err := parseCursorLocation(cursor, true)
	if err != nil {
		return ErrInvalidCursor
	}

	if want.realtimeSet {
		if err := j.reader.SeekRealtimeUsec(want.realtime); err != nil {
			return err
		}
	} else if err := j.reader.SeekHead(); err != nil {
		return err
	}

	for {
		ok, err := j.reader.Step()
		if err != nil {
			if errors.Is(err, errEndOfEntries) {
				return nil
			}
			return err
		}
		if !ok {
			return nil
		}

		entry, err := j.reader.GetEntry()
		if err != nil {
			return err
		}

		got, err := parseCursorLocation(entry.Cursor, false)
		if err != nil {
			return err
		}
		if cursorLocationAtOrAfter(got, want) {
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

func SdJournalQueryUniqueState(j *sdJournal, field string) error {
	j.uniqueItems = j.uniqueItems[:0]
	err := j.reader.VisitUnique(field, func(value []byte) error {
		j.uniqueItems = append(j.uniqueItems, payloadFromFieldValue(field, value))
		return nil
	})
	if err != nil {
		return err
	}
	j.uniqueIndex = 0
	return nil
}

func SdJournalRestartUnique(j *sdJournal) error {
	j.uniqueIndex = 0
	return nil
}

func SdJournalEnumerateAvailableUnique(j *sdJournal) ([]byte, bool, error) {
	if j.uniqueIndex >= len(j.uniqueItems) {
		return nil, false, nil
	}
	item := append([]byte(nil), j.uniqueItems[j.uniqueIndex]...)
	j.uniqueIndex++
	return item, true, nil
}

func payloadFromFieldValue(field string, value []byte) []byte {
	payload := make([]byte, 0, len(field)+1+len(value))
	payload = append(payload, field...)
	payload = append(payload, '=')
	payload = append(payload, value...)
	return payload
}
