package main

import (
	"fmt"
	j "github.com/netdata/systemd-journal-sdk/go/journal"
	"os"
	"path/filepath"
	"strconv"
	"time"
)

func must(err error) {
	if err != nil {
		panic(err)
	}
}
func id(n byte) (v j.UUID) {
	for i := range v {
		v[i] = n
	}
	return
}
func options(n byte) j.Options {
	return j.Options{MachineID: id(n), BootID: id(2), SeqnumID: id(3), DataHashTableBuckets: 32, FieldHashTableBuckets: 16}
}
func config() j.LogConfig {
	return j.LogConfig{Options: options(1), Source: "history", StrictSystemdNaming: true, RootRetention: true}
}
func inventory(root string) {
	inv, e := j.InspectRootRetention(root, "history")
	must(e)
	for _, f := range inv.Files {
		fmt.Printf("%s %d %d %d %d %d %d %t\n", f.MachineID.String(), f.Bytes, f.Entries, f.HeadSeqnum, f.TailSeqnum, f.HeadRealtime, f.TailRealtime, f.Active)
	}
}
func main() {
	mode, root := os.Args[1], os.Args[2]
	now, e := strconv.ParseInt(os.Args[3], 10, 64)
	must(e)
	day := int64(24 * time.Hour / time.Microsecond)
	switch mode {
	case "fixture":
		count := 0
		if len(os.Args) > 4 {
			count, e = strconv.Atoi(os.Args[4])
			must(e)
		}
		for n := byte(21); n <= 23; n++ {
			times := []int64{now - 40*day}
			if n == 22 {
				times = append(times, now-20*day)
			}
			if count > 0 {
				times = make([]int64, count)
				for i := range times {
					times[i] = now + int64(i)
				}
				if n != 21 {
					continue
				}
			}
			dir := filepath.Join(root, id(n).String())
			must(os.MkdirAll(dir, 0700))
			path := filepath.Join(dir, "history.journal")
			w, e := j.Create(path, options(n))
			must(e)
			for i, t := range times {
				must(w.Append([]j.Field{j.StringField("MESSAGE", "fixture")}, j.EntryOptions{RealtimeUsec: uint64(t), MonotonicUsec: uint64(i + 1)}))
			}
			if n == 23 {
				must(w.Close())
			} else {
				must(w.ArchiveTo(filepath.Join(dir, fmt.Sprintf("history@%s-%016x-%016x.journal", id(3).String(), 1, times[0]))))
			}
		}
	case "benchmark":
		start := time.Now()
		for i := 0; i < 1000; i++ {
			inv, e := j.InspectRootRetention(root, "history")
			must(e)
			if len(inv.Files) != 1 {
				panic("benchmark fixture")
			}
		}
		fmt.Println(time.Since(start).Nanoseconds() / 1000)
	case "inspect":
		inventory(root)
	case "maintain":
		l, e := j.NewLog(root, config())
		must(e)
		must(l.SetRootRetentionPolicy(j.RetentionPolicy{}.WithMaxAge(30 * 24 * time.Hour)))
		r, e := l.MaintainRootRetention(time.UnixMicro(now))
		must(e)
		fmt.Printf("deleted %d\n", r.DeletedFiles)
		must(l.CloseWithoutRetention())
		inventory(root)
	case "reject":
		_, e := j.InspectRootRetention(root, "history")
		if e == nil {
			panic("accepted unsafe inventory")
		}
		cfg := config()
		cfg.RetentionPolicy = j.RetentionPolicy{}.WithMaxBytes(1)
		l, e := j.NewLog(root, cfg)
		if e == nil {
			l.CloseWithoutRetention()
			panic("accepted unsafe startup")
		}
		fmt.Println("rejected")
	case "live":
		l, e := j.NewLog(root, config())
		must(e)
		appendEntry := func(t int64) {
			must(l.Append([]j.Field{j.StringField("MESSAGE", "live")}, j.EntryOptions{RealtimeUsec: uint64(t), MonotonicUsec: 1}))
		}
		appendEntry(now)
		must(l.SetRootRetentionPolicy(j.RetentionPolicy{}.WithMaxAge(30 * 24 * time.Hour)))
		r, e := l.MaintainRootRetention(time.UnixMicro(now + 31*day))
		must(e)
		if len(r.Inventory.Files) != 0 || l.ActivePath() != "" {
			panic("expiry not lazy")
		}
		appendEntry(now + 32*day)
		must(l.CloseWithoutRetention())
		inventory(root)
	case "read":
		inv, e := j.InspectRootRetention(root, "history")
		must(e)
		for _, f := range inv.Files {
			r, e := j.OpenFile(f.Path)
			must(e)
			for i := uint64(0); i < f.Entries; i++ {
				must(r.Next())
				t, e := r.GetRealtimeUsec()
				must(e)
				fmt.Println(t)
			}
			must(r.Close())
		}
	default:
		panic("unknown mode")
	}
}
