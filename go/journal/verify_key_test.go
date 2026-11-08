package journal

import "testing"

const verifyKeySeed = "0123456789abcdef01234567"

// Accepted and rejected shapes were checked against the systemd v262 CLI.
func TestParseVerificationKeyFollowsSystemdGrammar(t *testing.T) {
	const maxUint64 = ^uint64(0)
	for _, tc := range []struct {
		name         string
		key          string
		wantStart    uint64
		wantInterval uint64
		wantErr      bool
	}{
		{name: "plain", key: verifyKeySeed + "/1-f4240", wantStart: 1, wantInterval: 0xf4240},
		{name: "dashed seed", key: "-012345-6789ab--cdef01234567/1-2", wantStart: 1, wantInterval: 2},
		{name: "trailing content", key: verifyKeySeed + "/1-2x", wantStart: 1, wantInterval: 2},
		{name: "spaces", key: verifyKeySeed + "/ 1- 2", wantStart: 1, wantInterval: 2},
		{name: "signs", key: verifyKeySeed + "/+1-+2", wantStart: 1, wantInterval: 2},
		{name: "prefix", key: verifyKeySeed + "/0x1-0X2", wantStart: 1, wantInterval: 2},
		{name: "negative wraps", key: verifyKeySeed + "/1--1", wantStart: 1, wantInterval: maxUint64},
		{name: "saturates", key: verifyKeySeed + "/1-1ffffffffffffffff", wantStart: 1, wantInterval: maxUint64},
		{name: "empty", key: "", wantErr: true},
		{name: "signed seed", key: "+a" + verifyKeySeed[2:] + "/1-2", wantErr: true},
		{name: "long seed", key: verifyKeySeed + "ab/1-2", wantErr: true},
		{name: "no slash", key: verifyKeySeed, wantErr: true},
		{name: "no interval", key: verifyKeySeed + "/1", wantErr: true},
		{name: "space before dash", key: verifyKeySeed + "/1 -2", wantErr: true},
		{name: "zero start", key: verifyKeySeed + "/0-2", wantErr: true},
		{name: "zero interval", key: verifyKeySeed + "/1-0", wantErr: true},
		{name: "prefix only", key: verifyKeySeed + "/0x-2", wantErr: true},
		{name: "start time overflow", key: verifyKeySeed + "/ffffffffffffffff-2", wantErr: true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			got, err := ParseVerificationKey(tc.key)
			if tc.wantErr {
				if err == nil {
					t.Fatalf("expected rejection, got %+v", got)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if got.StartEpoch != tc.wantStart || got.IntervalUsec != tc.wantInterval {
				t.Fatalf("got start %#x interval %#x", got.StartEpoch, got.IntervalUsec)
			}
			want := [12]byte{0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67}
			if got.Seed != want {
				t.Fatalf("got seed %x", got.Seed)
			}
		})
	}
}
