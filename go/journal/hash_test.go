package journal

import "testing"

func TestJenkinsHash64MatchesSystemdLookup3Values(t *testing.T) {
	tests := []struct {
		name string
		data []byte
		want uint64
	}{
		{name: "empty", data: []byte(""), want: 0xdead_beef_dead_beef},
		{name: "identifier", data: []byte("SYSLOG_IDENTIFIER=netdata"), want: 0x45cc_d0e9_ed13_614a},
		{name: "unit", data: []byte("_SYSTEMD_UNIT=netdata.service"), want: 0x1013_c5df_11a9_83f0},
		{name: "priority", data: []byte("PRIORITY=6"), want: 0x80f0_9f19_808d_26a3},
		{name: "message", data: []byte("MESSAGE=Test message"), want: 0x8ed5_3fb5_2aa5_c55d},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := jenkinsHash64(tt.data); got != tt.want {
				t.Fatalf("jenkinsHash64(%q) = %#x, want %#x", tt.data, got, tt.want)
			}
		})
	}
}

func TestSipHash24MatchesReferenceVectors(t *testing.T) {
	var key UUID
	for i := range key {
		key[i] = byte(i)
	}

	tests := []struct {
		length int
		want   uint64
	}{
		{length: 0, want: 0x726fdb47dd0e0e31},
		{length: 1, want: 0x74f839c593dc67fd},
		{length: 2, want: 0x0d6c8009d9a94f5a},
		{length: 3, want: 0x85676696d7fb7e2d},
		{length: 4, want: 0xcf2794e0277187b7},
		{length: 5, want: 0x18765564cd99a68d},
		{length: 6, want: 0xcbc9466e58fee3ce},
		{length: 7, want: 0xab0200f58b01d137},
	}

	message := make([]byte, 64)
	for i := range message {
		message[i] = byte(i)
	}

	for _, tt := range tests {
		t.Run("length", func(t *testing.T) {
			if got := sipHash24(key, message[:tt.length]); got != tt.want {
				t.Fatalf("sipHash24(length=%d) = %#x, want %#x", tt.length, got, tt.want)
			}
		})
	}
}
