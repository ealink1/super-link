package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
)

func (p *aiPanel) build() {
	p.model = widget.NewLabel("正在读取配置…")
	p.model.Truncation = fyne.TextTruncateEllipsis
	p.newButton = shellButton("", "plus", false, p.newConversation)
	p.settingsButton = shellButton("", "settings", false, p.settings)
	closeButton := shellButton("", "x", false, p.owner.aiEntry)
	// Border centres its trailing object, which floated the actions between the
	// title and its subtitle; keep them on the title row instead.
	header := container.NewVBox(
		container.NewHBox(widget.NewLabelWithStyle("SuperLink AI", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), layout.NewSpacer(), p.newButton, p.settingsButton, closeButton),
		p.model,
	)
	p.messages = container.NewVBox()
	p.scroll = container.NewVScroll(shellInset(p.messages, 14))
	p.status = widget.NewLabel("正在读取 AI 配置…")
	p.status.Wrapping = fyne.TextWrapWord
	p.input = newAIInput(p.send)
	p.input.SetPlaceHolder("输入问题…  Ctrl / ⌘ + Enter 发送")
	p.input.SetMinRowsVisible(4)
	p.sendButton = shellButton("发送", "send", true, p.send)
	settings := shellButton("AI 设置", "settings", false, p.settings)
	footer := container.NewVBox(widget.NewSeparator(), p.status, p.input,
		container.NewHBox(settings, layout.NewSpacer(), p.sendButton))
	body := container.NewBorder(container.NewVBox(header, widget.NewSeparator()), footer, nil, nil, p.scroll)
	p.content = container.NewThemeOverride(container.NewBorder(nil, nil, shellLine(), nil, shellInset(body, 12)), newShellTheme())
	p.refreshControls()
}

func (p *aiPanel) introduction() {
	p.visibleText = nil
	// A markdown heading has no trailing margin, so the greeting and its
	// description were drawn as one block; separate widgets keep the gap fixed.
	greeting := shellLabel(widget.NewLabelWithStyle("你好，我是 SuperLink AI", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}), 16)
	summary := widget.NewLabel("可以随时提问，帮你解释概念、整理思路、润色文字或排查问题。")
	summary.Wrapping = fyne.TextWrapWord
	objects := []fyne.CanvasObject{greeting, shellFixed(layout.NewSpacer(), 0, 6), summary, widget.NewSeparator()}
	for _, prompt := range []string{"解释一个技术概念", "帮我整理工作计划", "润色一段文字", "帮我分析报错原因"} {
		text := prompt
		button := widget.NewButton(text, func() {
			p.input.SetText(text + "：")
			p.owner.Window.Canvas().Focus(p.input)
		})
		button.Alignment = widget.ButtonAlignLeading
		objects = append(objects, button)
	}
	note := widget.NewLabel("仅发送本次对话内容。聊天保留至退出应用。")
	note.Wrapping = fyne.TextWrapWord
	objects = append(objects, note)
	p.messages.Objects = objects
	p.messages.Refresh()
	p.scroll.ScrollToTop()
}

func (p *aiPanel) appendMessage(role, text string) *widget.RichText {
	label := widget.NewLabelWithStyle(role, fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	rich := widget.NewRichText(&widget.TextSegment{Text: text, Style: widget.RichTextStyleInline})
	rich.Wrapping = fyne.TextWrapWord
	copyButton := shellButton("复制", "copy", false, func() { p.owner.Window.Clipboard().SetContent(rich.String()) })
	header := container.NewBorder(nil, nil, nil, copyButton, label)
	card := container.NewVBox(header, rich, widget.NewSeparator())
	p.messages.Add(card)
	p.visibleText = append(p.visibleText, text)
	p.trimVisible()
	p.scroll.ScrollToBottom()
	return rich
}

func (p *aiPanel) renderAnswer(text string, markdown bool) {
	if p.answer == nil {
		return
	}
	if markdown {
		parsed := widget.NewRichTextFromMarkdown(text)
		p.answer.Segments = safeNoteSegments(parsed.Segments)
	} else {
		p.answer.Segments = []widget.RichTextSegment{&widget.TextSegment{Text: text, Style: widget.RichTextStyleInline}}
	}
	p.answer.Refresh()
	if len(p.visibleText) > 0 {
		p.visibleText[len(p.visibleText)-1] = text
		p.trimVisible()
	}
	p.messages.Refresh()
	p.scroll.Refresh()
}

func (p *aiPanel) trimVisible() {
	size := 0
	for _, text := range p.visibleText {
		size += len(text)
	}
	// Bound shaping/layout memory too, including repeated failed requests.
	for len(p.visibleText) > 2 && (len(p.visibleText) > 40 || size > 128<<10) {
		size -= len(p.visibleText[0]) + len(p.visibleText[1])
		p.visibleText = append([]string(nil), p.visibleText[2:]...)
		p.messages.Objects = append([]fyne.CanvasObject(nil), p.messages.Objects[2:]...)
	}
}

// AI input keeps Enter for multiline text and offers the usual desktop send key.
type aiInput struct {
	widget.Entry
	send func()
}

func newAIInput(send func()) *aiInput {
	entry := &aiInput{send: send}
	entry.MultiLine = true
	entry.Wrapping = fyne.TextWrapWord
	entry.ExtendBaseWidget(entry)
	return entry
}

func (e *aiInput) TypedShortcut(shortcut fyne.Shortcut) {
	if key, ok := shortcut.(*desktop.CustomShortcut); ok && (key.KeyName == fyne.KeyReturn || key.KeyName == fyne.KeyEnter) &&
		(key.Modifier == fyne.KeyModifierControl || key.Modifier == fyne.KeyModifierSuper) {
		e.send()
		return
	}
	e.Entry.TypedShortcut(shortcut)
}
