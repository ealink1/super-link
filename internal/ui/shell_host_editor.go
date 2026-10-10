package ui

import (
	"context"
	"errors"
	"strconv"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/infra/secrets"
)

type shellHostEditor struct {
	workspace                                                                *shellWorkspace
	host                                                                     domain.ShellHost
	name, group, address, port, user, password, key, passphrase, notes, tags *shellFormEntry
	remember                                                                 *shellFormCheck
	system                                                                   *shellFormSelect
	keyMode                                                                  bool
	credentials                                                              *fyne.Container
	passwordMode, keyButton                                                  *shellAlignedButton
	hint                                                                     *widget.Label
	save                                                                     *shellAlignedButton
	modal                                                                    *shellModal
}

func (s *shellWorkspace) editHost(h domain.ShellHost) {
	e := newShellHostEditor(s, h)
	e.modal.popup.Show()
}

func newShellHostEditor(s *shellWorkspace, h domain.ShellHost) *shellHostEditor {
	entry := func(value, placeholder string) *shellFormEntry {
		e := newShellFormEntry(false)
		e.SetText(value)
		e.SetPlaceHolder(placeholder)
		return e
	}
	e := &shellHostEditor{workspace: s, host: h, keyMode: h.KeyPath != ""}
	e.name, e.group = entry(h.Name, "例如: Production DB"), entry(h.Group, "默认分组")
	e.address, e.port, e.user = entry(h.Host, "192.168.1.10"), entry(strconv.Itoa(h.Port), "22"), entry(h.User, "root")
	e.notes, e.tags, e.key = entry(h.Notes, "简短描述主机的用途..."), entry(h.Tags, "开发, 测试"), entry(h.KeyPath, "本机私钥的绝对路径")
	e.password, e.passphrase = newShellFormEntry(true), newShellFormEntry(true)
	e.password.SetText(h.Password)
	e.passphrase.SetText(h.Passphrase)
	e.remember = newShellFormCheck("记住密码和私钥口令")
	e.remember.SetChecked(h.Remember)
	e.system = newShellFormSelect([]string{"Linux", "macOS", "Windows", "其他"})
	e.system.SetSelected("Linux")
	if h.OperatingSystem != "" {
		e.system.SetSelected(h.OperatingSystem)
	}
	e.hint = widget.NewLabel("")
	e.hint.Wrapping = fyne.TextWrapWord
	e.credentials = container.NewStack()
	e.passwordMode = shellButton("密码认证", "", false, func() { e.keyMode = false; e.refreshAuthentication() })
	e.keyButton = shellButton("密钥认证", "", false, func() { e.keyMode = true; e.refreshAuthentication() })
	e.save = shellButton("保存主机", "", true, e.persist)
	endpoint := shellBorder(nil, nil, nil, shellHBox(shellFixed(layout.NewSpacer(), 16, 0), shellFixed(shellField("端口", e.port), 140, 0)), shellField("主机地址 / IP", e.address))
	rows := shellVBox(shellVBox(shellText("基础信息", 12, true, shellMutedColor), shellFixed(layout.NewSpacer(), 0, 12)), shellField("主机别名", e.name), shellFixed(layout.NewSpacer(), 0, 12), shellColumns(16, shellField("主机分组", e.group), shellField("操作系统", e.system)), shellFixed(layout.NewSpacer(), 0, 12), shellField("备注 (Description)", e.notes), shellFixed(layout.NewSpacer(), 0, 12), shellField("标签（逗号分隔）", e.tags), shellFixed(layout.NewSpacer(), 0, 20), shellLine(), shellSection("连接设置"), endpoint, shellFixed(layout.NewSpacer(), 0, 20), shellLine(), shellSection("认证方式"), shellColumns(16, shellOutlined(e.passwordMode), shellOutlined(e.keyButton)), shellFixed(layout.NewSpacer(), 0, 16), e.credentials, shellFixed(layout.NewSpacer(), 0, 12), e.remember, shellText("凭据加密保存在本机，重新启动后自动读取", 12, false, shellMutedColor))
	footer := shellBorder(nil, nil, e.hint, shellHBox(shellButtonView(shellButton("取消", "", false, func() { e.modal.hide() })), shellFixed(layout.NewSpacer(), 12, 0), shellButtonView(e.save)), layout.NewSpacer())
	title := "新建主机 · SSH"
	if h.ID != "" {
		title = "编辑主机 · SSH"
	}
	e.modal = s.newModal(title, rows, footer, func() { e.password.SetText(""); e.passphrase.SetText("") })
	e.refreshAuthentication()
	return e
}

func (e *shellHostEditor) refreshAuthentication() {
	e.passwordMode.Importance, e.keyButton.Importance = widget.LowImportance, widget.LowImportance
	if e.keyMode {
		e.keyButton.Importance = widget.HighImportance
		e.credentials.Objects = []fyne.CanvasObject{shellVBox(shellColumns(16, shellField("用户名", e.user), shellField("私钥路径", e.key)), shellFixed(layout.NewSpacer(), 0, 16), shellField("私钥口令", e.passphrase))}
	} else {
		e.passwordMode.Importance = widget.HighImportance
		e.credentials.Objects = []fyne.CanvasObject{shellColumns(16, shellField("用户名", e.user), shellField("密码", e.password))}
	}
	e.passwordMode.Refresh()
	e.keyButton.Refresh()
	e.credentials.Refresh()
	if e.modal != nil {
		e.modal.scope.Refresh()
	}
}

func (e *shellHostEditor) persist() {
	port, err := strconv.Atoi(e.port.Text)
	if err != nil {
		e.hint.SetText("端口必须是数字")
		return
	}
	next := e.host
	next.Name, next.Group, next.Host, next.Port, next.User = e.name.Text, e.group.Text, e.address.Text, port, e.user.Text
	next.Notes, next.Tags, next.OperatingSystem = e.notes.Text, e.tags.Text, e.system.Selected
	next.Password, next.Passphrase, next.KeyPath = "", "", ""
	if e.keyMode {
		next.KeyPath, next.Passphrase = e.key.Text, e.passphrase.Text
	} else {
		next.Password = e.password.Text
	}
	next.Remember = e.remember.Checked
	if err = next.Validate(); err != nil {
		e.hint.SetText(err.Error())
		return
	}
	e.save.Disable()
	e.hint.SetText("正在保存…")
	e.workspace.owner.jobs.run(func(ctx context.Context) (any, error) { return e.workspace.service.Save(ctx, next) }, func(_ any, err error) {
		e.save.Enable()
		if err != nil {
			e.hint.SetText(err.Error())
			return
		}
		e.modal.hide()
		e.workspace.reloadHosts()
	})
}

func (s *shellWorkspace) loadHostEditor(h domain.ShellHost) {
	s.owner.jobs.run(func(ctx context.Context) (any, error) { return s.service.Get(ctx, h.ID) }, func(value any, err error) {
		if err != nil && !errors.Is(err, secrets.ErrMissing) && !errors.Is(err, secrets.ErrLegacy) {
			s.owner.showError(err)
			return
		}
		s.editHost(value.(domain.ShellHost))
	})
}
