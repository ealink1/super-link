package ui

import "fyne.io/fyne/v2/dialog"

func (w *Window) closeTable(t *tableWorkspace) {
	close := func() {
		t.closed = true
		if t.cancel != nil {
			t.cancel()
		}
		delete(w.tables, t.item)
		w.tabs.Remove(t.item)
		w.syncDocuments()
	}
	if t.dirty() || t.busy {
		text := "丢弃当前未提交修改并关闭？"
		if t.busy {
			text += "\n正在执行的请求将被取消；已提交的写入不会回滚。"
		}
		dialog.ShowConfirm("关闭表工作台", text, func(ok bool) {
			if ok {
				close()
			}
		}, w.Window)
		return
	}
	close()
}

func (w *Window) closeImport(i *importWorkbench) {
	close := func() {
		i.closed = true
		if i.cancel != nil {
			i.cancel()
		}
		delete(w.imports, i.item)
		w.tabs.Remove(i.item)
		w.syncDocuments()
	}
	if i.busy {
		dialog.ShowConfirm("停止并关闭导入", "停止当前任务并关闭？已提交批次会保留；取消提交时需要检查服务端结果。", func(ok bool) {
			if ok {
				close()
			}
		}, w.Window)
		return
	}
	close()
}

func (w *Window) removeProfileDocuments(id string) {
	for _, page := range w.databases {
		if page.profile.ID == id {
			w.closeDatabaseTables(page)
		}
	}
	for item, s := range w.workspaces {
		if s.profile.ID == id {
			s.closed = true
			s.cancelOperation()
			if s.saveCancel != nil {
				s.saveCancel()
			}
			delete(w.workspaces, item)
			w.tabs.Remove(item)
		}
	}
	for item, t := range w.tables {
		if t.profile.ID == id {
			t.closed = true
			if t.cancel != nil {
				t.cancel()
			}
			delete(w.tables, item)
			w.tabs.Remove(item)
		}
	}
	for item, i := range w.imports {
		if i.profile.ID == id {
			i.closed = true
			if i.cancel != nil {
				i.cancel()
			}
			delete(w.imports, item)
			w.tabs.Remove(item)
		}
	}
	for item, d := range w.designers {
		if d.profile.ID == id {
			d.closed = true
			if d.cancel != nil {
				d.cancel()
			}
			delete(w.designers, item)
			w.tabs.Remove(item)
		}
	}
	w.syncDocuments()
}
