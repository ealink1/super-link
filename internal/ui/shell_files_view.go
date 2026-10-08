package ui

import (
	"fmt"
	"image/color"
	"path"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

func newShellFilesView(f *shellFiles) *fyne.Container {
	f.search = widget.NewEntry()
	f.search.SetPlaceHolder("搜索文件…")
	f.search.OnChanged = func(string) { f.filterFiles() }
	f.list = widget.NewList(func() int { return len(f.shown) }, func() fyne.CanvasObject { return newShellFileRow() }, func(id widget.ListItemID, object fyne.CanvasObject) {
		row := object.(*shellFileRow)
		index := f.shown[id]
		row.update(f.files[index])
		row.selected = index == f.selected
		row.Refresh()
		row.selectRow = func() { f.list.Select(id) }
		row.open = func() {
			f.selected = index
			if f.files[index].Directory {
				f.enter()
			} else {
				f.openLocal()
			}
		}
		row.menu = func(position fyne.Position) { f.selected = index; f.list.Select(id); f.showMenu(position) }
	})
	f.list.HideSeparators = true
	f.list.OnSelected = func(id widget.ListItemID) { f.selected = f.shown[id]; f.list.Refresh() }
	f.list.OnUnselected = func(id widget.ListItemID) {
		if id < len(f.shown) && f.selected == f.shown[id] {
			f.selected = -1
		}
		f.list.Refresh()
	}
	f.directory.OnSubmitted = func(string) { f.refresh() }
	up := shellButtonView(shellButton("上级", "", false, func() { f.directory.SetText(path.Join(f.directory.Text, "..")); f.refresh() }))
	directory := shellFixed(shellBorder(nil, nil, up, shellButtonView(shellButton("进入", "folder", false, f.enter)), f.directory), 0, 30)
	actions := shellHBox(shellButtonView(shellButton("刷新", "refresh-cw", false, f.refresh)), shellFixed(layout.NewSpacer(), 8, 0), shellButtonView(shellButton("上传", "", false, f.upload)), shellFixed(layout.NewSpacer(), 8, 0), shellButtonView(shellButton("下载", "", false, f.download)))
	f.cancelButton = shellButton("", "x", false, func() {
		if f.cancel != nil {
			f.cancel()
			f.status.SetText("正在取消…")
		}
	})
	f.cancelButton.Disable()
	headings := newShellFileRow()
	headings.header = true
	headings.name.SetText("名称")
	headings.size.SetText("大小")
	headings.modified.SetText("时间")
	headings.icon.Hide()
	header := shellVBox(directory, shellFixed(layout.NewSpacer(), 0, 8), actions, shellFixed(layout.NewSpacer(), 0, 8), f.search, shellFixed(layout.NewSpacer(), 0, 8), headings, shellLine())
	f.progress = widget.NewProgressBar()
	f.progress.TextFormatter = func() string { return "" }
	f.transferLabel = widget.NewLabel("")
	f.transferLabel.Truncation = fyne.TextTruncateEllipsis
	f.transferLabel.TextStyle.Monospace = true
	progress := container.NewThemeOverride(f.progress, shellTransferTheme{newShellTheme()})
	transfer := container.NewStack(progress, shellInset(shellBorder(nil, nil, nil, shellFixed(shellButtonView(f.cancelButton), 20, 20), container.NewThemeOverride(f.transferLabel, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: 11}, shade: shellTextColor})), 4))
	f.transferBar = shellVBox(shellFixed(transfer, 0, 26))
	f.transferBar.Hide()
	footer := shellVBox(shellLine(), shellFixed(layout.NewSpacer(), 0, 6), f.transferBar, shellLabel(f.status, 11))
	return container.NewStack(shellRectangle(monitorSurfaceColor, 0, nil), container.NewThemeOverride(shellInset(shellBorder(header, footer, nil, nil, f.list), 12), shellFileListTheme{newShellTheme()}))
}

func (f *shellFiles) filterFiles() {
	f.shown = nil
	needle := strings.ToLower(f.search.Text)
	for i, file := range f.files {
		if strings.Contains(strings.ToLower(file.Name), needle) {
			f.shown = append(f.shown, i)
		}
	}
	f.selected = -1
	f.list.UnselectAll()
	f.list.Refresh()
}

type shellFileRow struct {
	widget.BaseWidget
	icon                      *canvas.Image
	name, size, modified      *widget.Label
	selectRow, open           func()
	menu                      func(fyne.Position)
	selected, hovered, header bool
}

