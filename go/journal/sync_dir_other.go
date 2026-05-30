//go:build !unix

package journal

func syncParentDir(_ string) error {
	return nil
}
