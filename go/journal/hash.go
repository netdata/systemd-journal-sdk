package journal

import (
	"encoding/binary"
	"math/bits"
)

func sipHash24(key UUID, msg []byte) uint64 {
	k0 := binary.LittleEndian.Uint64(key[0:8])
	k1 := binary.LittleEndian.Uint64(key[8:16])

	v0 := uint64(0x736f6d6570736575) ^ k0
	v1 := uint64(0x646f72616e646f6d) ^ k1
	v2 := uint64(0x6c7967656e657261) ^ k0
	v3 := uint64(0x7465646279746573) ^ k1

	round := func() {
		v0 += v1
		v1 = bits.RotateLeft64(v1, 13)
		v1 ^= v0
		v0 = bits.RotateLeft64(v0, 32)
		v2 += v3
		v3 = bits.RotateLeft64(v3, 16)
		v3 ^= v2
		v0 += v3
		v3 = bits.RotateLeft64(v3, 21)
		v3 ^= v0
		v2 += v1
		v1 = bits.RotateLeft64(v1, 17)
		v1 ^= v2
		v2 = bits.RotateLeft64(v2, 32)
	}

	remaining := msg
	for len(remaining) >= 8 {
		m := binary.LittleEndian.Uint64(remaining[:8])
		v3 ^= m
		round()
		round()
		v0 ^= m
		remaining = remaining[8:]
	}

	b := uint64(len(msg)) << 56
	for i, c := range remaining {
		b |= uint64(c) << (8 * uint(i))
	}

	v3 ^= b
	round()
	round()
	v0 ^= b
	v2 ^= 0xff
	for i := 0; i < 4; i++ {
		round()
	}

	return v0 ^ v1 ^ v2 ^ v3
}

func jenkinsHash64(data []byte) uint64 {
	a, b := jenkinsHashLittle2(data)
	return (uint64(a) << 32) | uint64(b)
}

func jenkinsHashLittle2(data []byte) (uint32, uint32) {
	length := uint32(len(data))
	a := uint32(0xdeadbeef) + length
	b := a
	c := a

	k := data
	for len(k) > 12 {
		a += binary.LittleEndian.Uint32(k[0:4])
		b += binary.LittleEndian.Uint32(k[4:8])
		c += binary.LittleEndian.Uint32(k[8:12])
		a, b, c = jenkinsMix(a, b, c)
		k = k[12:]
	}

	if len(k) == 0 {
		return c, b
	}
	a += jenkinsTailWord(k, 0)
	b += jenkinsTailWord(k, 4)
	c += jenkinsTailWord(k, 8)

	_, b, c = jenkinsFinal(a, b, c)
	return c, b
}

func jenkinsTailWord(data []byte, start int) uint32 {
	var word uint32
	for i := 0; i < 4 && start+i < len(data); i++ {
		word |= uint32(data[start+i]) << (8 * i)
	}
	return word
}

func jenkinsMix(a, b, c uint32) (uint32, uint32, uint32) {
	a -= c
	a ^= bits.RotateLeft32(c, 4)
	c += b
	b -= a
	b ^= bits.RotateLeft32(a, 6)
	a += c
	c -= b
	c ^= bits.RotateLeft32(b, 8)
	b += a
	a -= c
	a ^= bits.RotateLeft32(c, 16)
	c += b
	b -= a
	b ^= bits.RotateLeft32(a, 19)
	a += c
	c -= b
	c ^= bits.RotateLeft32(b, 4)
	b += a
	return a, b, c
}

func jenkinsFinal(a, b, c uint32) (uint32, uint32, uint32) {
	c ^= b
	c -= bits.RotateLeft32(b, 14)
	a ^= c
	a -= bits.RotateLeft32(c, 11)
	b ^= a
	b -= bits.RotateLeft32(a, 25)
	c ^= b
	c -= bits.RotateLeft32(b, 16)
	a ^= c
	a -= bits.RotateLeft32(c, 4)
	b ^= a
	b -= bits.RotateLeft32(a, 14)
	c ^= b
	c -= bits.RotateLeft32(b, 24)
	return a, b, c
}
