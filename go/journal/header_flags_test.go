package journal

import "testing"

// Values from systemd journal-def.h; consumers rely on the exported names.
func TestHeaderFlagValuesMatchSystemd(t *testing.T) {
	cases := []struct {
		name string
		got  uint32
		want uint32
	}{
		{"HEADER_COMPATIBLE_SEALED", HeaderCompatibleSealed, 1 << 0},
		{"HEADER_COMPATIBLE_TAIL_ENTRY_BOOT_ID", HeaderCompatibleTailEntryBootID, 1 << 1},
		{"HEADER_COMPATIBLE_SEALED_CONTINUOUS", HeaderCompatibleSealedContinuous, 1 << 2},
		{"HEADER_INCOMPATIBLE_COMPRESSED_XZ", HeaderIncompatibleCompressedXZ, 1 << 0},
		{"HEADER_INCOMPATIBLE_COMPRESSED_LZ4", HeaderIncompatibleCompressedLZ4, 1 << 1},
		{"HEADER_INCOMPATIBLE_KEYED_HASH", HeaderIncompatibleKeyedHash, 1 << 2},
		{"HEADER_INCOMPATIBLE_COMPRESSED_ZSTD", HeaderIncompatibleCompressedZSTD, 1 << 3},
		{"HEADER_INCOMPATIBLE_COMPACT", HeaderIncompatibleCompact, 1 << 4},
	}
	for _, tc := range cases {
		if tc.got != tc.want {
			t.Errorf("%s = %#x, want %#x", tc.name, tc.got, tc.want)
		}
	}
}
