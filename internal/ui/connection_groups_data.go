package ui

import (
	"context"
	"slices"
)

func readConnectionGroupSnapshot(ctx context.Context, w *Window) (connectionGroupSnapshot, error) {
	profiles, err := w.Profiles.List(ctx)
	if err != nil {
		return connectionGroupSnapshot{}, err
	}
	groups, err := w.Profiles.Groups(ctx)
	if err != nil {
		return connectionGroupSnapshot{}, err
	}
	parents, err := w.Profiles.GroupParents(ctx)
	if err != nil {
		return connectionGroupSnapshot{}, err
	}
	options, err := w.Profiles.GroupOptions(ctx)
	if err != nil {
		return connectionGroupSnapshot{}, err
	}
	for _, p := range profiles {
		if p.Group != "" && !slices.Contains(groups, p.Group) {
			groups = append(groups, p.Group)
		}
	}
	return connectionGroupSnapshot{profiles: profiles, groups: groups, parents: parents, options: options, statuses: w.Engine.ConnectionStatuses()}, nil
}

// Preserve sibling order and tolerate old or invalid parent metadata.
func connectionGroupOrder(groups []string, parents map[string]string) []string {
	result := make([]string, 0, len(groups))
	seen := map[string]bool{}
	var visit func(string)
	visit = func(name string) {
		if seen[name] {
			return
		}
		seen[name] = true
		result = append(result, name)
		for _, child := range groups {
			if parents[child] == name {
				visit(child)
			}
		}
	}
	for _, name := range groups {
		if parents[name] == "" || !slices.Contains(groups, parents[name]) {
			visit(name)
		}
	}
	for _, name := range groups {
		visit(name)
	}
	return result
}

func connectionGroupDepth(name string, parents map[string]string) int {
	seen := map[string]bool{}
	depth := 0
	for parent := parents[name]; parent != "" && !seen[parent]; parent = parents[parent] {
		seen[parent] = true
		depth++
	}
	return depth
}
