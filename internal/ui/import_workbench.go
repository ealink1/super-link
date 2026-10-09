package ui

import (
	"context"
	"errors"
	"fmt"
	"path/filepath"
	"strconv"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/datafile"
	"github.com/ealink1/super-link/internal/domain"
)

type importWorkbench struct {
	owner             *Window
	profile           domain.Profile
	object            domain.Object
	info              domain.TableInfo
	file, sheet       *widget.Entry
	encoding          *widget.Select
	empty             *widget.Check
	batch             *widget.Select
	status            *widget.Label
	preview, mappings *fyne.Container
	dataset           datafile.Dataset
	loadedPath        string
	loadedOptions     datafile.ReadOptions
	mapping           []*widget.Select
	item              *container.TabItem
	runButton         *widget.Button
	cancel            context.CancelFunc
	busy, closed      bool
}

func (w *Window) importTable(p domain.Profile, object domain.Object, info domain.TableInfo) {
	i := &importWorkbench{owner: w, profile: p, object: object, info: info, file: widget.NewEntry(), sheet: widget.NewEntry(), encoding: widget.NewSelect([]string{"UTF-8", "GBK", "GB18030"}, nil), empty: widget.NewCheck("将空单元格转换为 NULL", nil), batch: widget.NewSelect([]string{"100", "500", "1000"}, nil), status: widget.NewLabel("选择文件后预览，并确认字段映射。"), preview: container.NewStack(), mappings: container.NewVBox()}
	i.encoding.SetSelected("UTF-8")
	i.batch.SetSelected("500")
	i.sheet.SetPlaceHolder("XLSX 工作表名称，留空使用第一个")
	browse := action("选择文件", "folder-open", func() {
		w.chooseLocalPath("选择导入数据文件", false, nil, func(path string) {
			if i.closed {
				return
			}
			i.file.SetText(path)
			i.loadFile()
		})
	})
	i.runButton = widget.NewButton("开始导入", func() { i.run("") })
	i.runButton.Importance = widget.HighImportance
	i.runButton.Disable()
	top := container.NewVBox(container.NewHBox(widget.NewLabelWithStyle("数据导入", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), layout.NewSpacer(), widget.NewLabel(p.Name+" · "+object.Scope+" · "+object.Name)), widget.NewForm(widget.NewFormItem("文件", container.NewBorder(nil, nil, nil, browse, i.file)), widget.NewFormItem("编码", i.encoding), widget.NewFormItem("工作表", i.sheet)), container.NewHBox(action("重新预览", "refresh", i.loadFile), i.empty, layout.NewSpacer(), widget.NewLabel("每批提交"), i.batch, i.runButton, action("停止", "stop", func() {
		if i.cancel != nil {
			i.cancel()
		}
	})))
	tabs := container.NewAppTabs(container.NewTabItem("源数据预览", i.preview), container.NewTabItem("字段映射", container.NewVScroll(i.mappings)))
	content := container.NewBorder(top, i.status, nil, nil, tabs)
	i.item = container.NewTabItem("数据导入 · "+object.Name, content)
	w.imports[i.item] = i
	w.tabs.Append(i.item)
	w.tabs.Select(i.item)
	w.syncDocuments()
	// Long-running import callbacks check the document still belongs to the window.
	if p.ReadOnly || p.Config.Protection.RestrictDataImport {
		i.status.SetText("此连接限制数据导入；仍可预览文件。")
		i.runButton.Disable()
	}
}
func (i *importWorkbench) active() bool {
	if i.closed {
		return false
	}
	for _, item := range i.owner.tabs.Items {
		if item == i.item {
			return true
		}
	}
	return false
}
func (i *importWorkbench) loadFile() {
	if i.busy || i.file.Text == "" {
		return
	}
	i.busy = true
	i.runButton.Disable()
	i.status.SetText("正在解析文件…")
	path := i.file.Text
	options := datafile.ReadOptions{Encoding: i.encoding.Selected, Sheet: i.sheet.Text, EmptyAsNull: i.empty.Checked}
	i.owner.jobs.runWithCancel(&i.cancel, func(ctx context.Context) (any, error) { return datafile.ReadFile(ctx, path, options) }, func(value any, err error) {
		if !i.active() {
			return
		}
		i.busy = false
		if err != nil {
			i.status.SetText("文件读取失败：" + err.Error())
			return
		}
		i.dataset = value.(datafile.Dataset)
		i.loadedPath, i.loadedOptions = path, options
		i.showPreview()
		i.buildMapping()
		i.status.SetText(fmt.Sprintf("%s · %d 行 · %d 列", filepath.Base(path), len(i.dataset.Rows), len(i.dataset.Columns)))
		if !i.profile.ReadOnly && !i.profile.Config.Protection.RestrictDataImport {
			i.runButton.Enable()
		}
	})
}
func (i *importWorkbench) showPreview() {
	columns := make([]domain.Column, len(i.dataset.Columns))
	for n, name := range i.dataset.Columns {
		columns[n] = domain.Column{Name: name}
	}
	rows := i.dataset.Rows[:min(100, len(i.dataset.Rows))]
	model := gridModel{columns: columns, length: func() int { return len(rows) }, value: func(row, col int) any { return rows[row][col] }, selected: map[int]bool{}, inspect: func(row, col int) { i.owner.showCell(columns[col].Name, rows[row][col]) }}
	i.preview.Objects = []fyne.CanvasObject{newDataGrid(model)}
	i.preview.Refresh()
}
func (i *importWorkbench) buildMapping() {
	i.mapping = nil
	i.mappings.Objects = nil
	names := []string{"不导入"}
	for _, c := range i.info.Columns {
		if domain.WritableColumn(c) {
			names = append(names, c.Name)
		}
	}
	for _, source := range i.dataset.Columns {
		picker := widget.NewSelect(names, nil)
		picker.SetSelected("不导入")
		for _, target := range i.info.Columns {
			if source == target.Name && domain.WritableColumn(target) {
				picker.SetSelected(source)
				break
			}
		}
		i.mapping = append(i.mapping, picker)
		i.mappings.Add(container.NewBorder(nil, nil, widget.NewLabel(source), nil, picker))
	}
	i.mappings.Refresh()
}
func (i *importWorkbench) request() domain.ImportRequest {
	batch, _ := strconv.Atoi(i.batch.Selected)
	r := domain.ImportRequest{Object: i.object, Revision: i.profile.Revision, Columns: i.dataset.Columns, Rows: i.dataset.Rows, Mapping: map[int]string{}, BatchSize: batch}
	for source, picker := range i.mapping {
		if picker.Selected != "不导入" && picker.Selected != "" {
			r.Mapping[source] = picker.Selected
		}
	}
	return r
}
func (i *importWorkbench) run(confirmation string) {
	if !i.active() || i.busy || len(i.dataset.Rows) == 0 || i.profile.ReadOnly || i.profile.Config.Protection.RestrictDataImport {
		return
	}
	if i.file.Text != i.loadedPath || (datafile.ReadOptions{Encoding: i.encoding.Selected, Sheet: i.sheet.Text, EmptyAsNull: i.empty.Checked}) != i.loadedOptions {
		i.status.SetText("文件或解析选项已修改，请重新预览后再导入。")
		return
	}
	request := i.request()
	request.Confirmation = confirmation
	i.busy = true
	i.runButton.Disable()
	i.status.SetText("正在导入…")
	id := i.profile.ID
	i.owner.jobs.runWithCancel(&i.cancel, func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 10*time.Minute)
		defer cancel()
		return i.owner.Engine.ImportRows(ctx, id, request)
	}, func(value any, err error) {
		if !i.active() {
			return
		}
		i.busy = false
		i.runButton.Enable()
		var required *domain.ConfirmationRequired
		if errors.As(err, &required) {
			i.status.SetText("等待确认导入")
			dialog.ShowConfirm("确认导入数据", fmt.Sprintf("文件：%s\n连接：%s · %s\n表：%s.%s\n导入 %d 行，每批 %d 行。\n失败或停止时，已经提交的批次会保留。", filepath.Base(i.loadedPath), i.profile.Name, i.profile.Environment, i.object.Scope, i.object.Name, len(request.Rows), request.BatchSize), func(ok bool) {
				if ok {
					i.run(required.Fingerprint)
				}
			}, i.owner.Window)
			return
		}
		result := domain.ImportResult{}
		if value != nil {
			result = value.(domain.ImportResult)
		}
		if err != nil {
			i.status.SetText(fmt.Sprintf("已确认提交 %d 行 · %s", result.Committed, err.Error()))
			if result.Attempted {
				i.runButton.Disable()
			}
			return
		}
		i.status.SetText(fmt.Sprintf("导入完成 · %d 行 · %d 个批次", result.Committed, result.Batches))
		i.runButton.Disable()
		for _, t := range i.owner.tables {
			if t.profile.ID == id && t.object == i.object && !t.dirty() {
				t.refresh()
			}
		}
	})
}
