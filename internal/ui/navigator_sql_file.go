package ui

import "github.com/ealink1/super-link/internal/domain"

func (n *navigator) executeSQLFile(node *navNode) {
	p, ok := n.nodeProfile(node)
	if !ok {
		return
	}
	scope := node.scope
	n.owner.chooseLocalPath("执行 SQL 文件", false, []string{"sql"}, func(path string) { n.loadSQLFile(p, scope, path) })
}

func (n *navigator) loadSQLFile(p domain.Profile, scope, path string) {
	n.showSQLFileDialog(p, scope, path)
}
