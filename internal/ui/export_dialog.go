package ui

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/datafile"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

type exportSource struct {
	profile domain.Profile
	result  domain.Result
	request domain.Execution
	table   string
}

type exportForm struct {
	owner       *Window
	source      exportSource
	format      *widget.Select
	mode        *widget.RadioGroup
	table, path *widget.Entry
	bom         *widget.Check
	checks      []*widget.Check
	status      *widget.Label
	save        *widget.Button
	modal       dialog.Dialog
	closed      bool
}

func (w *Window) exportDialog(source exportSource) {
	f := &exportForm{owner: w, source: source, format: widget.NewSelect(datafile.Formats, nil), mode: widget.NewRadioGroup([]string{"完整查询结果（重新执行）", "当前已加载结果"}, nil), table: widget.NewEntry(), path: widget.NewEntry(), bom: widget.NewCheck("CSV 添加 UTF-8 BOM（兼容 Excel）", nil), status: widget.NewLabel("")}
	f.format.SetSelected("XLSX")
	f.mode.SetSelected("完整查询结果（重新执行）")
	if source.request.Text == "" {
		f.mode.SetSelected("当前已加载结果")
		f.mode.Disable()
		f.status.SetText("此结果仅支持导出已加载数据。完整导出需要单条无参数的读取查询。")
	}
	f.table.SetText(source.table)
	f.table.SetPlaceHolder("INSERT SQL 的目标表名")
	f.table.Hide()
	f.bom.SetChecked(true)
	f.bom.Hide()
	home, _ := os.UserHomeDir()
	f.path.SetText(filepath.Join(home, "Downloads", "query-result.xlsx"))
	f.format.OnChanged = f.changeFormat
	f.status.Wrapping = fyne.TextWrapWord
	f.save = widget.NewButton("导出", f.submit)
	f.save.Importance = widget.HighImportance
	browse := action("选择目录", "folder-open", func() {
		w.chooseLocalPath("选择导出目录", true, nil, func(path string) {
			if !f.closed {
				f.path.SetText(filepath.Join(path, filepath.Base(f.path.Text)))
			}
		})
	})
	form := widget.NewForm(widget.NewFormItem("格式", f.format), widget.NewFormItem("数据范围", f.mode), widget.NewFormItem("目标表", f.table), widget.NewFormItem("输出文件", container.NewBorder(nil, nil, nil, browse, f.path)))
	content := container.NewBorder(form, container.NewVBox(f.bom, f.status, container.NewHBox(layout.NewSpacer(), f.save)), nil, nil, f.columnPanel())
	f.modal = dialog.NewCustom("导出查询结果", "取消", content, w.Window)
	f.modal.SetOnClosed(func() { f.closed = true })
	f.modal.Resize(fyne.NewSize(760, 530))
	f.modal.Show()
}

func (f *exportForm) columnPanel() fyne.CanvasObject {
	columns := container.NewVBox()
	for _, c := range f.source.result.Columns {
		check := widget.NewCheck(c.Name, nil)
		check.SetChecked(true)
		f.checks = append(f.checks, check)
		columns.Add(check)
	}
	scroll := container.NewVScroll(columns)
	scroll.SetMinSize(fyne.NewSize(300, 170))
	set := func(value bool) {
		for _, c := range f.checks {
			c.SetChecked(value)
		}
	}
	return container.NewBorder(container.NewHBox(widget.NewLabel("选择导出字段"), layout.NewSpacer(), action("全选", "cell-select", func() { set(true) }), action("清空", "trash", func() { set(false) })), nil, nil, nil, scroll)
}

func (f *exportForm) changeFormat(value string) {
	f.path.SetText(strings.TrimSuffix(f.path.Text, filepath.Ext(f.path.Text)) + datafile.Extension(value))
	f.table.Hide()
	f.bom.Hide()
	if value == "INSERT SQL" {
		f.table.Show()
	}
	if value == "CSV" {
		f.bom.Show()
	}
}

func (f *exportForm) options() (datafile.Options, error) {
	indices := []int{}
	for i, c := range f.checks {
		if c.Checked {
			indices = append(indices, i)
		}
	}
	if len(indices) == 0 {
		return datafile.Options{}, errors.New("至少选择一个字段")
	}
	o := datafile.Options{Format: f.format.Selected, Columns: indices, Dialect: f.source.profile.SQLDialect(), BOM: f.bom.Checked}
	if o.Format == "INSERT SQL" {
		parts, err := sqlworkbench.ParsePath(f.table.Text)
		if err != nil {
			return o, err
		}
		for i, part := range parts {
			parts[i], err = sqlworkbench.Quote(o.Dialect, part)
			if err != nil {
				return o, err
			}
		}
		o.Table = strings.Join(parts, ".")
	}
	return o, nil
}

func (f *exportForm) submit() {
	options, err := f.options()
	if err != nil {
		f.status.SetText(err.Error())
		return
	}
	path := strings.TrimSpace(f.path.Text)
	if path == "" {
		f.status.SetText("请选择导出文件")
		return
	}
	full := f.mode.Selected == "完整查询结果（重新执行）"
	f.save.Disable()
	f.owner.jobs.run(func(context.Context) (any, error) {
		_, err := os.Lstat(path)
		if errors.Is(err, os.ErrNotExist) {
			return false, nil
		}
		return err == nil, err
	}, func(value any, err error) {
		if f.closed {
			return
		}
		f.save.Enable()
		if err != nil {
			f.status.SetText(err.Error())
			return
		}
		run := func(overwrite bool) {
			if f.closed {
				return
			}
			f.modal.Hide()
			f.owner.startExport(f.source, options, path, overwrite, full)
		}
		if value.(bool) {
			dialog.ShowConfirm("覆盖文件", "导出成功后替换「"+filepath.Base(path)+"」？", func(ok bool) {
				if ok {
					run(true)
				}
			}, f.owner.Window)
		} else {
			run(false)
		}
	})
}
