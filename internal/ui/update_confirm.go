package ui

import (
	"fmt"
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

var updateInk = color.NRGBA{R: 30, G: 41, B: 59, A: 255}
var updateMuted = color.NRGBA{R: 100, G: 116, B: 139, A: 255}

func updateText(text string, size float32, bold bool, ink color.Color) *canvas.Text {
	item := canvas.NewText(text, ink)
	item.TextSize = size
	item.TextStyle.Bold = bold
	item.Alignment = fyne.TextAlignCenter
	return item
}

func updateSpace(height float32) fyne.CanvasObject {
	item := canvas.NewRectangle(color.Transparent)
	item.SetMinSize(fyne.NewSize(1, height))
	return item
}

func updateRounded(content fyne.CanvasObject, fill color.Color, radius float32) fyne.CanvasObject {
	background := canvas.NewRectangle(fill)
	background.CornerRadius = radius
	return container.NewStack(background, content)
}

func updateVersion(text string, fresh bool) fyne.CanvasObject {
	ink := color.Color(updateInk)
	fill := color.Color(color.NRGBA{R: 226, G: 232, B: 240, A: 255})
	if fresh {
		ink = color.White
		fill = color.NRGBA{R: 74, G: 140, B: 255, A: 255}
	}
	label := updateText(text, 14, true, ink)
	label.TextStyle.Monospace = true
	return updateRounded(container.NewPadded(label), fill, 10)
}

func updateInfo(icon fyne.Resource, caption, value string, tint color.NRGBA) fyne.CanvasObject {
	image := widget.NewIcon(icon)
	tile := updateRounded(container.NewCenter(image), color.NRGBA{R: tint.R, G: tint.G, B: tint.B, A: 28}, 10)
	label := widget.NewLabel(value)
	label.TextStyle.Bold = true
	label.Wrapping = fyne.TextWrapWord
	heading := updateText(caption, 12, false, updateMuted)
	heading.Alignment = fyne.TextAlignLeading
	return container.NewBorder(nil, nil, container.NewGridWrap(fyne.NewSize(34, 34), tile), nil, container.NewVBox(heading, label))
}

func updateCard(current, version string, size int64, progress fyne.CanvasObject, actions fyne.CanvasObject) fyne.CanvasObject {
	versions := container.NewHBox(updateVersion(current, false), container.NewCenter(updateText("→", 16, false, updateMuted)), updateVersion(version, true))
	info := updateRounded(container.NewPadded(container.NewVBox(
		updateInfo(theme.DownloadIcon(), "更新内容", "下载完整应用包（含配套驱动）", color.NRGBA{R: 74, G: 140, B: 255, A: 255}),
		widget.NewSeparator(),
		updateInfo(theme.StorageIcon(), "安装包大小", fmt.Sprintf("%.1f MiB", float64(size)/(1<<20)), color.NRGBA{R: 245, G: 158, B: 11, A: 255}),
	)), color.NRGBA{R: 248, G: 250, B: 252, A: 255}, 18)
	body := container.NewVBox(
		updateText("发现新版本", 24, true, updateInk), updateText("官方签名清单已验证，可安全下载", 13, false, updateMuted),
		updateSpace(8), container.NewCenter(versions), updateSpace(12), info, updateSpace(8))
	if progress != nil {
		body.Add(progress)
		body.Add(updateSpace(8))
	}
	body.Add(actions)
	body.Add(updateSpace(8))
	body.Add(updateText("通过官方通道下载，安装前校验应用包", 11, false, updateMuted))
	return updateRounded(container.New(layout.NewCustomPaddedLayout(12, 12, 20, 20), body), color.NRGBA{R: 255, G: 255, B: 255, A: 255}, 28)
}

func (w *Window) showUpdateConfirm(current, version string, size int64, accept func()) {
	content := container.NewStack()
	modal := widget.NewModalPopUp(content, w.Window.Canvas())
	cancel := widget.NewButtonWithIcon("暂不更新", theme.CancelIcon(), modal.Hide)
	confirm := widget.NewButtonWithIcon("立即更新", theme.DownloadIcon(), func() { modal.Hide(); accept() })
	confirm.Importance = widget.HighImportance
	content.Add(updateCard(current, version, size, nil, container.NewGridWithColumns(2, cancel, confirm)))
	modal.Resize(fyne.NewSize(440, content.MinSize().Height))
	modal.Show()
}
