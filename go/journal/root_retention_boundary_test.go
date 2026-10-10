package journal

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"
)

const boundaryStamp = uint64(1_800_000_000_000_000)

func boundaryAppend(log *Log, raw bool, stamp uint64) error {
	opts := EntryOptions{RealtimeUsec: stamp, MonotonicUsec: stamp - boundaryStamp + 1}
	if raw {
		return log.AppendRaw([][]byte{[]byte("MESSAGE=boundary")}, opts)
	}
	return log.Append([]Field{StringField("MESSAGE", "boundary")}, opts)
}

func runRootBoundary(log *Log, boundary string) error {
	switch boundary {
	case "close":
		return log.Close()
	case "close_without_retention":
		return log.CloseWithoutRetention()
	default:
		return boundaryAppend(log, strings.HasSuffix(boundary, "raw"), boundaryStamp+2_000_000)
	}
}

func boundaryConfig(boundary string) LogConfig {
	cfg := rootTestConfig()
	cfg.OpenMode = LogOpenEager
	if strings.HasPrefix(boundary, "count") {
		cfg.RotationPolicy = RotationPolicy{}.WithMaxEntries(1)
	}
	if strings.HasPrefix(boundary, "duration") {
		cfg.RotationPolicy = RotationPolicy{}.WithMaxDuration(time.Second)
	}
	return cfg
}

func copyBoundaryFile(t *testing.T, source, destination string) {
	t.Helper()
	data, err := os.ReadFile(source)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(destination, data, 0600); err != nil {
		t.Fatal(err)
	}
}

func requireBoundaryRejection(t *testing.T, dir string, events *[]LogLifecycleEvent, operation func() error) {
	t.Helper()
	before := rootEvidenceSnapshot(t, dir)
	eventCount := len(*events)
	err := operation()
	if err == nil || errors.Is(err, ErrWriterFailed) {
		t.Errorf("expected safe preflight rejection, got %v", err)
	}
	if !reflect.DeepEqual(before, rootEvidenceSnapshot(t, dir)) {
		t.Error("rejected operation changed root evidence")
	}
	if len(*events) != eventCount {
		t.Error("rejected operation emitted lifecycle events")
	}
	if t.Failed() {
		t.FailNow()
	}
}

func TestRootBoundaryPreservesReplacedActive(t *testing.T) {
	for _, boundary := range []string{"close", "close_without_retention", "count", "count_raw", "duration", "duration_raw"} {
		for _, empty := range []bool{false, true} {
			if empty && !strings.HasPrefix(boundary, "close") {
				continue
			}
			name := boundary
			if empty {
				name += "_empty"
			}
			t.Run(name, func(t *testing.T) {
				dir := t.TempDir()
				cfg := boundaryConfig(boundary)
				var events []LogLifecycleEvent
				cfg.Lifecycle = LogLifecycleObserverFunc(func(event LogLifecycleEvent) { events = append(events, event) })
				log := rootOpen(t, dir, cfg)
				if !empty {
					rootAppend(t, log, boundaryStamp)
				}
				active := log.ActivePath()
				parked := filepath.Join(dir, "parked-original")
				if err := os.Rename(active, parked); err != nil {
					t.Fatal(err)
				}
				replacement := rootFixture(t, t.TempDir(), cfg.Options.MachineID[0], []uint64{boundaryStamp, boundaryStamp + 1}, true)
				copyBoundaryFile(t, replacement, active)
				if _, err := log.InspectRootRetention(); err == nil {
					t.Fatal("replacement must fail live inventory")
				}
				requireBoundaryRejection(t, dir, &events, func() error { return runRootBoundary(log, boundary) })
				if strings.HasPrefix(boundary, "close") {
					if err := log.Sync(); !errors.Is(err, errWriterClosed) {
						t.Fatalf("rejected close did not release resources: %v", err)
					}
					return
				}
				if err := os.Rename(active, filepath.Join(dir, "parked-replacement")); err != nil {
					t.Fatal(err)
				}
				if err := os.Rename(parked, active); err != nil {
					t.Fatal(err)
				}
				if err := runRootBoundary(log, boundary); err != nil {
					t.Fatalf("rotation retry after restoring ownership: %v", err)
				}
				if len(rootInspect(t, dir).Files) != 2 {
					t.Fatal("retry must retain the archive and successor")
				}
			})
		}
	}
}