func newShellFileRow() *shellFileRow {
	r := &shellFileRow{icon: canvas.NewImageFromResource(shellIcon("file-code", false)), name: widget.NewLabel(""), size: widget.NewLabel(""), modified: widget.NewLabel("")}
	r.icon.FillMode = canvas.ImageFillContain
	for _, label := range []*widget.Label{r.name, r.size, r.modified} {
		label.Truncation = fyne.TextTruncateEllipsis
	}
	r.size.TextStyle.Monospace = true
	r.modified.TextStyle.Monospace = true
	r.ExtendBaseWidget(r)
	return r
}
func (r *shellFileRow) update(file transport.File) {
	if file.Directory {
		r.icon.Resource = monitorMetricIcon("DIR", color.NRGBA{112, 101, 255, 255})
	} else {
		r.icon.Resource = monitorMetricIcon("FILE", monitorMutedColor)
	}
	r.icon.Refresh()
	r.name.SetText(file.Name)
	size := "—"
	if !file.Directory {
		size = shellFileRowSize(file.Size)
	}
	r.size.SetText(size)
	modified := "—"
	if !file.Modified.IsZero() {
		modified = file.Modified.Format("1/2 15:04")
	}
	r.modified.SetText(modified)
}
func shellFileSize(size int64) string {
	for i, unit := range []string{"GiB", "MiB", "KiB"} {
		divisor := float64(int64(1) << uint(30-i*10))
		if float64(size) >= divisor {
			return fmt.Sprintf("%.1f %s", float64(size)/divisor, unit)
		}
	}
	return fmt.Sprintf("%d B", size)
}
func shellFileRowSize(size int64) string {
	for i, unit := range []string{"GB", "MB", "KB"} {
		divisor := float64(int64(1) << uint(30-i*10))
		if float64(size) >= divisor {
			return fmt.Sprintf("%.2f %s", float64(size)/divisor, unit)
		}
	}
	return fmt.Sprintf("%d B", size)
}
func (r *shellFileRow) CreateRenderer() fyne.WidgetRenderer {
	label := func(l *widget.Label, size float32, shade color.Color) fyne.CanvasObject {
		return container.NewThemeOverride(l, monitorTintTheme{shellLabelTheme: shellLabelTheme{shellTheme: newShellTheme(), size: size}, shade: shade})
	}
	nameSize := float32(13)
	nameShade := color.Color(shellFileTextColor)
	if r.header {
		nameSize = 12
		nameShade = monitorMutedColor
		r.name.TextStyle.Bold = true
		r.size.TextStyle.Monospace = false
		r.modified.TextStyle.Monospace = false
	}
	renderer := &shellFileRowRenderer{row: r, objects: []fyne.CanvasObject{
		shellRectangle(monitorSurfaceColor, 0, nil), shellRectangle(color.NRGBA{112, 101, 255, 255}, 0, nil),
		r.icon, label(r.name, nameSize, nameShade), label(r.size, 11, monitorMutedColor), label(r.modified, 12, monitorMutedColor),
		shellRectangle(shellFileSeparatorColor, 0, nil), shellText("↑", 12, false, color.NRGBA{137, 126, 255, 255}),
	}}
	renderer.objects[1].Hide()
	if !r.header {
		renderer.objects[7].Hide()
	}
	return renderer
}

type shellFileRowRenderer struct {
	row     *shellFileRow
	objects []fyne.CanvasObject
}

func (*shellFileRowRenderer) MinSize() fyne.Size { return fyne.NewSize(280, 37) }
func (r *shellFileRowRenderer) Layout(size fyne.Size) {
	place := func(i int, x, y, w, h float32) {
		r.objects[i].Move(fyne.NewPos(x, y))
		r.objects[i].Resize(fyne.NewSize(max(0, w), max(0, h)))
	}
	place(0, 0, 0, size.Width, size.Height)
	place(1, 0, 0, 1, size.Height)
	place(2, 16, (size.Height-16)/2, 16, 16)
	timeEdge := size.Width - 88
	edge := max(120, timeEdge-80)
	nameX, nameWidth := float32(44), edge-52
	if r.row.header {
		nameX, nameWidth = 16, 30
	}
	columns := []struct {
		index    int
		x, width float32
	}{{3, nameX, nameWidth}, {4, edge, timeEdge - edge - 4}, {5, timeEdge, size.Width - timeEdge - 8}}
	for _, column := range columns {
		height := r.objects[column.index].MinSize().Height
		place(column.index, column.x, (size.Height-height)/2-2, column.width, height)
	}
	place(6, 0, size.Height-1, size.Width, 1)
	place(7, 46, (size.Height-r.objects[7].MinSize().Height)/2-2, 12, r.objects[7].MinSize().Height)
}
func (r *shellFileRowRenderer) Objects() []fyne.CanvasObject { return r.objects }
func (*shellFileRowRenderer) Destroy()                       {}
func (r *shellFileRowRenderer) Refresh() {
	background := r.objects[0].(*shellPrimitive)
	background.fill = monitorSurfaceColor
	if r.row.selected || r.row.hovered {
		background.fill = monitorBadgeFill
	}
	if r.row.selected && !r.row.header {
		r.objects[1].Show()
	} else {
		r.objects[1].Hide()
	}
	if r.row.header {
		r.objects[7].Show()
	} else {
		r.objects[7].Hide()
	}
	for _, object := range r.objects {
		object.Refresh()
	}
	r.Layout(r.row.Size())
}
func (r *shellFileRow) MouseIn(*desktop.MouseEvent)    { r.hovered = true; r.Refresh() }
func (r *shellFileRow) MouseMoved(*desktop.MouseEvent) {}
func (r *shellFileRow) MouseOut()                      { r.hovered = false; r.Refresh() }

var shellFileSeparatorColor = shellTone{day: color.NRGBA{207, 210, 215, 255}, night: color.NRGBA{64, 70, 79, 255}}

func (r *shellFileRow) Tapped(*fyne.PointEvent) {
	if r.selectRow != nil {
		r.selectRow()
	}
}
func (r *shellFileRow) DoubleTapped(*fyne.PointEvent) {
	if r.open != nil {
		r.open()
	}
}
func (r *shellFileRow) TappedSecondary(e *fyne.PointEvent) {
	if r.menu != nil {
		r.menu(e.AbsolutePosition)
	}
}

type shellTransferTheme struct{ shellTheme }

func (t shellTransferTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNamePrimary {
		return color.NRGBA{157, 143, 238, 255}
	}
	return t.shellTheme.Color(name, variant)
}

var shellFileTextColor = shellTone{day: color.NRGBA{78, 80, 104, 255}, night: color.NRGBA{202, 206, 220, 255}}

type shellFileListTheme struct{ shellTheme }

func (t shellFileListTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameSelection || name == theme.ColorNameHover {
		return resolveShellColor(monitorBadgeFill)
	}
	return t.shellTheme.Color(name, variant)
}
