package ui

func (n *navigator) syncProfiles() {
	current := make(map[string]bool, len(n.owner.profiles))
	for _, p := range n.owner.profiles {
		current["connection:"+p.ID] = true
	}
	for id, node := range n.nodes {
		if node.kind == "connection" && !current[id] {
			n.removeChildren(node)
			if node.cancel != nil {
				node.cancel()
			}
			delete(n.nodes, id)
		}
	}
	n.roots = n.roots[:0]
	groups := map[string]*navNode{}
	var ungrouped, created []string
	ensure := func(name string) *navNode {
		// Parent metadata is local, but still bound traversal in case it is malformed.
		path := []string{name}
		seen := map[string]bool{name: true}
		for parent := n.owner.profileGroupParents[name]; parent != "" && !seen[parent]; parent = n.owner.profileGroupParents[parent] {
			seen[parent] = true
			path = append(path, parent)
		}
		var ancestor *navNode
		for i := len(path) - 1; i >= 0; i-- {
			groupName := path[i]
			id := "connection-group:" + groupName
			group := groups[id]
			if group == nil {
				group = n.nodes[id]
				if group == nil {
					group = &navNode{id: id, label: groupName, kind: "connection-group", loaded: true}
					n.nodes[id] = group
					created = append(created, id)
				}
				group.children = nil
				group.count = 0
				group.parent = ""
				groups[id] = group
				if ancestor == nil {
					n.roots = append(n.roots, id)
				} else {
					group.parent = ancestor.id
					ancestor.children = append(ancestor.children, id)
				}
			}
			ancestor = group
		}
		return ancestor
	}
	if n.owner.search.Text == "" {
		for _, name := range connectionGroupOrder(n.owner.profileGroups, n.owner.profileGroupParents) {
			ensure(name)
		}
	}
	for _, p := range n.owner.visible {
		id := "connection:" + p.ID
		node := n.nodes[id]
		if node == nil || node.revision != p.Revision {
			if node != nil {
				n.removeChildren(node)
				if node.cancel != nil {
					node.cancel()
				}
			}
			node = &navNode{id: id, label: p.Name, kind: "connection", profileID: p.ID, revision: p.Revision}
			n.nodes[id] = node
		}
		node.parent = ""
		if p.Group == "" {
			ungrouped = append(ungrouped, id)
			continue
		}
		group := ensure(p.Group)
		node.parent = group.id
		group.children = append(group.children, id)
		for ancestor := group; ancestor != nil; ancestor = n.nodes[ancestor.parent] {
			ancestor.count++
		}
	}
	n.roots = append(n.roots, ungrouped...)
	for id, node := range n.nodes {
		if node.kind == "connection-group" && groups[id] == nil {
			delete(n.nodes, id)
		}
	}
	n.tree.Refresh()
	for _, id := range created {
		n.tree.OpenBranch(id)
	}
}
