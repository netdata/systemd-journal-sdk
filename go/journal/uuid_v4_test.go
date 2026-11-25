package journal

import "testing"

// systemd marks random IDs as RFC 4122 version 4, variant DCE
// (src/libsystemd/sd-id128/id128-util.c id128_make_v4_uuid).
func TestNewUUIDSetsVersion4AndVariant(t *testing.T) {
	for i := 0; i < 64; i++ {
		id, err := NewUUID()
		if err != nil {
			t.Fatal(err)
		}
		if id[6]>>4 != 4 {
			t.Fatalf("version nibble = %x in %x", id[6]>>4, id)
		}
		if id[8]>>6 != 2 {
			t.Fatalf("variant bits = %b in %x", id[8]>>6, id)
		}
	}
}
