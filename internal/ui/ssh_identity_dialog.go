package ui

import (
	"fmt"
	"image/color"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

var (
	sshIdentityFill      = shellTone{day: color.NRGBA{R: 255, G: 255, B: 255, A: 255}, night: color.NRGBA{R: 20, G: 26, B: 36, A: 255}}
	sshIdentityPanel     = shellTone{day: color.NRGBA{R: 246, G: 248, B: 252, A: 255}, night: color.NRGBA{R: 12, G: 17, B: 25, A: 255}}
	sshIdentityBlue      = shellTone{day: color.NRGBA{R: 37, G: 99, B: 235, A: 255}, night: color.NRGBA{R: 142, G: 197, B: 255, A: 255}}
	sshIdentityAmber     = shellTone{day: color.NRGBA{R: 180, G: 83, B: 9, A: 255}, night: color.NRGBA{R: 252, G: 211, B: 77, A: 255}}
	sshIdentityMint      = shellTone{day: color.NRGBA{R: 4, G: 120, B: 87, A: 255}, night: color.NRGBA{R: 110, G: 231, B: 183, A: 255}}
	sshIdentitySecondary = shellTone{day: color.NRGBA{R: 85, G: 97, B: 122, A: 255}, night: color.NRGBA{R: 154, G: 167, B: 184, A: 255}}
)

type sshIdentityDialog struct {
	parent                fyne.Window
	host                  domain.ShellHost
	fingerprint           string
	popup                 *widget.PopUp
	scope                 *container.ThemeOverride
	remember              *sshIdentityCheck
	confirm, cancel, copy *sshIdentityButton
	previousFocus         fyne.Focusable
	callback              func(bool, bool)
	closed                bool
}

type sshIdentityTheme struct{ shellModalTheme }

func (t sshIdentityTheme) Size(name fyne.ThemeSizeName) float32 {
	switch name {
	case theme.SizeNameInnerPadding:
		return 0
	case theme.SizeNamePopupRadius:
		return 18
	case theme.SizeNameInputRadius:
		return 10
	}
	return t.shellModalTheme.Size(name)
}
func (t sshIdentityTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameOverlayBackground {
		return resolveShellColor(sshIdentityFill)
	}
	return t.shellModalTheme.Color(name, variant)
}

func newSSHIdentityDialog(parent fyne.Window, host domain.ShellHost, fingerprint string, callback func(bool, bool)) *sshIdentityDialog {
	d := &sshIdentityDialog{parent: parent, host: host, fingerprint: fingerprint, callback: callback}
	d.confirm = d.button("是，确认并连接", "check", true, func() { d.finish(true) })
	d.cancel = d.button("不，取消连接", "x", false, func() { d.finish(false) })
	d.copy = d.button("复制指纹", "copy", false, func() {
		d.parent.Clipboard().SetContent(d.fingerprint)
		d.copy.SetText("已复制")
	})
	d.remember = &sshIdentityCheck{dialog: d}
	d.remember.Text, d.remember.Checked = "信任此主机并记住", true
	d.remember.ExtendBaseWidget(d.remember)
	body := shellVBox(d.target(), sshIdentityGap(16), d.fingerprintCard(), sshIdentityGap(16), d.warning(), sshIdentityGap(14), d.remember,
		sshIdentityGap(4), d.label("将指纹保存在 SuperLink 本地主机记录中，后续连接不再提示。", 11, shellMutedColor, false), sshIdentityGap(14))
	scroll := container.NewVScroll(shellInset(body, 24))
	content := shellBorder(shellVBox(d.header(), shellLine()), shellVBox(shellLine(), d.footer()), nil, nil, scroll)
	palette := sshIdentityTheme{shellModalTheme{shellTheme: newShellTheme()}}
	d.popup = widget.NewModalPopUp(container.NewThemeOverride(content, palette), parent.Canvas())
	d.scope = container.NewThemeOverride(d.popup, palette)
	size := parent.Canvas().Size()
	d.popup.Resize(fyne.NewSize(min(560, max(0, size.Width-32)), min(672, max(0, size.Height-32))))
	return d
}

func (d *sshIdentityDialog) show() {
	d.previousFocus = d.parent.Canvas().Focused()
	d.popup.Show()
	d.parent.Canvas().Focus(d.cancel)
}
func (d *sshIdentityDialog) finish(accepted bool) {
	if d.closed {
		return
	}
	d.closed = true
	d.popup.Hide()
	d.parent.Canvas().Focus(d.previousFocus)
	if d.callback != nil {
		d.callback(accepted, d.remember.Checked)
	}
}
func (d *sshIdentityDialog) key(key *fyne.KeyEvent) bool {
	switch key.Name {
	case fyne.KeyEscape:
		d.finish(false)
		return true
	case fyne.KeyReturn, fyne.KeyEnter:
		d.finish(true)
		return true
	}
	return false
}
func (d *sshIdentityDialog) label(text string, size float32, shade color.Color, bold bool) fyne.CanvasObject {
	label := widget.NewLabel(text)
	label.Wrapping, label.TextStyle.Bold = fyne.TextWrapWord, bold
	return container.NewThemeOverride(label, sshIdentityTextTheme{shellLabelTheme{newShellTheme(), size}, shade})
}

// Use local text sizing without changing the application's shared theme.
type sshIdentityTextTheme struct {
	shellLabelTheme
	shade color.Color
}

func (t sshIdentityTextTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNameForeground {
		if role, ok := t.shade.(shellColor); ok {
			return t.shellTheme.Color(fyne.ThemeColorName(role), variant)
		}
		return resolveShellColor(t.shade)
	}
	return t.shellLabelTheme.Color(name, variant)
}

func (d *sshIdentityDialog) header() fyne.CanvasObject {
	title := shellBorder(nil, nil, nil, sshIdentityChip("首次连接", sshIdentityAmber), d.label("核实 SSH 主机指纹", 16.5, shellTextColor, true))
	text := shellVBox(title, sshIdentityGap(6), d.label("为防止中间人攻击，请确认远程主机的身份指纹无误后再继续连接。", 12.5, shellMutedColor, false))
	badge := shellFixed(shellPanel(container.NewCenter(shellImage("shield", true, 22)), sshIdentityTint(sshIdentityBlue, 28), 13, 0), 44, 44)
	close := d.button("", "x", false, func() { d.finish(false) })
	return shellInset(shellBorder(nil, nil, shellHBox(badge, shellFixed(layout.NewSpacer(), 14, 0)), shellHBox(shellFixed(layout.NewSpacer(), 8, 0), close), text), 24)
}
func (d *sshIdentityDialog) target() fyne.CanvasObject {
	address := widget.NewRichText(
		sshIdentitySegment(d.host.User, theme.ColorNamePrimary, false),
		sshIdentitySegment("@", theme.ColorNamePlaceHolder, false),
		sshIdentitySegment(d.host.Host, theme.ColorNameForeground, true),
		sshIdentitySegment(fmt.Sprintf(":%d", d.host.Port), theme.ColorNamePlaceHolder, false),
	)
	address.Wrapping = fyne.TextWrapBreak
	description := d.label(fmt.Sprintf("连接目标 · 端口 %d", d.host.Port), 11, shellMutedColor, false)
	info := shellVBox(container.NewThemeOverride(address, sshIdentityMonoTheme{shellLabelTheme{newShellTheme(), 13.5}}), sshIdentityGap(3), description)
	badge := shellFixed(shellPanel(container.NewCenter(shellImage("server", false, 16)), sshIdentityFill, 9, 0), 32, 32)
	row := shellBorder(nil, nil, shellHBox(badge, shellFixed(layout.NewSpacer(), 12, 0)), shellHBox(shellFixed(layout.NewSpacer(), 10, 0), sshIdentityChip("SSH2", sshIdentityMint)), info)
	return shellPanel(row, sshIdentityPanel, 12, 14)
}
func (d *sshIdentityDialog) fingerprintCard() fyne.CanvasObject {
	raw := strings.TrimPrefix(d.fingerprint, "SHA256:")
	groups := make([]string, 0, (len(raw)+3)/4)
	for len(raw) > 0 {
		n := min(4, len(raw))
		groups = append(groups, raw[:n])
		raw = raw[n:]
	}
	segments := []widget.RichTextSegment{sshIdentitySegment("SHA256: ", theme.ColorNamePlaceHolder, false)}
	for i, group := range groups {
		bold := i == 0 || i == len(groups)-1
		shade := theme.ColorNamePlaceHolder
		if bold {
			shade = theme.ColorNameForeground
		}
		segments = append(segments, sshIdentitySegment(group+"  ", shade, bold))
	}
	label := widget.NewRichText(segments...)
	label.Wrapping = fyne.TextWrapWord
	meta := shellBorder(nil, nil, nil, d.actionView(d.copy), d.label("已分块显示，便于逐段人工比对", 11, shellMutedColor, false))
	box := shellPanel(shellVBox(container.NewThemeOverride(label, sshIdentityMonoTheme{shellLabelTheme{newShellTheme(), 13}}), sshIdentityGap(12), shellLine(), sshIdentityGap(10), meta), sshIdentityPanel, 12, 14)
	heading := shellBorder(nil, nil, shellText("主机密钥指纹", 11.5, true, shellMutedColor), sshIdentityChip("SHA256", sshIdentityBlue), layout.NewSpacer())
	return shellVBox(heading, sshIdentityGap(9), box)
}

func sshIdentitySegment(text string, shade fyne.ThemeColorName, bold bool) *widget.TextSegment {
	return &widget.TextSegment{Text: text, Style: widget.RichTextStyle{Inline: true, ColorName: shade, TextStyle: fyne.TextStyle{Monospace: true, Bold: bold}}}
}

type sshIdentityMonoTheme struct{ shellLabelTheme }

func (t sshIdentityMonoTheme) Color(name fyne.ThemeColorName, variant fyne.ThemeVariant) color.Color {
	if name == theme.ColorNamePlaceHolder {
		return resolveShellColor(sshIdentitySecondary)
	}
	return t.shellLabelTheme.Color(name, variant)
}
func (d *sshIdentityDialog) warning() fyne.CanvasObject {
	text := d.label("请通过可信渠道核实后再确认，例如与云服务商控制台比对，或核查目标主机的 Ed25519 密钥：", 12.2, sshIdentityAmber, false)
	command := widget.NewLabel("ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub")
	command.Wrapping, command.TextStyle.Monospace = fyne.TextWrapBreak, true
	body := shellVBox(text, sshIdentityGap(6), container.NewThemeOverride(command, sshIdentityTextTheme{shellLabelTheme{newShellTheme(), 11.5}, sshIdentityAmber}))
	return shellPanel(shellBorder(nil, nil, shellHBox(shellImage("shield", false, 16), shellFixed(layout.NewSpacer(), 10, 0)), nil, body), sshIdentityTint(sshIdentityAmber, 15), 11, 14)
}
func (d *sshIdentityDialog) footer() fyne.CanvasObject {
	hint := d.label("Esc 取消 · Enter 确认", 11, shellMutedColor, false)
	actions := shellHBox(d.actionView(d.cancel), shellFixed(layout.NewSpacer(), 10, 0), d.actionView(d.confirm))
	return shellInset(shellVBox(hint, sshIdentityGap(10), shellHBox(layout.NewSpacer(), actions)), 20)
}
func sshIdentityGap(height float32) fyne.CanvasObject {
	return shellFixed(layout.NewSpacer(), 0, height)
}
func sshIdentityTint(t shellTone, alpha uint8) shellTone { t.day.A, t.night.A = alpha, alpha; return t }
func sshIdentityChip(text string, shade shellTone) fyne.CanvasObject {
	panel := shellPanel(shellText(text, 10.5, true, shade), sshIdentityTint(shade, 24), 6, 5)
	panel.Objects[0].(*shellPrimitive).stroke = sshIdentityTint(shade, 55)
	return panel
}

type sshIdentityButton struct {
	shellAlignedButton
	dialog *sshIdentityDialog
}

func (d *sshIdentityDialog) actionView(button *sshIdentityButton) fyne.CanvasObject {
	view := container.NewThemeOverride(button, sshIdentityActionTheme{shellButtonTheme{shellTheme: newShellTheme(), bold: true}})
	if button.Importance == widget.HighImportance {
		return view
	}
	return shellPanel(view, color.Transparent, 10, 0)
}

type sshIdentityActionTheme struct{ shellButtonTheme }

func (t sshIdentityActionTheme) Size(name fyne.ThemeSizeName) float32 {
	if name == theme.SizeNameText {
		return 13
	}
	if name == theme.SizeNameInnerPadding {
		return 10
	}
	if name == theme.SizeNameInputRadius {
		return 10
	}
	return t.shellButtonTheme.Size(name)
}
func (d *sshIdentityDialog) button(text, name string, primary bool, run func()) *sshIdentityButton {
	b := &sshIdentityButton{dialog: d}
	b.Text, b.OnTapped, b.bold = text, run, true
	b.ExtendBaseWidget(b)
	b.Importance = widget.LowImportance
	if primary {
		b.Importance = widget.HighImportance
	}
	if name != "" {
		resource := shellIcon(name, false)
		if outline, ok := resource.(*shellOutlineIcon); ok && primary {
			outline.shade = theme.ColorNameForegroundOnPrimary
		}
		b.SetIcon(resource)
	}
	return b
}
func (b *sshIdentityButton) TypedKey(key *fyne.KeyEvent) {
	if !b.dialog.key(key) {
		b.Button.TypedKey(key)
	}
}

type sshIdentityCheck struct {
	widget.Check
	dialog *sshIdentityDialog
}

func (c *sshIdentityCheck) TypedKey(key *fyne.KeyEvent) {
	if !c.dialog.key(key) {
		c.Check.TypedKey(key)
	}
}
