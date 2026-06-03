package journal

import (
	"bytes"
	"encoding/binary"
	"errors"
	"io"

	"github.com/klauspost/compress/zstd"
	"github.com/pierrec/lz4/v4"
	"github.com/ulikunitz/xz"
)

func (w *Writer) compressedDataPayload(payload []byte) ([]byte, uint8) {
	if len(payload) < w.compressThreshold {
		return payload, 0
	}
	if compressed, flag, ok := w.tryCompressDataPayload(payload); ok {
		return compressed, flag
	}
	return payload, 0
}

func (w *Writer) tryCompressDataPayload(payload []byte) ([]byte, uint8, bool) {
	switch w.compression {
	case CompressionZSTD:
		return tryZstdDataPayload(payload)
	case CompressionXZ:
		return tryXZDataPayload(payload)
	case CompressionLZ4:
		return tryLZ4DataPayload(payload)
	default:
		return nil, 0, false
	}
}

func tryZstdDataPayload(payload []byte) ([]byte, uint8, bool) {
	compressed, err := zstdCompress(payload)
	if err == nil && len(compressed) < len(payload) {
		return compressed, objectCompressedZSTD, true
	}
	return nil, 0, false
}

func tryXZDataPayload(payload []byte) ([]byte, uint8, bool) {
	if len(payload) < 80 {
		return nil, 0, false
	}
	compressed, err := xzCompress(payload)
	if err == nil && len(compressed) < len(payload) {
		return compressed, objectCompressedXZ, true
	}
	return nil, 0, false
}

func tryLZ4DataPayload(payload []byte) ([]byte, uint8, bool) {
	if len(payload) < 9 {
		return nil, 0, false
	}
	compressed := lz4Compress(payload)
	if len(compressed) < len(payload) {
		return compressed, objectCompressedLZ4, true
	}
	return nil, 0, false
}

func dedupeEntryItems(items []entryItem) []entryItem {
	if len(items) < 2 {
		return items
	}
	out := items[:1]
	for _, item := range items[1:] {
		if item.offset != out[len(out)-1].offset {
			out = append(out, item)
		}
	}
	return out
}

func zstdCompress(payload []byte) ([]byte, error) {
	var buf bytes.Buffer
	enc, err := zstd.NewWriter(&buf, zstd.WithEncoderLevel(zstd.SpeedFastest))
	if err != nil {
		return nil, err
	}
	if _, err := enc.Write(payload); err != nil {
		return nil, err
	}
	if err := enc.Close(); err != nil {
		return nil, err
	}
	return zstdFrameWithContentSize(buf.Bytes(), len(payload)), nil
}

func zstdFrameWithContentSize(frame []byte, contentSize int) []byte {
	const (
		zstdMagic           = "\x28\xb5\x2f\xfd"
		singleSegmentFlag   = byte(1 << 5)
		contentChecksumFlag = byte(1 << 2)
	)
	if len(frame) < 6 || string(frame[:4]) != zstdMagic {
		return frame
	}
	descriptor := frame[4]
	dictionaryIDFlag := descriptor & 0x03
	frameContentSizeFlag := descriptor >> 6
	if dictionaryIDFlag != 0 || frameContentSizeFlag != 0 || descriptor&singleSegmentFlag != 0 {
		return frame
	}

	var sizeFlag byte
	var sizeBytes []byte
	switch {
	case contentSize <= 255:
		sizeFlag = 0
		sizeBytes = []byte{byte(contentSize)}
	case contentSize <= 65791:
		sizeFlag = 1
		encoded := uint16(contentSize - 256)
		sizeBytes = []byte{byte(encoded), byte(encoded >> 8)}
	case uint64(contentSize) <= uint64(^uint32(0)):
		sizeFlag = 2
		encoded := uint32(contentSize)
		sizeBytes = []byte{byte(encoded), byte(encoded >> 8), byte(encoded >> 16), byte(encoded >> 24)}
	default:
		sizeFlag = 3
		encoded := uint64(contentSize)
		sizeBytes = []byte{
			byte(encoded),
			byte(encoded >> 8),
			byte(encoded >> 16),
			byte(encoded >> 24),
			byte(encoded >> 32),
			byte(encoded >> 40),
			byte(encoded >> 48),
			byte(encoded >> 56),
		}
	}

	patched := make([]byte, 0, len(frame)+len(sizeBytes)-1)
	patched = append(patched, frame[:4]...)
	patched = append(patched, sizeFlag<<6|singleSegmentFlag|descriptor&contentChecksumFlag)
	patched = append(patched, sizeBytes...)
	patched = append(patched, frame[6:]...)
	return patched
}

func zstdDecompress(payload []byte) ([]byte, error) {
	decoder, err := zstd.NewReader(nil, zstd.WithDecoderMaxMemory(uint64(maxUncompressedDataObjectSize)))
	if err != nil {
		return nil, err
	}
	defer decoder.Close()
	return decoder.DecodeAll(payload, nil)
}

func xzCompress(payload []byte) ([]byte, error) {
	cfg := xz.WriterConfig{NoCheckSum: true}
	var buf bytes.Buffer
	w, err := cfg.NewWriter(&buf)
	if err != nil {
		return nil, err
	}
	if _, err := w.Write(payload); err != nil {
		return nil, err
	}
	if err := w.Close(); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

func xzDecompress(payload []byte) ([]byte, error) {
	r, err := xz.NewReader(bytes.NewReader(payload))
	if err != nil {
		return nil, err
	}
	return readAllLimited(r, maxUncompressedDataObjectSize)
}

func lz4Compress(payload []byte) []byte {
	maxCompressedSize := lz4.CompressBlockBound(len(payload))
	compressed := make([]byte, maxCompressedSize)
	n, err := lz4.CompressBlock(payload, compressed, nil)
	if err != nil || n == 0 {
		return payload
	}
	compressed = compressed[:n]
	out := make([]byte, 8+len(compressed))
	binary.LittleEndian.PutUint64(out[:8], uint64(len(payload)))
	copy(out[8:], compressed)
	return out
}

func lz4Decompress(payload []byte) ([]byte, error) {
	if len(payload) < 8 {
		return nil, errors.New("lz4 compressed payload too short")
	}
	uncompressedSize := binary.LittleEndian.Uint64(payload[:8])
	if uncompressedSize > maxUncompressedDataObjectSize {
		return nil, errors.New("lz4 decompressed payload too large")
	}
	compressedData := payload[8:]
	decoded := make([]byte, uncompressedSize)
	n, err := lz4.UncompressBlock(compressedData, decoded)
	if err != nil {
		return nil, err
	}
	if uint64(n) != uncompressedSize {
		return nil, errors.New("lz4 decompressed size mismatch")
	}
	return decoded, nil
}

func readAllLimited(r io.Reader, maxBytes int) ([]byte, error) {
	limited := io.LimitReader(r, int64(maxBytes)+1)
	decoded, err := io.ReadAll(limited)
	if err != nil {
		return nil, err
	}
	if len(decoded) > maxBytes {
		return nil, errors.New("decompressed payload too large")
	}
	return decoded, nil
}
