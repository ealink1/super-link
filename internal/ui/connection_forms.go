package ui

import (
	"context"
	"fmt"
	"strconv"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/infra/secrets"
)

func (e *connectionEditor) show() {
	e.initFields()
	c := e.original.Config
	raw, err := secrets.PublicJSON(c)
	if err != nil {
		e.owner.showError(err)
		return
	}
	e.extra = widget.NewMultiLineEntry()
	e.extra.SetText(string(raw))
	e.extra.SetMinRowsVisible(10)
	e.timeout = e.entry(strconv.Itoa(c.Timeout), false)
	e.queryTimeout = e.entry(strconv.Itoa(c.QueryTimeout), false)
	e.networkForm(c) // Initialize credential fields before constructing the network panels.
	advanced := e.advancedForm()
	appearance := e.appearanceForm()
	tabs := container.NewAppTabs(container.NewTabItem("基本", container.NewVScroll(e.basicForm())), container.NewTabItem("网络与安全", container.NewVScroll(e.networkPanels())), container.NewTabItem("外观", appearance), container.NewTabItem("高级", advanced))
	d, _ := domain.Resolve(c.Type)
	closeButton := widget.NewButtonWithIcon("", theme.CancelIcon(), e.close)
	closeButton.Importance = widget.LowImportance
	header := container.NewHBox(newDatabaseBadge("db-"+d.Key), widget.NewLabelWithStyle(d.Name+" 连接", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), layout.NewSpacer(), widget.NewLabel("1 类型  ›  2 参数  ›  3 保存"), closeButton)
	testButton := widget.NewButton("测试连接", nil)
	testButton.OnTapped = func() {
		profile, err := e.collect()
		if err != nil {
			e.hint.SetText(err.Error())
			return
		}
		testButton.Disable()
		e.hint.SetText("正在测试连接…")
		e.owner.jobs.runWithCancel(&e.testCancel, func(ctx context.Context) (any, error) { return nil, e.owner.Engine.Test(ctx, profile) }, func(_ any, err error) {
			if e.closed {
				return
			}
			testButton.Enable()
			if err != nil {
				e.hint.SetText("测试失败：" + err.Error())
			} else {
				e.hint.SetText("连接测试通过")
			}
		})
	}
	saveButton := widget.NewButton("保存", e.save)
	saveButton.Importance = widget.HighImportance
	buttons := container.NewHBox(testButton, widget.NewButton("取消", e.close), saveButton)
	footer := container.NewVBox(e.hint, container.NewBorder(nil, nil, action("上一步", "undo", func() { e.close(); e.owner.typePicker() }), buttons, nil))
	content := container.NewPadded(container.NewBorder(header, footer, nil, nil, tabs))
	e.modal = widget.NewModalPopUp(content, e.owner.Window.Canvas())
	size := e.owner.Window.Canvas().Size()
	e.modal.Resize(fyne.NewSize(min(920, size.Width-32), min(640, size.Height-48)))
	e.modal.Show()
}
func (e *connectionEditor) close() {
	e.closed = true
	if e.testCancel != nil {
		e.testCancel()
	}
	if e.modal != nil {
		e.modal.Hide()
	}
}
func (e *connectionEditor) basicForm() fyne.CanvasObject {
	d, _ := domain.Resolve(e.original.Config.Type)
	e.name.SetPlaceHolder("例如：本地测试库")
	e.database.SetPlaceHolder("数据库")
	e.password.SetPlaceHolder("密码")
	uri := container.NewVBox(widget.NewLabelWithStyle("连接串", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), e.uri, container.NewHBox(action("解析并填充", "sliders", e.parseURI), action("从字段生成", "link", e.generateURI), action("复制", "copy", func() { e.owner.Window.Clipboard().SetContent(e.uri.Text) })), widget.NewSeparator())
	e.allow = e.entry(joinPatterns(e.original.DatabaseAllow), false)
	e.allow.SetPlaceHolder("可选：精确库名，逗号分隔；空表示不限制")
	e.include = e.entry(joinPatterns(e.original.DatabaseInclude), false)
	e.include.SetPlaceHolder("例如 tenant_%, reporting*")
	e.exclude = e.entry(joinPatterns(e.original.DatabaseExclude), false)
	e.exclude.SetPlaceHolder("例如 *_archive, test_%")
	e.topology = widget.NewSelect([]string{"single", "replica", "cluster", "sentinel"}, nil)
	e.topology.SetSelected(e.original.Config.Topology)
	if e.topology.Selected == "" {
		e.topology.SetSelected("single")
	}
	e.hosts = e.entry(joinPatterns(e.original.Config.Hosts), false)
	e.hosts.SetPlaceHolder("多个 host:port，逗号分隔")
	e.connectAfterSave = widget.NewCheck("保存后展开", nil)
	e.connectAfterSave.SetChecked(e.original.ID == "")
	address := fyne.CanvasObject(container.NewHBox(container.NewGridWrap(fyne.NewSize(260, 32), e.host), container.NewGridWrap(fyne.NewSize(90, 32), e.port), container.NewGridWrap(fyne.NewSize(150, 32), e.database)))
	if d.Key == "sqlite" || d.Key == "duckdb" {
		browse := action("选择文件", "folder-open", func() {
			dialog.ShowFileOpen(func(reader fyne.URIReadCloser, err error) {
				if err != nil {
					e.owner.showError(err)
					return
				}
				if reader != nil {
					e.host.SetText(reader.URI().Path())
					_ = reader.Close()
				}
			}, e.owner.Window)
		})
		address = container.NewBorder(nil, nil, nil, browse, e.host)
		uri.Hide()
	}
	authentication := container.NewHBox(container.NewGridWrap(fyne.NewSize(160, 32), e.user), container.NewGridWrap(fyne.NewSize(240, 32), e.password), e.persist, e.connectAfterSave)
	items := []*widget.FormItem{widget.NewFormItem("名称", container.NewHBox(container.NewGridWrap(fyne.NewSize(310, 32), e.name), e.environment)), widget.NewFormItem("主机", address), widget.NewFormItem("认证", authentication), widget.NewFormItem("分组", e.group), widget.NewFormItem("数据库显示", e.allow), widget.NewFormItem("通配包含", e.include), widget.NewFormItem("通配排除", e.exclude)}
	if d.Key == "custom" {
		items = append(items, widget.NewFormItem("Driver", e.driver), widget.NewFormItem("DSN", e.dsn))
	}
	form := widget.NewForm(items...)
	e.protectEdit = widget.NewCheck("限制数据编辑", nil)
	e.protectEdit.SetChecked(e.original.Config.Protection.RestrictDataEdit)
	e.protectStructure = widget.NewCheck("限制结构编辑", nil)
	e.protectStructure.SetChecked(e.original.Config.Protection.RestrictStructureEdit)
	e.protectScript = widget.NewCheck("限制写入脚本执行", nil)
	e.protectScript.SetChecked(e.original.Config.Protection.RestrictScriptExecution)
	e.protectImport = widget.NewCheck("限制数据导入", nil)
	e.protectImport.SetChecked(e.original.Config.Protection.RestrictDataImport)
	protection := widget.NewAccordion(widget.NewAccordionItem("生产连接保护", container.NewVBox(e.readonly, e.protectEdit, e.protectStructure, e.protectScript, e.protectImport)))
	mode := widget.NewForm(widget.NewFormItem("模式", e.topology), widget.NewFormItem("节点地址", e.hosts))
	if d.Key == "sqlite" || d.Key == "duckdb" {
		mode.Hide()
	}
	rememberHint := widget.NewLabel("勾选后加密保存在本机，重启自动读取；取消后仅本次运行有效。")
	rememberHint.Wrapping = fyne.TextWrapWord
	return container.NewPadded(container.NewVBox(uri, form, rememberHint, mode, widget.NewSeparator(), protection))
}
func (e *connectionEditor) networkPanels() fyne.CanvasObject {
	panels := container.NewStack()
	selectPanel := func(name string) {
		var form fyne.CanvasObject
		switch name {
		case "SSL/TLS":
			form = widget.NewForm(widget.NewFormItem("验证模式", e.tlsMode), widget.NewFormItem("CA 证书", e.ca), widget.NewFormItem("客户端证书", e.cert), widget.NewFormItem("客户端私钥", e.key))
		case "SSH 隧道":
			form = widget.NewForm(widget.NewFormItem("SSH 主机", e.sshHost), widget.NewFormItem("端口", e.sshPort), widget.NewFormItem("用户名", e.sshUser), widget.NewFormItem("密码", e.sshPassword), widget.NewFormItem("私钥", e.sshKey), widget.NewFormItem("known_hosts", e.knownHosts), widget.NewFormItem("主机指纹", e.fingerprint))
		default:
			form = widget.NewForm(widget.NewFormItem("代理类型", e.proxyType), widget.NewFormItem("主机", e.proxyHost), widget.NewFormItem("端口", e.proxyPort), widget.NewFormItem("用户名", e.proxyUser), widget.NewFormItem("密码", e.proxyPassword))
		}
		panels.Objects = []fyne.CanvasObject{container.NewVBox(widget.NewLabelWithStyle(name, fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), form)}
		panels.Refresh()
	}
	row := func(check *widget.Check, name, description string) fyne.CanvasObject {
		return container.NewBorder(nil, nil, check, action("编辑", "sliders", func() { selectPanel(name) }), container.NewVBox(widget.NewLabelWithStyle(name, fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), widget.NewLabel(description)))
	}
	e.tlsEnabled.SetText("")
	e.sshEnabled.SetText("")
	e.proxyEnabled.SetText("")
	selectPanel("SSL/TLS")
	return container.NewPadded(container.NewVBox(widget.NewLabel("勾选启用 · 点击编辑详情"), row(e.tlsEnabled, "SSL/TLS", "加密与证书校验"), widget.NewSeparator(), row(e.sshEnabled, "SSH 隧道", "跳板机 / 堡垒机转发"), widget.NewSeparator(), row(e.proxyEnabled, "代理", "本地代理或网关转发"), widget.NewSeparator(), panels, widget.NewForm(widget.NewFormItem("连接超时（秒）", e.timeout), widget.NewFormItem("查询超时（秒，0 关闭）", e.queryTimeout))))
}
func parseTimeout(text string, maxValue int) (int, error) {
	value, err := strconv.Atoi(text)
	if err != nil || value < 0 || value > maxValue {
		return 0, fmt.Errorf("超时必须是 0–%d 的整数", maxValue)
	}
	return value, nil
}
