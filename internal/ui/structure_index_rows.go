package ui

import (
	"fmt"
	"sort"
	"strings"

	"github.com/ealink1/super-link/internal/upstream/connection"
)

type indexDisplayRow struct {
	name, columns, method string
	unique                bool
}

// Driver metadata has one entry per indexed column, while the designer selects
// and deletes whole indexes. Keep the display and action row mapping identical.
func indexDisplayRows(indexes []connection.IndexDefinition) []indexDisplayRow {
	groups := make([][]connection.IndexDefinition, 0)
	positions := map[string]int{}
	for _, index := range indexes {
		position, ok := positions[index.Name]
		if !ok {
			position = len(groups)
			positions[index.Name] = position
			groups = append(groups, nil)
		}
		groups[position] = append(groups[position], index)
	}
	rows := make([]indexDisplayRow, 0, len(groups))
	for _, columns := range groups {
		sort.SliceStable(columns, func(i, j int) bool { return columns[i].SeqInIndex < columns[j].SeqInIndex })
		names := make([]string, 0, len(columns))
		for _, column := range columns {
			name := column.ColumnName
			if column.SubPart > 0 {
				name += fmt.Sprintf("(%d)", column.SubPart)
			}
			names = append(names, name)
		}
		first := columns[0]
		rows = append(rows, indexDisplayRow{name: first.Name, columns: strings.Join(names, ", "), unique: first.NonUnique == 0, method: first.IndexType})
	}
	return rows
}
