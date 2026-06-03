package journal

import (
	"crypto/sha256"
	"encoding/binary"
	"errors"
	"math/big"
)

const (
	fsprgRecommendedSecpar  = 1536
	fsprgRecommendedSeedlen = 12
)

func isValidSecpar(secpar uint) bool {
	return secpar%16 == 0 && secpar >= 16 && secpar <= 16384
}

// mskInBytes returns the size of the master-secret key for a given secpar.
func mskInBytes(secpar uint) int {
	if !isValidSecpar(secpar) {
		panic("invalid secpar")
	}
	return 2 + int(secpar)/8
}

// mpkInBytes returns the size of the master-public key for a given secpar.
func mpkInBytes(secpar uint) int {
	if !isValidSecpar(secpar) {
		panic("invalid secpar")
	}
	return 2 + int(secpar)/8
}

// stateInBytes returns the size of an FSPRG state for a given secpar.
func stateInBytes(secpar uint) int {
	if !isValidSecpar(secpar) {
		panic("invalid secpar")
	}
	return 2 + 2*int(secpar)/8 + 8
}

func storeSecpar(secpar uint) []byte {
	v := uint16(secpar/16 - 1)
	b := make([]byte, 2)
	binary.BigEndian.PutUint16(b, v)
	return b
}

func readSecpar(buf []byte) uint {
	v := binary.BigEndian.Uint16(buf[:2])
	return 16 * (uint(v) + 1)
}

func mpiExport(x *big.Int, buflen int) []byte {
	b := x.Bytes()
	if len(b) > buflen {
		panic("mpiExport: value too large for buffer")
	}
	if len(b) == buflen {
		return b
	}
	out := make([]byte, buflen)
	copy(out[buflen-len(b):], b)
	return out
}

func mpiImport(buf []byte) *big.Int {
	return new(big.Int).SetBytes(buf)
}

func uint64Export(x uint64) []byte {
	b := make([]byte, 8)
	binary.BigEndian.PutUint64(b, x)
	return b
}

func uint64Import(buf []byte) uint64 {
	return binary.BigEndian.Uint64(buf[:8])
}

// detRandomize deterministically generates buflen pseudorandom bytes from seed and idx.
// The implementation computes SHA256(seed || idx || ctr), matching systemd's
// deterministic randomization for these vectors.
func detRandomize(buflen int, seed []byte, idx uint32) []byte {
	out := make([]byte, 0, buflen)
	ctr := uint32(0)
	for buflen > 0 {
		h := sha256.New()
		h.Write(seed)
		h.Write([]byte{byte(idx >> 24), byte(idx >> 16), byte(idx >> 8), byte(idx)})
		h.Write([]byte{byte(ctr >> 24), byte(ctr >> 16), byte(ctr >> 8), byte(ctr)})
		chunk := h.Sum(nil)
		cpylen := buflen
		if cpylen > 32 {
			cpylen = 32
		}
		out = append(out, chunk[:cpylen]...)
		buflen -= cpylen
		ctr++
	}
	return out
}

// millerRabin performs a deterministic Miller-Rabin probable-prime test
// using the first `rounds` prime bases.  Using 64 bases provides a
// stronger deterministic check than 12 bases and reduces arbitrary-seed
// divergence risk.  This does not claim to reproduce libgcrypt's random
// witness selection; it is an intentionally stronger deterministic check.
func millerRabin(n *big.Int, rounds int) bool {
	if decided, prime := millerRabinSmallDecision(n); decided {
		return prime
	}

	d, r := millerRabinDecompose(n)

	bases := []int64{
		2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53,
		59, 61, 67, 71, 73, 79, 83, 89, 97, 101, 103, 107, 109, 113,
		127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179, 181,
		191, 193, 197, 199, 211, 223, 227, 229, 233, 239, 241, 251,
		257, 263, 269, 271, 277, 281, 283, 293, 307, 311,
	}
	for i := 0; i < rounds && i < len(bases); i++ {
		if millerRabinBasePasses(n, d, r, bases[i]) {
			continue
		}
		return false
	}
	return true
}

func millerRabinSmallDecision(n *big.Int) (bool, bool) {
	if n.Sign() < 0 {
		return true, false
	}
	if n.Cmp(big.NewInt(2)) == 0 || n.Cmp(big.NewInt(3)) == 0 {
		return true, true
	}
	if n.Bit(0) == 0 {
		return true, false
	}
	return false, false
}

func millerRabinDecompose(n *big.Int) (*big.Int, int) {
	d := new(big.Int).Sub(n, big.NewInt(1))
	r := 0
	for d.Bit(0) == 0 {
		d.Rsh(d, 1)
		r++
	}
	return d, r
}

func millerRabinBasePasses(n, d *big.Int, rounds int, base int64) bool {
	a := big.NewInt(base)
	if a.Cmp(n) >= 0 {
		return true
	}
	x := new(big.Int).Exp(a, d, n)
	nMinusOne := new(big.Int).Sub(n, big.NewInt(1))
	if x.Cmp(big.NewInt(1)) == 0 || x.Cmp(nMinusOne) == 0 {
		return true
	}
	for j := 1; j < rounds; j++ {
		x.Mul(x, x).Mod(x, n)
		if x.Cmp(nMinusOne) == 0 {
			return true
		}
	}
	return false
}

