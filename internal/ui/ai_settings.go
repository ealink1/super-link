package ui

import (
	"context"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
	"github.com/ealink1/super-link/internal/infra/chat"
)

func (p *aiPanel) settings() {
	if p.settingsOpen || p.loading || p.saving || p.busy || p.owner.shuttingDown {
		return
	}
	p.settingsOpen = true
	base, model, key := widget.NewEntry(), widget.NewEntry(), widget.NewPasswordEntry()
	base.SetPlaceHolder("https://api.example.com/v1")
	model.SetPlaceHolder("填写服务支持的模型名称")
	key.SetPlaceHolder("本地服务可留空")
	base.SetText(p.config.BaseURL)
	model.SetText(p.config.Model)
	key.SetText(p.config.APIKey)
	stream := widget.NewCheck("流式回复（不支持时取消勾选）", nil)
	stream.SetChecked(p.config.Model == "" || p.config.Stream)
	feedback := widget.NewLabel("")
	feedback.Wrapping = fyne.TextWrapWord
	form := widget.NewForm(widget.NewFormItem("API 基础地址", base), widget.NewFormItem("模型", model), widget.NewFormItem("API Key", key))
	help := widget.NewLabel("使用 OpenAI 兼容接口。Ollama 示例：http://localhost:11434/v1\n配置加密保存在本机；保存不会发送测试请求。")
	help.Wrapping = fyne.TextWrapWord
	var modal *cardDialog
	save := widget.NewButton("保存", func() {
		config := chat.Config{BaseURL: strings.TrimSpace(base.Text), Model: strings.TrimSpace(model.Text), APIKey: strings.TrimSpace(key.Text), Stream: stream.Checked}
		if err := config.Validate(); err != nil {
			feedback.SetText(err.Error())
			return
		}
		p.persistConfig(config, feedback, modal)
	})
	save.Importance = widget.HighImportance
	buttons := container.NewHBox(widget.NewButton("取消", func() {
		if !p.saving {
			modal.Hide()
		}
	}), save)
	body := container.NewVBox(help, form, stream, feedback, buttons)
	modal = newCardCustomNoButtons("AI 设置", body, p.owner.Window)
	modal.Resize(fyne.NewSize(570, 340))
	p.settingsDialog = modal
	modal.SetOnClosed(func() { p.settingsOpen = false; p.settingsDialog = nil; key.SetText("") })
	modal.Show()
}

func (p *aiPanel) persistConfig(config chat.Config, feedback *widget.Label, modal *cardDialog) {
	if p.saving || p.owner.shuttingDown {
		return
	}
	p.saving = true
	p.refreshControls()
	feedback.SetText("正在加密保存…")
	p.owner.jobs.run(func(ctx context.Context) (any, error) { return p.service.Save(ctx, config) }, func(value any, err error) {
		p.saving = false
		p.refreshControls()
		if err != nil {
			feedback.SetText("保存失败：" + err.Error())
			return
		}
		saved := value.(application.AIConfigSave)
		changedProvider := p.config.BaseURL != saved.Config.BaseURL || p.config.Model != saved.Config.Model
		p.config = saved.Config
		if changedProvider {
			p.history = nil // Preserve visible text and unsent input when changing models.
		}
		p.showConfigurationStatus()
		if changedProvider {
			p.status.SetText("AI 配置已保存 · 新模型从新上下文开始")
		}
		if saved.CleanupError != nil {
			p.status.SetText("配置已保存；旧密文未能清理")
		}
		modal.Hide()
	})
}
