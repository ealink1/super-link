package ui

import (
	"context"
	"errors"
	"fmt"
	"slices"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/google/uuid"
)

type workspace struct {
	code                           *codeEntry
	scopePicker                    *widget.SelectEntry
	schemaPicker, connectionPicker *widget.Select
	schemaHost                     *fyne.Container
	runFrame, stopFrame            fyne.CanvasObject
	schemaName                     string
	rowLimit                       int
	owner                          *Window
	profile                        domain.Profile
	descriptor                     domain.Descriptor
	id                             string
	title, savedID                 string
	savedRevision                  int64
	saving                         bool
	item                           *container.TabItem
	editor, scope                  *widget.Entry
	objects                        []domain.Object
	objectScope                    string
	objectList                     *widget.List
	selectedObject                 *domain.Object
	results                        *resultTabs
	lastResults                    []domain.Result
	lastRequest                    domain.Execution
	lastProfile                    domain.Profile
	outputTabs                     *resultTabs
	parameterTab, logTab           *container.TabItem
	parameterHost                  *fyne.Container
	parameters                     map[string]*queryParameterField
	logView                        *widget.Entry
	logs                           []string
	status                         *widget.Label
	execute, write, stop           *widget.Button
	cancel, saveCancel             context.CancelFunc
	generation                     uint64
	closed, busy                   bool
}

