package journal

import (
	"fmt"
)

// VerificationError indicates a journal file failed structural integrity verification.
type VerificationError struct {
	Reason string
}

func (e *VerificationError) Error() string {
	if e == nil {
		return "verification error"
	}
	return e.Reason
}

// VerifyFile validates the structural integrity of a journal file.
// It opens the file, validates the header, and walks all entries and
// their referenced data objects. Compressed files are decompressed.
// For sealed journals, tag/HMAC verification is not yet implemented.
func VerifyFile(path string) error {
	r, err := OpenFile(path)
	if err != nil {
		// Any open or decompression failure is a verification failure.
		msg := fmt.Sprintf("journal verification failed: corrupt or unreadable file: %v", err)
		return &VerificationError{Reason: msg}
	}
	defer r.Close()

	if err := r.SeekHead(); err != nil {
		return &VerificationError{
			Reason: fmt.Sprintf("journal verification failed: seek failed: %v", err),
		}
	}

	for {
		ok, err := r.Step()
		if err != nil {
			return &VerificationError{
				Reason: fmt.Sprintf("journal verification failed: corrupt entry chain: %v", err),
			}
		}
		if !ok {
			break
		}
		if _, err := r.GetEntry(); err != nil {
			return &VerificationError{
				Reason: fmt.Sprintf("journal verification failed: corrupt entry data: %v", err),
			}
		}
	}
	return nil
}
