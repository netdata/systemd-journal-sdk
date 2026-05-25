package journal

import (
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestFSPRGVectors(t *testing.T) {
	// fss_test.go lives in go/journal/; repo root is two levels up.
	repoRoot, err := filepath.Abs(filepath.Join("..", ".."))
	if err != nil {
		t.Fatalf("repo root: %v", err)
	}
	data, err := os.ReadFile(filepath.Join(repoRoot, "tests", "fss", "fixtures", "fsprg-vectors-v01.json"))
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}
	var fixture struct {
		FSPRGParams struct {
			Secpar uint `json:"secpar"`
		} `json:"fsprg_params"`
		Vectors []struct {
			SeedDesc   string `json:"seed_desc"`
			SeedHex    string `json:"seed_hex"`
			MskHex     string `json:"msk_hex"`
			MpkHex     string `json:"mpk_hex"`
			State0Hex  string `json:"state0_hex"`
			Epochs []struct {
				Epoch        uint64 `json:"epoch"`
				StateHex     string `json:"state_hex"`
				SeekStateHex string `json:"seek_state_hex"`
				Keys []struct {
					Idx    uint32 `json:"idx"`
					KeyLen uint32 `json:"keylen"`
					KeyHex string `json:"key_hex"`
				} `json:"keys"`
			} `json:"epochs"`
		} `json:"vectors"`
	}
	if err := json.Unmarshal(data, &fixture); err != nil {
		t.Fatalf("parse vectors: %v", err)
	}

	for _, vec := range fixture.Vectors {
		seed, err := hex.DecodeString(vec.SeedHex)
		if err != nil {
			t.Fatalf("decode seed %s: %v", vec.SeedDesc, err)
		}
		expectedMsk, err := hex.DecodeString(vec.MskHex)
		if err != nil {
			t.Fatalf("decode msk %s: %v", vec.SeedDesc, err)
		}
		expectedMpk, err := hex.DecodeString(vec.MpkHex)
		if err != nil {
			t.Fatalf("decode mpk %s: %v", vec.SeedDesc, err)
		}
		expectedState0, err := hex.DecodeString(vec.State0Hex)
		if err != nil {
			t.Fatalf("decode state0 %s: %v", vec.SeedDesc, err)
		}

		msk, mpk, err := fsprgGenMK(seed, fixture.FSPRGParams.Secpar)
		if err != nil {
			t.Fatalf("GenMK %s: %v", vec.SeedDesc, err)
		}
		if !bytesEqual(msk, expectedMsk) {
			t.Fatalf("msk mismatch for %s", vec.SeedDesc)
		}
		if !bytesEqual(mpk, expectedMpk) {
			t.Fatalf("mpk mismatch for %s", vec.SeedDesc)
		}

		state0 := fsprgGenState0(mpk, seed)
		if !bytesEqual(state0, expectedState0) {
			t.Fatalf("state0 mismatch for %s", vec.SeedDesc)
		}
		if fsprgGetEpoch(state0) != 0 {
			t.Fatalf("epoch0 mismatch for %s", vec.SeedDesc)
		}

		for _, ep := range vec.Epochs {
			evolved := make([]byte, len(state0))
			copy(evolved, state0)
			for e := uint64(0); e < ep.Epoch; e++ {
				evolved = fsprgEvolve(evolved)
			}
			expectedState, err := hex.DecodeString(ep.StateHex)
			if err != nil {
				t.Fatalf("decode state %s epoch %d: %v", vec.SeedDesc, ep.Epoch, err)
			}
			if !bytesEqual(evolved, expectedState) {
				t.Fatalf("evolve mismatch for %s epoch %d", vec.SeedDesc, ep.Epoch)
			}

			seeked := fsprgSeek(state0, ep.Epoch, msk, seed)
			expectedSeek, err := hex.DecodeString(ep.SeekStateHex)
			if err != nil {
				t.Fatalf("decode seek_state %s epoch %d: %v", vec.SeedDesc, ep.Epoch, err)
			}
			if !bytesEqual(seeked, expectedSeek) {
				t.Fatalf("seek mismatch for %s epoch %d", vec.SeedDesc, ep.Epoch)
			}

			for _, k := range ep.Keys {
				key := fsprgGetKey(evolved, k.KeyLen, k.Idx)
				expectedKey, err := hex.DecodeString(k.KeyHex)
				if err != nil {
					t.Fatalf("decode key %s epoch %d idx %d: %v", vec.SeedDesc, ep.Epoch, k.Idx, err)
				}
				if !bytesEqual(key, expectedKey) {
					t.Fatalf("key mismatch for %s epoch %d idx %d", vec.SeedDesc, ep.Epoch, k.Idx)
				}
			}
		}
	}
}

func bytesEqual(a, b []byte) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
