package ui

import (
	"fmt"
	"image/color"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
)

func (s *workspace) queryContent() fyne.CanvasObject {
	s.editor.TextStyle = fyne.TextStyle{Monospace: true}
	if s.code != nil {
		s.editor.TextStyle = queryEditorStyle()
	}
	connection := s.buildConnectionPicker()
	s.scopePicker.Scroll = container.ScrollNone
	s.schemaPicker = widget.NewSelect(nil, func(value string) {
		s.schemaName = value
		if s.completion != nil {
			s.completion.reset()
		}
		s.scheduleSave()
	})
	s.schemaPicker.PlaceHolder = "Schema"
	s.schemaPicker.Hide()
	s.schemaHost = container.NewGridWrap(fyne.NewSize(150, 32), s.schemaPicker)
	s.schemaHost.Hide()
	limit := widget.NewSelect([]string{"100", "1000", "5000", "10000"}, func(value string) { s.rowLimit, _ = strconv.Atoi(value) })
	limit.SetSelected("5000")
	s.execute.SetText("")
	s.execute.SetIcon(theme.MediaPlayIcon())
	s.execute.Importance = widget.HighImportance
	s.execute.OnTapped = s.runAdaptive
	s.stop.SetText("")
	s.stop.SetIcon(icon("stop"))
	s.runFrame, s.stopFrame = framedControl(s.execute), framedControl(s.stop)
	s.stopFrame.Hide()
	more := toolbarAction("", "connection-menu", s.queryMenu)
	top := container.NewHBox(container.NewGridWrap(fyne.NewSize(140, 32), connection), container.NewGridWrap(fyne.NewSize(166, 32), s.scopePicker), s.schemaHost, container.NewGridWrap(fyne.NewSize(80, 32), limit), s.runFrame, s.stopFrame, toolbarAction("", "save", s.saveNamedQuery), more, toolbarAction("", "search", s.findText), toolbarAction("", "wrap", func() {
		if s.editor.Wrapping == fyne.TextWrapOff {
			s.editor.Wrapping = fyne.TextWrapWord
		} else {
			s.editor.Wrapping = fyne.TextWrapOff
		}
		s.editor.Refresh()
	}), toolbarAction("", "settings", s.owner.settingsPage))
	gutter := widget.NewLabel("1")
	gutter.TextStyle = s.editor.TextStyle
	gutter.Alignment = fyne.TextAlignTrailing
	changed := s.editor.OnChanged
	updateGutter := func(text string) {
		count := strings.Count(text, "\n") + 1
		lines := make([]string, count)
		for i := range lines {
			lines[i] = fmt.Sprint(i + 1)
		}
		gutter.SetText(strings.Join(lines, "\n"))
	}
	updateGutter(s.editor.Text)
	s.editor.OnChanged = func(text string) {
		updateGutter(text)
		if changed != nil {
			changed(text)
		}
	}
	gutterHost := container.NewScroll(container.NewThemeOverride(gutter, editorTheme{}))
	gutterHost.Direction = container.ScrollNone
	var code fyne.CanvasObject
	if s.code != nil {
		code = container.NewThemeOverride(s.code, codeTheme{state: s.code.colors})
		s.code.onRun = s.runAdaptive
		s.code.onScroll = func(position fyne.Position) {
			if gutterHost.Offset.Y != position.Y {
				gutterHost.Offset.Y = position.Y
				gutterHost.Refresh()
			}
		}
	} else {
		code = container.NewThemeOverride(s.editor, editorTheme{})
	}
	// ScrollNone inherits the full label height. Bound the line-number viewport
	// so its document length cannot enlarge the query/results split or window.
	gutterViewport := container.New(queryGutterLayout{}, gutterHost)
	editor := container.NewBorder(nil, nil, gutterViewport, nil, code)
	query := container.NewBorder(top, s.status, nil, nil, editor)
	s.logTab = container.NewTabItem("日志", s.logPanel())
	s.parameterTab = container.NewTabItem("参数", s.parameterPanel())
	s.results.SetItems([]*container.TabItem{s.logTab, s.parameterTab, container.NewTabItem("结果", widget.NewLabel("执行查询后显示结果"))})
	s.results.SelectIndex(2)
	s.outputTabs = s.results
	paramChanged := s.editor.OnChanged
	s.editor.OnChanged = func(text string) {
		if s.completion != nil {
			s.completion.view.hide()
		}
		s.syncParameters()
		if paramChanged != nil {
			paramChanged(text)
		}
	}
	result := container.NewBorder(nil, nil, nil, nil, s.outputTabs)
	split := newQuerySplit(query, result)
	s.setupCompletion()
	return container.NewThemeOverride(split, querySplitTheme{})
}

