package ui

func (w *Window) closeTable(t *tableWorkspace) {
	w.closeTableThen(t, nil)
}
func (w *Window) closeTableThen(t *tableWorkspace, done func(bool)) {
	close := func() {
		t.closed = true
		if t.cancel != nil {
			t.cancel()
		}
		delete(w.tables, t.item)
		w.tabs.Remove(t.item)
		w.syncDocuments()
		completeTabClose(done, true)
	}
	if t.dirty() || t.busy {
		text := "丢弃当前未提交修改并关闭？"
		if t.busy {
			text += "\n正在执行的请求将被取消；已提交的写入不会回滚。"
		}
		showConfirmDialog("关闭表工作台", text, func(ok bool) {
			if ok {
				close()
			} else {
				completeTabClose(done, false)
			}
		}, w.Window)
		return
	}
	close()
}

func (w *Window) closeImport(i *importWorkbench) {
	w.closeImportThen(i, nil)
}
func (w *Window) closeImportThen(i *importWorkbench, done func(bool)) {
	close := func() {
		i.closed = true
		if i.cancel != nil {
			i.cancel()
		}
		delete(w.imports, i.item)
		w.tabs.Remove(i.item)
		w.syncDocuments()
		completeTabClose(done, true)
	}
	if i.busy {
		showConfirmDialog("停止并关闭导入", "停止当前任务并关闭？已提交批次会保留；取消提交时需要检查服务端结果。", func(ok bool) {
			if ok {
				close()
			} else {
				completeTabClose(done, false)
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
