package ui

import "strings"

// An empty fenced block still needs one real editable line. Importing adjacent
// fences otherwise leaves only hidden syntax with nowhere to place a caret.
func noteEditableEmptyCode(source string) string {
	lines := strings.Split(source, "\n")
	result := make([]string, 0, len(lines))
	opening := -1
	for i, line := range lines {
		if strings.HasPrefix(strings.TrimSpace(line), "```") {
			if opening < 0 {
				opening = i
			} else {
				if i == opening+1 {
					result = append(result, "")
				}
				opening = -1
			}
		}
		result = append(result, line)
	}
	return strings.Join(result, "\n")
}
