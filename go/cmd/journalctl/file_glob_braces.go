//go:build cgo

package main

// Windows has no libc glob. Expand its brace alternatives before Unicode globbing.
func expandFileBraces(pattern string) []string {
	for start := 0; start < len(pattern); start++ {
		if pattern[start] != '{' {
			continue
		}
		depth, part := 1, start+1
		var alternatives []string
		for end := start + 1; end < len(pattern); end++ {
			switch pattern[end] {
			case '{':
				depth++
			case ',':
				if depth == 1 {
					alternatives = append(alternatives, pattern[part:end])
					part = end + 1
				}
			case '}':
				depth--
				if depth != 0 {
					continue
				}
				if len(alternatives) > 0 {
					alternatives = append(alternatives, pattern[part:end])
					var result []string
					for _, alternative := range alternatives {
						result = append(result, expandFileBraces(pattern[:start]+alternative+pattern[end+1:])...)
					}
					return result
				}
				end = len(pattern)
			}
		}
	}
	return []string{pattern}
}
