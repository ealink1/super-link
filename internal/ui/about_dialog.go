package ui

import (
	"image/color"
	"os"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

const aboutCardWidth = 400

// The About card sits on the dialog's translucent overlay surface, so its
// fields are translucent too and must stay readable in both appearances.
var (
	aboutVersionInk  = shellTone{day: color.NRGBA{R: 12, G: 82, B: 200, A: 255}, night: color.NRGBA{R: 137, G: 176, B: 255, A: 255}}
	aboutVersionFill = shellTone{day: color.NRGBA{R: 10, G: 108, B: 255, A: 26}, night: color.NRGBA{R: 59, G: 130, B: 246, A: 44}}
	aboutChipFill    = shellTone{day: color.NRGBA{R: 15, G: 23, B: 42, A: 14}, night: color.NRGBA{R: 255, G: 255, B: 255, A: 18}}
	aboutFieldFill   = shellTone{day: color.NRGBA{R: 15, G: 23, B: 42, A: 10}, night: color.NRGBA{R: 255, G: 255, B: 255, A: 14}}
)

// The mark reuses the bundled link geometry on the gradient tile; a scaled
// group transform does not survive Fyne's SVG parser, so the tile shares the
// 24 unit grid and the image widget scales the whole drawing.
const aboutLogoSVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">` +
	`<defs><linearGradient id="aboutLogo" x1="0" y1="0" x2="1" y2="1">` +
	`<stop stop-color="#5f97ff"/><stop offset=".58" stop-color="#0a5fe0"/><stop offset="1" stop-color="#8a5bff"/>` +
	`</linearGradient></defs>` +
	`<rect width="24" height="24" rx="6.5" fill="url(#aboutLogo)"/>` +
	`<g fill="none" stroke="#ffffff" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">` +
	`<path d="M9.8 14.2a3.6 3.6 0 0 0 5.1 0l3.2-3.2a3.6 3.6 0 0 0-5.1-5.1l-1 1"/>` +
	`<path d="M14.2 9.8a3.6 3.6 0 0 0-5.1 0L5.9 13a3.6 3.6 0 0 0 5.1 5.1l1-1"/>` +
	`</g></svg>`

const (
	aboutLockGlyph   = `<rect x="4.5" y="10.5" width="15" height="9.5" rx="2"/><path d="M8 10.5V8a4 4 0 0 1 8 0v2.5"/>`
	aboutKeyGlyph    = `<circle cx="9" cy="12" r="3.2"/><path d="M12.2 12H21M18.4 12v3.2"/>`
	aboutFolderGlyph = `<path d="M3 7.5A1.5 1.5 0 0 1 4.5 6h4l2 2.5h9A1.5 1.5 0 0 1 21 10v7.5A1.5 1.5 0 0 1 19.5 19h-15A1.5 1.5 0 0 1 3 17.5z"/>`
)

func aboutSpace(height float32) fyne.CanvasObject {
	return shellFixed(layout.NewSpacer(), 0, height)
}

func aboutGap(width float32) fyne.CanvasObject {
	return shellFixed(layout.NewSpacer(), width, 0)
}

func aboutChip(text string, ink, fill color.Color) fyne.CanvasObject {
	label := shellText(text, 11, true, ink)
	return shellFixed(container.NewStack(shellRectangle(fill, 10, nil), container.NewCenter(label)), label.MinSize().Width+18, 20)
}

// Inline geometry avoids growing the licensed asset manifests for one dialog.
func aboutGlyph(name, body string) fyne.Resource {
	source := []byte(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">` + body + `</svg>`)
	return &shellOutlineIcon{name: "about-" + name, source: source, shade: theme.ColorNamePlaceHolder}
}

var (
	aboutLockMark   = aboutGlyph("lock", aboutLockGlyph)
	aboutKeyMark    = aboutGlyph("key", aboutKeyGlyph)
	aboutFolderMark = aboutGlyph("folder", aboutFolderGlyph)
)

func aboutLogo() fyne.CanvasObject {
	image := canvas.NewImageFromResource(fyne.NewStaticResource("about-logo.svg", []byte(aboutLogoSVG)))
	image.FillMode = canvas.ImageFillContain
	return container.NewGridWrap(fyne.NewSquareSize(74), image)
}

func aboutDisplayPath(root string) string {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		return root
	}
	if rest, ok := strings.CutPrefix(filepath.ToSlash(root), filepath.ToSlash(home)+"/"); ok {
		return "~/" + rest
	}
	return root
}

func (w *Window) aboutFacts() fyne.CanvasObject {
	label := widget.NewLabel(aboutDisplayPath(w.Root))
	label.TextStyle.Monospace = true
	label.Truncation = fyne.TextTruncateEllipsis
	root := w.Root
	copyPath := widget.NewButtonWithIcon("", shellIcon("copy", false), func() { fyne.CurrentApp().Clipboard().SetContent(root) })
	copyPath.Importance = widget.LowImportance
	path := container.NewBorder(nil, nil, aboutGlyphLead(aboutFolderMark), copyPath, shellLabel(label, 11.5))
	return shellPanel(container.NewVBox(
		aboutTip(aboutLockMark, "数据库权限 600", "仅当前用户可读写"),
		aboutSpace(9),
		aboutTip(aboutKeyMark, "记住的密码加密保存", "凭据不明文写入磁盘"),
		aboutSpace(9),
		path,
	), aboutFieldFill, 11, 13)
}

func aboutTip(mark fyne.Resource, strong, rest string) fyne.CanvasObject {
	text := container.NewHBox(shellText(strong, 12.5, true, shellTextColor), aboutGap(6), shellText(rest, 12.5, false, shellMutedColor))
	return container.NewBorder(nil, nil, aboutGlyphLead(mark), nil, text)
}

func aboutGlyphLead(mark fyne.Resource) fyne.CanvasObject {
	return shellFixed(container.NewCenter(widget.NewIcon(mark)), 22, 18)
}

func (w *Window) about() {
	content := container.NewStack()
	modal := dialog.NewCustomWithoutButtons("", content, w.Window)
	updates := shellButton("检查更新", "refresh-cw", false, func() {
		modal.Hide()
		w.checkUpdates()
	})
	dismiss := shellButton("好", "", true, modal.Hide)
	content.Add(container.NewVBox(
		container.NewCenter(aboutLogo()),
		aboutSpace(11),
		container.NewCenter(shellText("SuperLink", 22, true, shellTextColor)),
		aboutSpace(8),
		container.NewCenter(container.NewHBox(
			aboutChip("版本 "+w.Version, aboutVersionInk, aboutVersionFill),
			aboutGap(8),
			aboutChip("Go · Fyne", shellMutedColor, aboutChipFill),
		)),
		aboutSpace(12),
		container.NewCenter(shellText("连接、草稿与密码全部保存在本机。", 12.5, false, shellMutedColor)),
		aboutSpace(14),
		w.aboutFacts(),
		aboutSpace(10),
		container.NewCenter(shellText("数据库权限是最终保护边界，草稿可能包含业务数据。", 11, false, shellMutedColor)),
		aboutSpace(12),
		shellLine(),
		aboutSpace(10),
		shellFixed(container.NewHBox(updates, layout.NewSpacer(), dismiss), 0, 28),
		aboutSpace(4),
	))
	modal.Resize(fyne.NewSize(aboutCardWidth, 1))
	modal.Show()
}