func TestRootBoundaryPreservesArchiveCollision(t *testing.T) {
	for _, boundary := range []string{"close", "close_without_retention", "count", "count_raw", "duration", "duration_raw"} {
		t.Run(boundary, func(t *testing.T) {
			dir := t.TempDir()
			cfg := boundaryConfig(boundary)
			var events []LogLifecycleEvent
			cfg.Lifecycle = LogLifecycleObserverFunc(func(event LogLifecycleEvent) { events = append(events, event) })
			log := rootOpen(t, dir, cfg)
			rootAppend(t, log, boundaryStamp)
			source := rootFixture(t, t.TempDir(), cfg.Options.MachineID[0], []uint64{boundaryStamp, boundaryStamp + 1}, false)
			target := filepath.Join(log.JournalDirectory(), filepath.Base(source))
			copyBoundaryFile(t, source, target)
			if _, err := log.InspectRootRetention(); err == nil {
				t.Fatal("collision must fail inventory")
			}
			requireBoundaryRejection(t, dir, &events, func() error { return runRootBoundary(log, boundary) })
			if strings.HasPrefix(boundary, "close") {
				return
			}
			if err := os.Rename(target, filepath.Join(dir, "parked-collision")); err != nil {
				t.Fatal(err)
			}
			if err := runRootBoundary(log, boundary); err != nil {
				t.Fatalf("rotation retry after moving collision: %v", err)
			}
			if len(rootInspect(t, dir).Files) != 2 {
				t.Fatal("retry must retain the archive and successor")
			}
		})
	}
}

func TestRootBoundaryPreservesUnexpectedLazyActive(t *testing.T) {
	for _, maintained := range []bool{false, true} {
		for _, raw := range []bool{false, true} {
			name := map[bool]string{false: "initial", true: "successor"}[maintained] + map[bool]string{false: "", true: "_raw"}[raw]
			t.Run(name, func(t *testing.T) {
				dir := t.TempDir()
				cfg := rootTestConfig()
				var events []LogLifecycleEvent
				cfg.Lifecycle = LogLifecycleObserverFunc(func(event LogLifecycleEvent) { events = append(events, event) })
				log := rootOpen(t, dir, cfg)
				if maintained {
					rootAppend(t, log, boundaryStamp)
					if err := log.SetRootRetentionPolicy(RetentionPolicy{}.WithMaxAge(time.Second)); err != nil {
						t.Fatal(err)
					}
					if _, err := log.MaintainRootRetention(time.UnixMicro(int64(boundaryStamp + 2_000_000))); err != nil {
						t.Fatal(err)
					}
				}
				if log.ActivePath() != "" {
					t.Fatal("fixture must be lazy")
				}
				unexpected := rootFixture(t, dir, cfg.Options.MachineID[0], []uint64{boundaryStamp, boundaryStamp + 1}, true)
				requireBoundaryRejection(t, dir, &events, func() error { return boundaryAppend(log, raw, boundaryStamp+3_000_000) })
				if err := os.Rename(unexpected, filepath.Join(dir, "parked-unexpected")); err != nil {
					t.Fatal(err)
				}
				if err := boundaryAppend(log, raw, boundaryStamp+3_000_000); err != nil {
					t.Fatalf("creation retry after restoring empty slot: %v", err)
				}
				if len(rootInspect(t, dir).Files) != 1 {
					t.Fatal("retry must create one successor")
				}
			})
		}
	}
}
