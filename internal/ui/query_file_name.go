package ui

import (
	"path/filepath"

	"fyne.io/fyne/v2/widget"
)

func (s *workspace) chooseSQLName(directory, text string) {
	name := widget.NewEntry()
	name.SetText("query.sql")
	showCardForm("导出 SQL 文件", "导出", "取消", []*widget.FormItem{widget.NewFormItem("文件名", name)}, func(ok bool) {
		if !ok || s.closed {
			return
		}
		filename := sqlFileName(name.Text)
		if filename == "" {
			s.status.SetText("文件名不能为空，也不能包含目录路径。")
			return
		}
		s.saveSQLFile(filepath.Join(directory, filename), text, false)
	}, s.owner.Window)
}
