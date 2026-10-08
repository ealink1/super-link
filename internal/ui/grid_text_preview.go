package ui

import "fyne.io/fyne/v2"

type gridTextPreview struct {
	value, text string
	width, size float32
	style       fyne.TextStyle
	ready       bool
}

func (p *gridTextPreview) fit(value string, width, size float32, style fyne.TextStyle) string {
	if p.ready && p.value == value && p.width == width && p.size == size && p.style == style {
		return p.text
	}
	p.value, p.width, p.size, p.style = value, width, size, style
	p.text = fitText(value, width, size, style)
	p.ready = true
	return p.text
}
