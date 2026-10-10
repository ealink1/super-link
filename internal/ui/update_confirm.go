package ui

import (
	"fmt"
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
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
	pill := updateRounded(container.NewPadded(label), fill, 10)
	if !fresh {
		return pill
	}
	badge := updateRounded(container.NewPadded(updateText("NEW", 9, true, color.White)), color.NRGBA{R: 255, G: 74, B: 110, A: 255}, 9)
	return container.NewVBox(container.NewCenter(badge), pill)
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
	icon := canvas.NewImageFromResource(fyne.NewStaticResource("update-download.svg", []byte(`<svg xmlns="http://www.w3.org/2000/svg" width="72" height="72" viewBox="0 0 72 72"><defs><linearGradient id="g" x2="1" y2="1"><stop stop-color="#5cc8ff"/><stop offset=".5" stop-color="#4a8cff"/><stop offset="1" stop-color="#6b5cff"/></linearGradient></defs><rect width="72" height="72" rx="20" fill="url(#g)"/><g fill="none" stroke="white" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M36 18v25m-10-10 10 10 10-10"/><rect x="23" y="49" width="26" height="7" rx="3"/></g></svg>`)))
	icon.FillMode = canvas.ImageFillContain
	versions := container.NewHBox(updateVersion(current, false), container.NewCenter(updateText("→", 16, false, updateMuted)), updateVersion(version, true))
	info := updateRounded(container.NewPadded(container.NewVBox(
		updateInfo(theme.DownloadIcon(), "更新内容", "下载完整应用包（含配套驱动）", color.NRGBA{R: 74, G: 140, B: 255, A: 255}),
		widget.NewSeparator(),
		updateInfo(theme.StorageIcon(), "安装包大小", fmt.Sprintf("%.1f MiB", float64(size)/(1<<20)), color.NRGBA{R: 245, G: 158, B: 11, A: 255}),
	)), color.NRGBA{R: 248, G: 250, B: 252, A: 255}, 18)
	body := container.NewVBox(updateSpace(16), container.NewCenter(container.NewGridWrap(fyne.NewSize(72, 72), icon)), updateSpace(8),
		updateText("发现新版本", 24, true, updateInk), updateText("官方签名清单已验证，可安全下载", 13, false, updateMuted),
		updateSpace(8), container.NewCenter(versions), updateSpace(12), info, updateSpace(8))
	if progress != nil {
		body.Add(progress)
		body.Add(updateSpace(8))
	}
	body.Add(actions)
	body.Add(updateSpace(8))
	body.Add(updateText("通过官方通道下载，安装前校验应用包", 11, false, updateMuted))
	body.Add(updateSpace(12))
	stripe := canvas.NewHorizontalGradient(color.NRGBA{R: 92, G: 200, B: 255, A: 255}, color.NRGBA{R: 107, G: 92, B: 255, A: 255})
	stripe.SetMinSize(fyne.NewSize(380, 4))
	card := updateRounded(container.NewBorder(stripe, nil, nil, nil, container.NewPadded(body)), color.NRGBA{R: 255, G: 255, B: 255, A: 255}, 28)
	return container.NewStack(canvas.NewLinearGradient(color.NRGBA{R: 240, G: 247, B: 255, A: 255}, color.NRGBA{R: 245, G: 240, B: 255, A: 255}, 135), container.NewPadded(card))
}

func (w *Window) showUpdateConfirm(current, version string, size int64, accept func()) {
	content := container.NewStack()
	modal := dialog.NewCustomWithoutButtons("", content, w.Window)
	cancel := widget.NewButtonWithIcon("暂不更新", theme.CancelIcon(), modal.Hide)
	confirm := widget.NewButtonWithIcon("立即更新", theme.DownloadIcon(), func() { modal.Hide(); accept() })
	confirm.Importance = widget.HighImportance
	content.Add(updateCard(current, version, size, nil, container.NewGridWithColumns(2, cancel, confirm)))
	modal.Resize(fyne.NewSize(440, 520))
	modal.Show()
}