func genPrime3Mod4(bits uint, seed []byte, idx uint32) *big.Int {
	buflen := bits / 8
	buf := detRandomize(int(buflen), seed, idx)
	buf[0] |= 0xc0
	buf[len(buf)-1] |= 0x03
	p := mpiImport(buf)
	four := big.NewInt(4)
	for !millerRabin(p, 64) {
		p.Add(p, four)
	}
	return p
}

func genSquare(n *big.Int, seed []byte, idx uint32, secpar uint) *big.Int {
	buflen := secpar / 8
	buf := detRandomize(int(buflen), seed, idx)
	buf[0] &= 0x7f
	x := mpiImport(buf)
	if x.Cmp(n) >= 0 {
		panic("genSquare: x >= n")
	}
	result := new(big.Int).Mul(x, x)
	result.Mod(result, n)
	return result
}

func twopowmodphi(m uint64, p *big.Int) *big.Int {
	phi := new(big.Int).Sub(p, big.NewInt(1))
	return new(big.Int).Exp(big.NewInt(2), big.NewInt(0).SetUint64(m), phi)
}

func crtCompose(xp, xq, p, q *big.Int) *big.Int {
	a := new(big.Int).Sub(xq, xp)
	a.Mod(a, q)
	u := new(big.Int).ModInverse(p, q)
	a.Mul(a, u).Mod(a, q)
	result := new(big.Int).Mul(p, a)
	result.Add(result, xp)
	n := new(big.Int).Mul(p, q)
	result.Mod(result, n)
	return result
}

// fsprgGenMK generates a master key pair deterministically from seed.
// It returns msk (master secret key) and mpk (master public key).
func fsprgGenMK(seed []byte, secpar uint) (msk, mpk []byte, err error) {
	if !isValidSecpar(secpar) {
		return nil, nil, errors.New("invalid secpar")
	}
	p := genPrime3Mod4(secpar/2, seed, 0x01)
	q := genPrime3Mod4(secpar/2, seed, 0x02)
	n := new(big.Int).Mul(p, q)
	msk = make([]byte, 0, mskInBytes(secpar))
	msk = append(msk, storeSecpar(secpar)...)
	msk = append(msk, mpiExport(p, int(secpar)/16)...)
	msk = append(msk, mpiExport(q, int(secpar)/16)...)
	mpk = make([]byte, 0, mpkInBytes(secpar))
	mpk = append(mpk, storeSecpar(secpar)...)
	mpk = append(mpk, mpiExport(n, int(secpar)/8)...)
	return msk, mpk, nil
}

// fsprgGenState0 generates the epoch-0 state from mpk and seed.
func fsprgGenState0(mpk, seed []byte) []byte {
	secpar := readSecpar(mpk)
	n := mpiImport(mpk[2 : 2+secpar/8])
	x := genSquare(n, seed, 0x03, secpar)
	state := make([]byte, stateInBytes(secpar))
	copy(state, mpk)
	copy(state[2+secpar/8:], mpiExport(x, int(secpar)/8))
	// epoch zero is already zero-initialized
	return state
}

// fsprgGetEpoch returns the epoch encoded in state.
func fsprgGetEpoch(state []byte) uint64 {
	secpar := readSecpar(state)
	return uint64Import(state[2+2*secpar/8 : 2+2*secpar/8+8])
}

// fsprgEvolve advances state forward by one epoch.
// The input slice is not modified; a new state is returned.
func fsprgEvolve(state []byte) []byte {
	secpar := readSecpar(state)
	n := mpiImport(state[2 : 2+secpar/8])
	x := mpiImport(state[2+secpar/8 : 2+2*secpar/8])
	epoch := uint64Import(state[2+2*secpar/8 : 2+2*secpar/8+8])
	x.Mul(x, x).Mod(x, n)
	epoch++
	newState := make([]byte, len(state))
	copy(newState, state)
	copy(newState[2+secpar/8:], mpiExport(x, int(secpar)/8))
	copy(newState[2+2*secpar/8:], uint64Export(epoch))
	return newState
}

// fsprgSeek seeks to an arbitrary epoch using msk and seed.
// The supplied state must be an epoch-0 state.
func fsprgSeek(state []byte, epoch uint64, msk, seed []byte) []byte {
	secpar := readSecpar(msk)
	p := mpiImport(msk[2 : 2+secpar/16])
	q := mpiImport(msk[2+secpar/16 : 2+2*secpar/16])
	n := new(big.Int).Mul(p, q)
	x := genSquare(n, seed, 0x03, secpar)
	xp := new(big.Int).Mod(x, p)
	xq := new(big.Int).Mod(x, q)
	kp := twopowmodphi(epoch, p)
	kq := twopowmodphi(epoch, q)
	xp.Exp(xp, kp, p)
	xq.Exp(xq, kq, q)
	xm := crtCompose(xp, xq, p, q)
	newState := make([]byte, len(state))
	copy(newState, state[:2+secpar/8])
	copy(newState[2+secpar/8:], mpiExport(xm, int(secpar)/8))
	copy(newState[2+2*secpar/8:], uint64Export(epoch))
	return newState
}

// fsprgGetKey extracts a deterministic key from state.
func fsprgGetKey(state []byte, keylen uint32, idx uint32) []byte {
	secpar := readSecpar(state)
	return detRandomize(int(keylen), state[2:2+2*secpar/8+8], idx)
}
