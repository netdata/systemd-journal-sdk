//go:build !linux

package journal

import "errors"

func readHostBootID() (UUID, error) {
	return UUID{}, errors.New("host boot id is not available on this platform")
}
