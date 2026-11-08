package journal

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"testing"

	"github.com/ulikunitz/xz"
)

// xzDeclaredDictCap returns the LZMA2 dictionary size declared in the first
// block header of an XZ stream (xz-file-format 1.2.0, sections 3.1 and 5.3.1).
func xzDeclaredDictCap(t testing.TB, stream []byte) int {
	t.Helper()
	const streamHeaderSize = 12
	if len(stream) < streamHeaderSize+2 {
		t.Fatalf("xz stream too short: %d bytes", len(stream))
	}
	header := stream[streamHeaderSize:]
	headerSize := (int(header[0]) + 1) * 4
	flags := header[1]
	pos := 2
	readVarint := func() uint64 {
		v, n := binary.Uvarint(header[pos:headerSize])
		if n <= 0 {
			t.Fatalf("bad varint in xz block header")
		}
		pos += n
		return v
	}
	if flags&0x40 != 0 {
		readVarint() // compressed size
	}
	if flags&0x80 != 0 {
		readVarint() // uncompressed size
	}
	if filters := int(flags&0x03) + 1; filters != 1 {
		t.Fatalf("xz block declares %d filters, want 1", filters)
	}
	if id := readVarint(); id != 0x21 {
		t.Fatalf("xz filter id = %#x, want LZMA2 (0x21)", id)
	}
	if size := readVarint(); size != 1 {
		t.Fatalf("LZMA2 property size = %d, want 1", size)
	}
	prop := int(header[pos])
	if prop > 40 {
		t.Fatalf("LZMA2 dictionary property = %d, want <= 40", prop)
	}
	return (2 | (prop & 1)) << (prop/2 + 11)
}

// xzTestPayload is deterministic log-like text with no repeats beyond one line,
// so it compresses the same under any dictionary of at least a few KiB.
func xzTestPayload(size int) []byte {
	var buf bytes.Buffer
	state := uint64(0x9e3779b97f4a7c15)
	for i := 0; buf.Len() < size; i++ {
		state ^= state << 13
		state ^= state >> 7
		state ^= state << 17
		fmt.Fprintf(&buf, "flow=%d src=10.%d.%d.%d dst=port%d bytes=%d\n",
			i, state&0xff, state>>8&0xff, state>>16&0xff, state>>24&0xffff, state>>40)
	}
	return buf.Bytes()[:size]
}

func TestXZCompressDeclaresWriterDictionary(t *testing.T) {
	for _, size := range []int{600, xzDictCap + 64<<10, 4 << 20} {
		payload := xzTestPayload(size)
		stream, err := xzCompress(payload)
		if err != nil {
			t.Fatalf("xzCompress(%d) error = %v", size, err)
		}
		if got := xzDeclaredDictCap(t, stream); got != xzDictCap {
			t.Fatalf("declared dictionary for %d-byte payload = %d, want %d", size, got, xzDictCap)
		}
		decoded, err := xzDecompress(stream)
		if err != nil {
			t.Fatalf("xzDecompress(%d) error = %v", size, err)
		}
		if !bytes.Equal(decoded, payload) {
			t.Fatalf("round trip of %d-byte payload differs", size)
		}
	}
}

func xzStreamWithDictionary(tb testing.TB, payload []byte, dict int) []byte {
	tb.Helper()
	cfg := xz.WriterConfig{NoCheckSum: true, DictCap: dict}
	var buf bytes.Buffer
	w, err := cfg.NewWriter(&buf)
	if err != nil {
		tb.Fatal(err)
	}
	if _, err := w.Write(payload); err != nil {
		tb.Fatal(err)
	}
	if err := w.Close(); err != nil {
		tb.Fatal(err)
	}
	return buf.Bytes()
}

// Files written before the 256 KiB dictionary declare larger ones.
func TestXZDecompressRoundTripsLegacyDictionaries(t *testing.T) {
	for _, dict := range []int{1 << 20, 8 << 20} {
		for _, size := range []int{600, 64 << 10} {
			payload := xzTestPayload(size)
			decoded, err := xzDecompress(xzStreamWithDictionary(t, payload, dict))
			if err != nil {
				t.Fatalf("dict=%d size=%d: %v", dict, size, err)
			}
			if !bytes.Equal(decoded, payload) {
				t.Fatalf("dict=%d size=%d: round trip differs", dict, size)
			}
		}
	}
}

// BenchmarkXZDecompressByDeclaredDictionary measures per-DATA decode cost,
// which follows the dictionary the writer declared, not the payload size.
func BenchmarkXZDecompressByDeclaredDictionary(b *testing.B) {
	for _, size := range []int{600, 64 << 10} {
		payload := xzTestPayload(size)
		for _, dict := range []int{xzDictCap, 1 << 20, 8 << 20} {
			stream := xzStreamWithDictionary(b, payload, dict)
			b.Run(fmt.Sprintf("payload=%d/dict=%dKiB", size, dict>>10), func(b *testing.B) {
				b.ReportAllocs()
				for i := 0; i < b.N; i++ {
					if _, err := xzDecompress(stream); err != nil {
						b.Fatal(err)
					}
				}
			})
		}
	}
}
