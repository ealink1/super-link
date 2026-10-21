package ui

import (
	"fmt"
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
)

func updateVersionChip(value string, fresh bool) fyne.CanvasObject {
	ink := color.Color(cardInk)
	fill := color.Color(cardDivider)
	if fresh {
		ink = color.White
		fill = cardAccent
	}
	label := cardText(value, 14, true, ink)
	label.TextStyle.Monospace = true
	return cardRounded(container.NewPadded(label), fill, 10)
}

func updateVersions(current, version string) fyne.CanvasObject {
	return container.NewHBox(
		updateVersionChip(current, false),
		container.NewCenter(cardText("→", 16, false, cardMuted)),
		updateVersionChip(version, true),
	)
}

func updateCardSpec(current, version string, size int64) cardSpec {
	return cardSpec{
		Title:    "发现新版本",
		Subtitle: "官方签名清单已验证，可安全下载",
		Extra:    updateVersions(current, version),
		Info: []cardInfoRow{
			{Icon: theme.DownloadIcon(), Tint: cardAccent, Label: "更新内容", Value: "下载完整应用包（含配套驱动）"},
			{Icon: theme.StorageIcon(), Tint: color.NRGBA{R: 245, G: 158, B: 11, A: 255}, Label: "安装包大小", Value: fmt.Sprintf("%.1f MiB", float64(size)/(1<<20))},
		},
		Width:  cardDefaultWidth,
		Footer: "安装前校验应用包",
	}
}

func (w *Window) showUpdateConfirm(current, version string, size int64, accept func()) {
	spec := updateCardSpec(current, version, size)
	spec.Actions = []cardAction{
		{Label: "暂不更新", Icon: theme.CancelIcon()},
		{Label: "立即更新", Icon: theme.DownloadIcon(), Primary: true, Accept: true, OnTapped: accept},
	}
	newCardDialog(spec, w.Window, nil).Show()
}
