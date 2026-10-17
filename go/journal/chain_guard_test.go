package journal

import (
	"bytes"
	"encoding/binary"
	"errors"
	"os"
	"testing"
)

func TestChainGuardAllSmallGraphs(t *testing.T) {
	for size := 1; size <= 5; size++ {
		graphs := 1
		for range size {
			graphs *= size + 1
		}
		for encoding := range graphs {
			links := make([]uint64, size+1)
			code := encoding
			for node := 1; node <= size; node++ {
				links[node] = uint64(code % (size + 1))
				code /= size + 1
			}
			for start := 1; start <= size; start++ {
				seen := make(map[uint64]bool)
				node := uint64(start)
				for node != 0 && !seen[node] {
					seen[node] = true
					node = links[node]
				}
				wantCycle := node != 0
				for _, descending := range []bool{false, true} {
					var guard chainGuard
					node = uint64(start)
					cycle := false
					for steps := 0; node != 0 && steps < 4*size+4; steps++ {
						next := links[node]
						if !guard.advance(node, next, descending) {
							cycle = true
							break
						}
						node = next
					}
					if cycle != wantCycle || (!cycle && node != 0) {
						t.Fatalf("links=%v start=%d descending=%v cycle=%v want=%v", links, start, descending, cycle, wantCycle)
					}
				}
			}
		}
	}
}

func TestUniqueChainCycleAndAcyclicReverseOrder(t *testing.T) {
	for _, cycle := range []bool{false, true} {
		t.Run(map[bool]string{false: "acyclic", true: "cycle"}[cycle], func(t *testing.T) {
			path := createReaderMessageJournal(t, 1, []Field{
				StringField("MESSAGE", "one"), StringField("MESSAGE", "two"), StringField("MESSAGE", "three"),
			})
			r := mustOpenReaderFile(t, path)
			head, ok, err := r.findFieldHeadDataOffset([]byte("MESSAGE"))
			if err != nil || !ok {
				t.Fatalf("field lookup: %v, %v", ok, err)
			}
			var nodes []uint64
			for offset := head; offset != 0; {
				nodes = append(nodes, offset)
				h, err := r.readDataHeaderAt(offset)
				if err != nil {
					t.Fatal(err)
				}
				offset = h.nextFieldOffset
			}
			fieldBucket := r.header.fieldHashTableOffset + r.hash([]byte("MESSAGE"))%(r.header.fieldHashTableSize/hashItemSize)*hashItemSize
			buf, err := r.readSlice(fieldBucket, hashItemSize)
			if err != nil {
				t.Fatal(err)
			}
			fieldOffset := parseHashItem(buf).head
			for fieldOffset != 0 {
				h, payload, err := r.readFieldObjectAt(fieldOffset)
				if err != nil {
					t.Fatal(err)
				}
				if bytes.Equal(payload, []byte("MESSAGE")) {
					break
				}
				fieldOffset = h.nextHashOffset
			}
			if fieldOffset == 0 || len(nodes) != 3 {
				t.Fatal("unexpected fixture graph")
			}
			if err := r.Close(); err != nil {
				t.Fatal(err)
			}
			data, err := os.ReadFile(path)
			if err != nil {
				t.Fatal(err)
			}
			if cycle {
				binary.LittleEndian.PutUint64(data[nodes[len(nodes)-1]+32:], head)
			} else {
				binary.LittleEndian.PutUint64(data[fieldOffset+32:], nodes[len(nodes)-1])
				for i, offset := range nodes {
					var next uint64
					if i > 0 {
						next = nodes[i-1]
					}
					binary.LittleEndian.PutUint64(data[offset+32:], next)
				}
			}
			if err := os.WriteFile(path, data, 0o600); err != nil {
				t.Fatal(err)
			}
			r = mustOpenReaderFile(t, path)
			defer r.Close()
			_, err = r.QueryUnique("MESSAGE")
			if cycle && !errors.Is(err, errCorruptObject) {
				t.Fatalf("QueryUnique error=%v", err)
			}
			if !cycle && err != nil {
				t.Fatal(err)
			}
			if err := r.QueryUniqueState("MESSAGE"); err != nil {
				t.Fatal(err)
			}
			count := 0
			for count < 16 {
				_, ok, nextErr := r.EnumerateUniquePayload()
				err = nextErr
				if err != nil || !ok {
					break
				}
				count++
			}
			if cycle && !errors.Is(err, errCorruptObject) {
				t.Fatalf("stateful error=%v", err)
			}
			if !cycle && err != nil {
				t.Fatal(err)
			}
			if !cycle && count != 3 {
				t.Fatalf("acyclic values=%d", count)
			}
			if cycle {
				if _, ok, err := r.EnumerateUniquePayload(); err != nil || ok {
					t.Fatalf("failed iterator not exhausted: %v %v", ok, err)
				}
			}
		})
	}
}
