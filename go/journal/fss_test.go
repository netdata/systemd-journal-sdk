package journal

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

type fsprgFixture struct {
	FSPRGParams struct {
		Secpar uint `json:"secpar"`
	} `json:"fsprg_params"`
	Vectors []fsprgVector `json:"vectors"`
}

type fsprgVector struct {
	SeedDesc  string       `json:"seed_desc"`
	SeedHex   string       `json:"seed_hex"`
	MskHex    string       `json:"msk_hex"`
	MpkHex    string       `json:"mpk_hex"`
	State0Hex string       `json:"state0_hex"`
	Epochs    []fsprgEpoch `json:"epochs"`
}

type fsprgEpoch struct {
	Epoch        uint64     `json:"epoch"`
	StateHex     string     `json:"state_hex"`
	SeekStateHex string     `json:"seek_state_hex"`
	Keys         []fsprgKey `json:"keys"`
}

type fsprgKey struct {
	Idx    uint32 `json:"idx"`
	KeyLen uint32 `json:"keylen"`
	KeyHex string `json:"key_hex"`
}

func loadFSPRGFixture(t *testing.T) fsprgFixture {
	t.Helper()
	repoRoot, err := filepath.Abs(filepath.Join("..", ".."))
	if err != nil {
		t.Fatalf("repo root: %v", err)
	}
	data, err := os.ReadFile(filepath.Join(repoRoot, "tests", "fss", "fixtures", "fsprg-vectors-v01.json"))
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}
	var fixture fsprgFixture
	if err := json.Unmarshal(data, &fixture); err != nil {
		t.Fatalf("parse vectors: %v", err)
	}
	return fixture
}

func TestFSPRGVectors(t *testing.T) {
	fixture := loadFSPRGFixture(t)
	for _, vec := range fixture.Vectors {
		assertFSPRGVector(t, fixture.FSPRGParams.Secpar, vec)
	}
}

func assertFSPRGVector(t *testing.T, secpar uint, vec fsprgVector) {
	t.Helper()
	seed := decodeHex(t, "seed "+vec.SeedDesc, vec.SeedHex)
	msk, mpk, err := fsprgGenMK(seed, secpar)
	if err != nil {
		t.Fatalf("GenMK %s: %v", vec.SeedDesc, err)
	}
	requireBytesEqual(t, "msk "+vec.SeedDesc, msk, decodeHex(t, "msk "+vec.SeedDesc, vec.MskHex))
	requireBytesEqual(t, "mpk "+vec.SeedDesc, mpk, decodeHex(t, "mpk "+vec.SeedDesc, vec.MpkHex))

	state0 := fsprgGenState0(mpk, seed)
	requireBytesEqual(t, "state0 "+vec.SeedDesc, state0, decodeHex(t, "state0 "+vec.SeedDesc, vec.State0Hex))
	if fsprgGetEpoch(state0) != 0 {
		t.Fatalf("epoch0 mismatch for %s", vec.SeedDesc)
	}
	for _, ep := range vec.Epochs {
		assertFSPRGEpoch(t, vec.SeedDesc, seed, msk, state0, ep)
	}
}

func assertFSPRGEpoch(t *testing.T, seedDesc string, seed []byte, msk []byte, state0 []byte, ep fsprgEpoch) {
	t.Helper()
	evolved := evolveToEpoch(state0, ep.Epoch)
	requireBytesEqual(t, fmt.Sprintf("evolve %s epoch %d", seedDesc, ep.Epoch), evolved, decodeHex(t, "state", ep.StateHex))

	seeked := fsprgSeek(state0, ep.Epoch, msk, seed)
	requireBytesEqual(t, fmt.Sprintf("seek %s epoch %d", seedDesc, ep.Epoch), seeked, decodeHex(t, "seek_state", ep.SeekStateHex))

	for _, k := range ep.Keys {
		key := fsprgGetKey(evolved, k.KeyLen, k.Idx)
		label := fmt.Sprintf("key %s epoch %d idx %d", seedDesc, ep.Epoch, k.Idx)
		requireBytesEqual(t, label, key, decodeHex(t, label, k.KeyHex))
	}
}

func evolveToEpoch(state0 []byte, epoch uint64) []byte {
	evolved := make([]byte, len(state0))
	copy(evolved, state0)
	for range epoch {
		evolved = fsprgEvolve(evolved)
	}
	return evolved
}

func decodeHex(t *testing.T, label string, value string) []byte {
	t.Helper()
	decoded, err := hex.DecodeString(value)
	if err != nil {
		t.Fatalf("decode %s: %v", label, err)
	}
	return decoded
}

func requireBytesEqual(t *testing.T, label string, got []byte, want []byte) {
	t.Helper()
	if !bytesEqual(got, want) {
		t.Fatalf("%s mismatch", label)
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
