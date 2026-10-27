package main

import "fmt"

func readSDKFilter(cfg benchConfig) (counts, error) {
	reader, err := openSDKReader(cfg.surface, cfg.inputs, cfg.options)
	if err != nil {
		return counts{}, err
	}
	defer reader.Close()
	install := func() {
		reader.FlushMatches()
		for _, match := range cfg.filterMatches {
			reader.AddMatch(match)
		}
	}
	install()
	var result counts
	for repetition := 0; repetition < cfg.filterRepetitions; repetition++ {
		if repetition != 0 && cfg.filterRebuild {
			install()
		}
		if cfg.filterRealtimeSet {
			err = reader.SeekRealtimeUsec(cfg.filterRealtime)
		} else {
			err = seekSDKReader(reader, cfg.direction)
		}
		if err != nil {
			return counts{}, err
		}
		for count := uint64(0); cfg.filterLimit == 0 || count < cfg.filterLimit; count++ {
			ok, err := stepSDKReader(reader, cfg.direction)
			if err != nil {
				return counts{}, err
			}
			if !ok {
				break
			}
			if cfg.mode == "sdk-filter-digest" {
				if err := addSDKDigest(&result, reader); err != nil {
					return counts{}, err
				}
			} else {
				realtime, err := reader.GetRealtimeUsec()
				if err != nil {
					return counts{}, fmt.Errorf("filtered realtime: %w", err)
				}
				result.addRecordMarker(realtime)
			}
		}
	}
	return result, nil
}

func readFacadeFilter(cfg benchConfig) (counts, error) {
	reader, err := openFacade(cfg.surface, cfg.inputs, cfg.options)
	if err != nil {
		return counts{}, err
	}
	defer reader.Close()
	install := func() {
		reader.FlushMatches()
		for _, match := range cfg.filterMatches {
			reader.AddMatch(match)
		}
	}
	install()
	var result counts
	for repetition := 0; repetition < cfg.filterRepetitions; repetition++ {
		if repetition != 0 && cfg.filterRebuild {
			install()
		}
		if cfg.filterRealtimeSet {
			err = reader.SeekRealtimeUsec(cfg.filterRealtime)
		} else {
			err = seekFacade(reader, cfg.direction)
		}
		if err != nil {
			return counts{}, err
		}
		for count := uint64(0); cfg.filterLimit == 0 || count < cfg.filterLimit; count++ {
			ok, err := stepFacade(reader, cfg.direction)
			if err != nil {
				return counts{}, err
			}
			if ok == 0 {
				break
			}
			if cfg.mode == "facade-filter-digest" {
				if err := addFacadeDigest(&result, reader); err != nil {
					return counts{}, err
				}
			} else if err := addFacadeNext(&result, reader); err != nil {
				return counts{}, err
			}
		}
	}
	return result, nil
}