func (w *Window) openWorkspace(p domain.Profile, draft domain.Draft) *workspace {
	descriptor, err := domain.Resolve(p.Config.Type)
	if err != nil {
		w.showError(err)
		return nil
	}
	if draft.ID == "" {
		draft = domain.Draft{ID: uuid.NewString(), ProfileID: p.ID, Title: p.Name, Text: descriptor.DefaultQuery(), Scope: p.Config.Database}
		if descriptor.Key == "oracle" || descriptor.Key == "oceanbase" && strings.EqualFold(p.Config.OceanBaseProtocol, "oracle") {
			draft.Scope = ""
			draft.Text = "SELECT 1 FROM dual;"
		}
	}
	space := &workspace{owner: w, profile: p, descriptor: descriptor, id: draft.ID, title: draft.Title, schemaName: draft.Schema}
	space.build(draft)
	space.item = container.NewTabItem(p.Name, space.content())
	w.workspaces[space.item] = space
	w.tabs.Append(space.item)
	w.tabs.Select(space.item)
	space.scheduleSave()
	w.syncDocuments()
	return space
}
func (s *workspace) build(draft domain.Draft) {
	s.scopePicker = widget.NewSelectEntry(nil)
	s.scope = &s.scopePicker.Entry
	s.scope.SetText(draft.Scope)
	s.scope.SetPlaceHolder("数据库 / Namespace / 范围")
	s.scope.OnChanged = func(value string) {
		if s.objectScope != value && s.schemaPicker != nil {
			s.schemaName = ""
			s.schemaPicker.ClearSelected()
			s.schemaPicker.Hide()
			s.schemaHost.Hide()
		}
		s.scheduleSave()
		s.owner.syncDocuments()
	}
	s.scope.OnSubmitted = func(string) { s.refreshObjects() }
	s.setScopeOptions(nil)
	if s.descriptor.Family == domain.SQL {
		s.code = newCodeEntry()
		s.editor = &s.code.Entry
	} else {
		s.editor = widget.NewMultiLineEntry()
	}
	s.editor.SetText(draft.Text)
	s.editor.Wrapping = fyne.TextWrapOff
	s.editor.SetMinRowsVisible(9)
	s.editor.OnChanged = func(string) { s.scheduleSave() }
	s.editor.OnSubmitted = func(string) { s.run(false, "") }
	s.status = widget.NewLabel("就绪 · 结果最多 10,000 行 / 64 MiB")
	s.status.Wrapping = fyne.TextWrapWord
	s.results = newResultTabs(container.NewTabItem("结果", widget.NewLabel("执行查询后显示结果；双击单元格可查看完整值。")))
	s.objectList = widget.NewList(func() int { return len(s.objects) }, func() fyne.CanvasObject { return widget.NewLabel("") }, func(id widget.ListItemID, item fyne.CanvasObject) {
		object := s.objects[id]
		item.(*widget.Label).SetText(object.Name + "  " + object.Kind)
	})
	s.objectList.OnSelected = func(id widget.ListItemID) { object := s.objects[id]; s.selectedObject = &object }
	s.execute = widget.NewButtonWithIcon("读取 / 查询", theme.MediaPlayIcon(), func() { s.run(false, "") })
	s.write = widget.NewButton("写入 / 有副作用操作", func() { s.run(true, "") })
	if s.profile.ReadOnly {
		s.write.Disable()
	}
	s.stop = widget.NewButtonWithIcon("停止", theme.MediaStopIcon(), s.cancelOperation)
	s.stop.Disable()
}
func (s *workspace) content() fyne.CanvasObject {
	if s.descriptor.Family == domain.SQL {
		return s.queryContent()
	}
	scopes := widget.NewButton("列出范围", s.loadScopes)
	navTop := container.NewVBox(container.NewBorder(nil, nil, nil, scopes, s.scope), container.NewHBox(widget.NewButton("刷新对象", s.refreshObjects), widget.NewButton("生成读取命令", s.previewObject), widget.NewButton("结构 / 详情", s.schema)))
	nav := container.NewBorder(navTop, nil, nil, nil, s.objectList)
	editorTop := container.NewVBox(s.protocolControls(), container.NewHBox(s.execute, s.write, s.stop, widget.NewButton("撤销", s.editor.Undo), widget.NewButton("重做", s.editor.Redo), widget.NewButton("历史", s.history), widget.NewButton("导出结果", s.exportResults)))
	editor := container.NewBorder(editorTop, nil, nil, nil, s.editor)
	queryAndResult := container.NewVSplit(editor, s.results)
	queryAndResult.Offset = 0.43
	split := container.NewHSplit(nav, queryAndResult)
	split.Offset = 0.22
	return container.NewBorder(nil, s.status, nil, nil, split)
}
func (s *workspace) draft() domain.Draft {
	return domain.Draft{ID: s.id, ProfileID: s.profile.ID, Title: s.title, Scope: s.scope.Text, Schema: s.schemaName, Text: s.editor.Text}
}
func (s *workspace) scheduleSave() {
	if s.closed || s.owner.jobs.closing.Load() {
		return
	}
	if s.saveCancel != nil {
		s.saveCancel()
	}
	draft := s.draft()
	s.owner.jobs.runWithCancel(&s.saveCancel, func(ctx context.Context) (any, error) {
		timer := time.NewTimer(time.Second)
		defer timer.Stop()
		select {
		case <-timer.C:
		case <-ctx.Done():
			return nil, ctx.Err()
		}
		return nil, s.owner.Store.SaveDraft(ctx, draft)
	}, func(_ any, err error) {
		if s.closed || errors.Is(err, context.Canceled) {
			return
		}
		if err != nil {
			s.status.SetText("草稿保存失败：" + err.Error())
		}
	})
}
func (s *workspace) cancelOperation() {
	if s.cancel != nil {
		s.cancel()
		s.status.SetText("正在取消并释放本次请求…")
	}
}
func (s *workspace) begin() {
	s.cancelOperation()
	s.generation++
	s.busy = true
	s.execute.Disable()
	if s.runFrame != nil {
		s.runFrame.Hide()
		s.stopFrame.Show()
	}
	if s.scopePicker != nil {
		s.scopePicker.Disable()
	}
	if s.schemaPicker != nil {
		s.schemaPicker.Disable()
	}
	if s.connectionPicker != nil {
		s.connectionPicker.Disable()
	}
	s.write.Disable()
	s.stop.Enable()
	s.status.SetText("正在执行…")
}
func (s *workspace) finish() {
	s.busy = false
	s.stop.Disable()
	s.execute.Enable()
	if s.runFrame != nil {
		s.runFrame.Show()
		s.stopFrame.Hide()
	}
	if s.scopePicker != nil {
		s.scopePicker.Enable()
	}
	if s.schemaPicker != nil {
		s.schemaPicker.Enable()
	}
	if s.connectionPicker != nil {
		s.connectionPicker.Enable()
	}
	if !s.profile.ReadOnly {
		s.write.Enable()
	}
}
func (s *workspace) run(write bool, confirmation string) {
	if s.closed || s.busy {
		return
	}
	text := s.editor.SelectedText()
	if strings.TrimSpace(text) == "" {
		text = s.editor.Text
	}
	request, err := s.executionRequest(text)
	if err != nil {
		s.status.SetText(err.Error())
		return
	}
	request.Write, request.Confirmation = write, confirmation
	s.runRequest(request)
}
func (s *workspace) runRequest(request domain.Execution) {
	if s.closed || s.busy {
		return
	}
	s.begin()
	generation := s.generation
	profileID := s.profile.ID
	profile := s.profile
	started := time.Now()
	s.owner.jobs.runWithCancel(&s.cancel, func(ctx context.Context) (any, error) { return s.owner.Engine.Execute(ctx, profileID, request) }, func(value any, err error) {
		if s.closed || generation != s.generation {
			return
		}
		s.finish()
		var confirmation *domain.ConfirmationRequired
		if errors.As(err, &confirmation) {
			s.status.SetText("等待确认目标和操作…")
			description := fmt.Sprintf("连接：%s\n环境：%s\n范围：%s\n操作：\n%s\n\n执行后可能修改服务端状态。中断写入不等于回滚。", s.profile.Name, s.profile.Environment, request.Scope, previewValue(request.Text))
			showConfirmDialog("确认有副作用操作", description, func(ok bool) {
				if ok {
					request.Confirmation = confirmation.Fingerprint
					s.runRequest(request)
				} else {
					s.status.SetText("已取消执行")
				}
			}, s.owner.Window)
			return
		}
		if value != nil {
			results := value.([]domain.Result)
			if len(results) > 0 {
				s.lastRequest, s.lastProfile = request, profile
				s.showResults(results)
			}
		}
		if err != nil {
			s.appendLog(started, err, value)
			s.status.SetText("执行失败：" + err.Error())
			return
		}
		s.appendLog(started, nil, value)
		s.status.SetText("执行完成 · 本地历史保存的是脱敏文本")
	})
}
func (s *workspace) showResults(results []domain.Result) {
	items := make([]*container.TabItem, 0, len(results))
	if s.logTab != nil {
		items = append(items, s.logTab, s.parameterTab)
	}
	for i, result := range results {
		view := s.owner.resultView(result)
		if s.descriptor.Family == domain.SQL {
			view = s.queryResultView(result, i)
		}
		items = append(items, container.NewTabItem(fmt.Sprintf("结果 %d", i+1), view))
	}
	s.results.SetItems(items)
	for i, result := range results {
		offset := 0
		if s.logTab != nil {
			offset = 2
		}
		s.results.counts[items[i+offset]] = len(result.Rows)
	}
	s.lastResults = results
	if s.outputTabs != nil {
		s.outputTabs.SelectIndex(2)
	}
}
func (s *workspace) loadScopes() {
	if s.busy {
		return
	}
	s.begin()
	s.status.SetText("正在连接并加载可选数据库 / 范围…")
	generation := s.generation
	profileID := s.profile.ID
	s.owner.jobs.runWithCancel(&s.cancel, func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		return s.owner.Engine.Scopes(ctx, profileID)
	}, func(value any, err error) {
		if s.closed || generation != s.generation {
			return
		}
		s.finish()
		if err != nil {
			s.status.SetText(err.Error())
			return
		}
		scopes := value.([]string)
		s.setScopeOptions(scopes)
		if len(scopes) == 0 {
			s.status.SetText("连接成功，但未发现可访问的数据库 / 范围。请检查账号权限或填写范围。")
			return
		}
		if s.scope.Text != "" && slices.Contains(scopes, s.scope.Text) {
			s.refreshObjects()
			return
		}
		if len(scopes) == 1 {
			s.scope.SetText(scopes[0])
			s.refreshObjects()
			return
		}
		if s.descriptor.Family == domain.SQL {
			s.status.SetText(fmt.Sprintf("已连接 · %d 个数据库；请从数据库选择器选择范围。", len(scopes)))
			return
		}
		labels := append([]string(nil), scopes...)
		for i, value := range labels {
			if value == "" {
				labels[i] = "(public / 默认范围)"
			}
		}
		selected := widget.NewSelect(labels, nil)
		selected.SetSelected(labels[0])
		dialog.ShowForm("选择范围", "选择", "取消", []*widget.FormItem{widget.NewFormItem("范围", selected)}, func(ok bool) {
			if !ok {
				return
			}
			for i, label := range labels {
				if label == selected.Selected {
					s.scope.SetText(scopes[i])
					s.scheduleSave()
					s.refreshObjects()
					break
				}
			}
		}, s.owner.Window)
		s.status.SetText(fmt.Sprintf("发现 %d 个范围", len(scopes)))
	})
}
func (s *workspace) refreshObjects() {
	if s.busy {
		return
	}
	s.begin()
	s.status.SetText("正在连接并加载库表 / 对象…")
	generation := s.generation
	scope := s.scope.Text
	profileID := s.profile.ID
	s.selectedObject = nil
	s.owner.jobs.runWithCancel(&s.cancel, func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		return s.owner.Engine.Objects(ctx, profileID, scope)
	}, func(value any, err error) {
		if s.closed || generation != s.generation {
			return
		}
		s.finish()
		if err != nil {
			s.status.SetText(err.Error())
			return
		}
		s.objects = value.([]domain.Object)
		s.objectScope = scope
		s.updateSchemas()
		s.objectList.UnselectAll()
		s.objectList.Refresh()
		s.status.SetText(fmt.Sprintf("显示 %d 个对象；Redis 最多 1,000 Key / Nacos 每页 100 配置", len(s.objects)))
	})
}
func (s *workspace) schema() {
	if s.selectedObject == nil || s.busy {
		return
	}
	object := *s.selectedObject
	profileID := s.profile.ID
	s.begin()
	generation := s.generation
	s.owner.jobs.runWithCancel(&s.cancel, func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		return s.owner.Engine.Schema(ctx, profileID, object.Scope, object.Name)
	}, func(value any, err error) {
		if s.closed || generation != s.generation {
			return
		}
		s.finish()
		if err != nil {
			s.status.SetText(err.Error())
			return
		}
		entry := widget.NewMultiLineEntry()
		entry.SetText(value.(string))
		entry.SetMinRowsVisible(16)
		d := dialog.NewCustom("结构 / 详情", "关闭", entry, s.owner.Window)
		d.Resize(fyne.NewSize(850, 600))
		d.Show()
		s.status.SetText("详情已载入")
	})
}
func (s *workspace) history() {
	profileID := s.profile.ID
	s.owner.jobs.run(func(ctx context.Context) (any, error) { return s.owner.Store.History(ctx, profileID) }, func(value any, err error) {
		if s.closed {
			return
		}
		if err != nil {
			s.owner.showError(err)
			return
		}
		history := value.([]domain.History)
		list := widget.NewList(func() int { return len(history) }, func() fyne.CanvasObject { return widget.NewLabel("") }, func(id widget.ListItemID, item fyne.CanvasObject) {
			h := history[id]
			item.(*widget.Label).SetText(h.CreatedAt.Format("01-02 15:04") + "  " + previewValue(h.Text))
		})
		list.OnSelected = func(id widget.ListItemID) {
			entry := widget.NewMultiLineEntry()
			entry.SetText(history[id].Text)
			entry.SetMinRowsVisible(10)
			dialog.ShowCustom("历史（脱敏，不能直接重放）", "关闭", entry, s.owner.Window)
		}
		d := dialog.NewCustom("最近 100 条执行记录", "关闭", list, s.owner.Window)
		d.Resize(fyne.NewSize(900, 550))
		d.Show()
	})
}
func (w *Window) closeTab(item *container.TabItem) {
	if page := w.databases[item]; page != nil {
		w.closeDatabaseTables(page)
		return
	}
	if designer := w.designers[item]; designer != nil {
		w.closeDesigner(designer)
		return
	}
	if table := w.tables[item]; table != nil {
		w.closeTable(table)
		return
	}
	if importer := w.imports[item]; importer != nil {
		w.closeImport(importer)
		return
	}
	space := w.workspaces[item]
	if space == nil {
		w.tabs.Remove(item)
		w.syncDocuments()
		return
	}
	space.cancelOperation()
	if space.saveCancel != nil {
		space.saveCancel()
	}
	// Block edits and new autosaves while persisting the closed flag, otherwise
	// a delayed autosave could reopen this tab on the next application start.
	space.closed = true
	space.editor.Disable()
	space.scope.Disable()
	draft := space.draft()
	draft.Closed = true
	w.jobs.run(func(ctx context.Context) (any, error) { return nil, w.Store.SaveDraft(ctx, draft) }, func(_ any, err error) {
		if err != nil {
			space.closed = false
			space.editor.Enable()
			space.scope.Enable()
			space.finish()
			w.showError(err)
			return
		}
		space.closed = true
		w.tabs.Remove(item)
		delete(w.workspaces, item)
		w.syncDocuments()
	})
}
func (w *Window) restoreDrafts() {
	w.jobs.run(func(ctx context.Context) (any, error) { return w.Store.Drafts(ctx) }, func(value any, err error) {
		if err != nil {
			w.showError(err)
			return
		}
		for _, draft := range value.([]domain.Draft) {
			if draft.Closed {
				continue
			}
			for _, p := range w.profiles {
				if p.ID == draft.ProfileID {
					w.openWorkspace(p, draft)
					break
				}
			}
		}
		close(w.ready)
	})
}