type queryGutterLayout struct{}

func (queryGutterLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	scroll := objects[0].(*container.Scroll)
	return fyne.NewSize(scroll.Content.MinSize().Width, 0)
}

func (queryGutterLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	objects[0].Move(fyne.Position{})
	objects[0].Resize(size)
}

const queryEditorTextSize float32 = 18

func queryEditorStyle() fyne.TextStyle {
	style := fyne.TextStyle{Monospace: true}
	metrics, baseline := fyne.CurrentApp().Driver().RenderedTextSize("M国", queryEditorTextSize, style, nil)
	style.LineHeight = metrics.Height / queryEditorTextSize
	style.LineBaseline = baseline / queryEditorTextSize
	return style
}

type editorTheme struct{ Theme }

func (t editorTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameText {
		return queryEditorTextSize
	}
	return fyne.CurrentApp().Settings().Theme().Size(name)
}

func (t editorTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	base := fyne.CurrentApp().Settings().Theme()
	if name == theme.ColorNameInputBackground {
		return base.Color(theme.ColorNameBackground, variant)
	}
	if name == theme.ColorNameInputBorder || name == theme.ColorNamePrimary {
		return color.Transparent
	}
	return base.Color(name, variant)
}

func (s *workspace) runAdaptive() {
	text := s.editor.SelectedText()
	if strings.TrimSpace(text) == "" {
		text = s.editor.Text
	}
	request, err := s.executionRequest(text)
	if err != nil {
		s.status.SetText(err.Error())
		return
	}
	write, err := application.ClassifyProfile(s.profile, request)
	if err != nil {
		s.status.SetText(err.Error())
		return
	}
	request.Write = write
	s.runRequest(request)
}
func (s *workspace) queryMenu() {
	menu := fyne.NewMenu("查询", fyne.NewMenuItem("重命名", s.renameQuery), fyne.NewMenuItem("保存查询", s.saveNamedQuery), fyne.NewMenuItem("打开 SQL 文件", s.openSQLFile), fyne.NewMenuItem("导出 SQL 文件", s.exportSQLFile), fyne.NewMenuItem("SQL 执行历史", s.history), fyne.NewMenuItem("导出查询结果", s.exportResults), fyne.NewMenuItem("撤销", s.editor.Undo), fyne.NewMenuItem("重做", s.editor.Redo), fyne.NewMenuItem("刷新数据库列表", s.loadScopes), fyne.NewMenuItem("重新加载对象", s.refreshObjects))
	widget.ShowPopUpMenuAtPosition(menu, s.owner.Window.Canvas(), fyne.NewPos(750, 100))
}
func (s *workspace) findText() {
	needle := widget.NewEntry()
	showCardForm("查找", "查找", "取消", []*widget.FormItem{widget.NewFormItem("文本", needle)}, func(ok bool) {
		if ok {
			if index := strings.Index(s.editor.Text, needle.Text); index >= 0 {
				prefix := s.editor.Text[:index]
				s.editor.CursorRow = strings.Count(prefix, "\n")
				last := strings.LastIndex(prefix, "\n")
				s.editor.CursorColumn = len([]rune(prefix[last+1:]))
				s.editor.Refresh()
				s.owner.Window.Canvas().Focus(s.editor)
				s.status.SetText(fmt.Sprintf("找到文本，字符位置 %d", index+1))
			} else {
				s.status.SetText("未找到匹配文本")
			}
		}
	}, s.owner.Window)
}
