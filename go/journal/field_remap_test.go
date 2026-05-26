package journal

import "testing"

func TestEncodeRemappedFieldNameVectors(t *testing.T) {
	tests := map[string]string{
		"hello":                                 "NDE_HELLO",
		"foo.bar":                               "NDAE_FOO_BAR",
		"fooBar":                                "NDA3J_FOOBAR",
		"log.body.HostName":                     "ND83AAO_LB_HOSTNAME",
		"OAuth2Token":                           "NDZ9SNSO_OAUTH2TOKEN",
		"HTTPSConnection":                       "NDNSSO_HTTPSCONNECTION",
		"hello-world":                           "NDCE_HELLO_WORLD",
		"resource.attributes.host.name":         "ND3AE_RA_HOST_NAME",
		"_CUSTOM_FIELD":                         "NDVQT__CUSTOM_FIELD",
		"field name":                            "ND_BFAAD773361A781112FB325B433D54F7",
		string([]byte{0xff, 0xfe}) + " invalid": "ND_33493B98B07A586AA08BE7C2E7D90C3A",
	}
	for input, want := range tests {
		if got := encodeRemappedFieldName([]byte(input)); got != want {
			t.Fatalf("encodeRemappedFieldName(%q) = %q, want %q", input, got, want)
		}
	}
}
