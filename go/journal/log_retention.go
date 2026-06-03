package journal

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"time"
)

type retentionRun struct {
	files        []archivedJournalFile
	activePath   string
	total        uint64
	fileCount    int
	deletedPaths []string
}

// EnforceRetention applies the configured retention policy without requiring a
// rotation or close. The current active file is counted in retention envelopes
// and protected from deletion.
func (l *Log) EnforceRetention() error {
	if l.closed {
		return errWriterClosed
	}
	return l.enforceRetention(l.activePath())
}

func (l *Log) enforceRetentionOnOpen() error {
	if l.openRetention || l.writer == nil {
		return nil
	}
	if err := l.enforceRetention(l.activePath()); err != nil {
		return err
	}
	l.openRetention = true
	return nil
}

func (l *Log) enforceRetention(protectedPath string) error {
	run, err := l.newRetentionRun(protectedPath)
	if err != nil {
		return err
	}
	if err := l.enforceRetentionLimits(&run); err != nil {
		return err
	}
	if err := syncJournalDirectory(l.machineDir); err != nil {
		return err
	}
	if len(run.deletedPaths) > 0 {
		l.emitLifecycle(LogLifecycleEvent{
			Type:         LogLifecycleDeleted,
			Reason:       LogLifecycleReasonRetention,
			DeletedPaths: run.deletedPaths,
		})
	}
	return nil
}

func (l *Log) newRetentionRun(protectedPath string) (retentionRun, error) {
	files, _, err := l.archivedFiles()
	if err != nil {
		return retentionRun{}, err
	}
	activePath := protectedPath
	if activePath == "" {
		activePath = l.activePath()
	}
	var total uint64
	activeInFiles := false
	for _, file := range files {
		if activePath != "" && file.path == activePath {
			activeInFiles = true
		}
		total = saturatingAdd(total, file.size)
	}
	activeExtraFile := false
	if activePath != "" && !activeInFiles {
		if activeInfo, err := os.Stat(activePath); err == nil {
			activeExtraFile = true
			activeSize, err := l.retainedSize(activePath, uint64(activeInfo.Size()))
			if err != nil {
				return retentionRun{}, err
			}
			total = saturatingAdd(total, activeSize)
		} else if !errors.Is(err, os.ErrNotExist) {
			return retentionRun{}, err
		}
	}

	sort.Slice(files, func(i, j int) bool {
		if files[i].headRealtime != files[j].headRealtime {
			return files[i].headRealtime < files[j].headRealtime
		}
		if files[i].headSeqnum != files[j].headSeqnum {
			return files[i].headSeqnum < files[j].headSeqnum
		}
		return files[i].path < files[j].path
	})

	fileCount := len(files)
	if activeExtraFile {
		fileCount++
	}
	return retentionRun{files: files, activePath: activePath, total: total, fileCount: fileCount}, nil
}

func (l *Log) enforceRetentionLimits(run *retentionRun) error {
	for l.retention.MaxFiles != nil && run.fileCount > *l.retention.MaxFiles {
		if ok, err := l.deleteOldestRetainedFile(run, nil); err != nil || !ok {
			return err
		}
	}
	for l.retention.MaxBytes != nil && run.total > *l.retention.MaxBytes && len(run.files) > 0 {
		if ok, err := l.deleteOldestRetainedFile(run, nil); err != nil || !ok {
			return err
		}
	}
	return l.enforceRetentionMaxAge(run)
}

func (l *Log) enforceRetentionMaxAge(run *retentionRun) error {
	if l.retention.MaxAge == nil {
		return nil
	}
	cutoff := retentionAgeCutoff(*l.retention.MaxAge)
	for len(run.files) > 0 {
		if run.files[0].headRealtime > cutoff {
			break
		}
		ok, err := l.deleteOldestRetainedFile(run, &cutoff)
		if err != nil || !ok {
			return err
		}
	}
	return nil
}

func retentionAgeCutoff(maxAge time.Duration) uint64 {
	cutoff := uint64(time.Now().UnixMicro())
	maxAgeUsec := durationUsec(maxAge)
	if cutoff >= maxAgeUsec {
		return cutoff - maxAgeUsec
	}
	return 0
}

func (l *Log) deleteOldestRetainedFile(run *retentionRun, cutoff *uint64) (bool, error) {
	deleteIndex := -1
	for i, file := range run.files {
		if cutoff != nil && file.headRealtime > *cutoff {
			break
		}
		if run.activePath == "" || file.path != run.activePath {
			deleteIndex = i
			break
		}
	}
	if deleteIndex == -1 {
		return false, nil
	}
	deleted := run.files[deleteIndex]
	if err := os.Remove(deleted.path); err != nil && !errors.Is(err, os.ErrNotExist) {
		return false, err
	}
	run.deletedPaths = append(run.deletedPaths, deleted.path)
	run.total = saturatingSub(run.total, deleted.size)
	run.files = append(run.files[:deleteIndex], run.files[deleteIndex+1:]...)
	run.fileCount--
	return true, nil
}

func (l *Log) archivedFiles() ([]archivedJournalFile, uint64, error) {
	entries, err := os.ReadDir(l.machineDir)
	if err != nil {
		return nil, 0, err
	}
	var files []archivedJournalFile
	var total uint64
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		archived, ok := parseArchivedJournalName(entry.Name(), l.source)
		if !ok {
			continue
		}
		info, err := entry.Info()
		if errors.Is(err, os.ErrNotExist) {
			continue
		}
		if err != nil {
			return nil, 0, err
		}
		archived.path = filepath.Join(l.machineDir, entry.Name())
		archived.size, err = l.retainedSize(archived.path, uint64(info.Size()))
		if err != nil {
			return nil, 0, err
		}
		files = append(files, archived)
		total = saturatingAdd(total, archived.size)
	}
	return files, total, nil
}

func (l *Log) retainedSize(path string, fallback uint64) (uint64, error) {
	size := committedJournalSize(path, fallback)
	if l.artifacts == nil {
		return size, nil
	}
	artifactSize, err := l.artifacts.JournalArtifactSize(path)
	if err != nil {
		return 0, err
	}
	return saturatingAdd(size, artifactSize), nil
}

func validateRetentionPolicy(policy RetentionPolicy) error {
	if policy.MaxFiles != nil && *policy.MaxFiles <= 0 {
		return fmt.Errorf("%w: retention max files must be greater than 0", errInvalidJournal)
	}
	if policy.MaxBytes != nil && *policy.MaxBytes == 0 {
		return fmt.Errorf("%w: retention max bytes must be greater than 0", errInvalidJournal)
	}
	if policy.MaxAge != nil && *policy.MaxAge <= 0 {
		return fmt.Errorf("%w: retention max age must be greater than 0", errInvalidJournal)
	}
	return nil
}
