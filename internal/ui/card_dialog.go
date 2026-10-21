package ui

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
)

// Every modal in the application is drawn from one card shell so that the
// update flow, confirmations and form dialogs share a single visual language.
const (
	cardRadius       = 28
	cardDefaultWidth = 440
	cardFormWidth    = 560
	cardPaddingX     = 26
	cardPaddingY     = 22
)

var (
	cardInk     = color.NRGBA{R: 30, G: 41, B: 59, A: 255}
	cardMuted   = color.NRGBA{R: 100, G: 116, B: 139, A: 255}
	cardHint    = color.NRGBA{R: 148, G: 163, B: 184, A: 255}
	cardFill    = color.NRGBA{R: 255, G: 255, B: 255, A: 255}
	cardSurface = color.NRGBA{R: 248, G: 250, B: 252, A: 255}
	cardAccent  = color.NRGBA{R: 74, G: 140, B: 255, A: 255}
	cardDivider = color.NRGBA{R: 226, G: 232, B: 240, A: 255}
)

type cardInfoRow struct {
	Icon  fyne.Resource
	Tint  color.NRGBA
	Label string
	Value string
}

type cardAction struct {
	Label    string
	Icon     fyne.Resource
	Primary  bool
	Accept   bool
	Disabled bool
	OnTapped func()
}

type cardSpec struct {
	Title    string
	Subtitle string
	Extra    fyne.CanvasObject
	Info     []cardInfoRow
	Body     fyne.CanvasObject
	Footer   string
	Actions  []cardAction
	Width    float32
	Height   float32
}

func cardText(value string, size float32, bold bool, ink color.Color) *canvas.Text {
	item := canvas.NewText(value, ink)
	item.TextSize = size
	item.TextStyle.Bold = bold
	item.Alignment = fyne.TextAlignCenter
	return item
}

func cardLeftText(value string, size float32, bold bool, ink color.Color) *canvas.Text {
	item := cardText(value, size, bold, ink)
	item.Alignment = fyne.TextAlignLeading
	return item
}

func cardSpace(height float32) fyne.CanvasObject {
	item := canvas.NewRectangle(color.Transparent)
	item.SetMinSize(fyne.NewSize(1, height))
	return item
}

func cardRounded(content fyne.CanvasObject, fill color.Color, radius float32) fyne.CanvasObject {
	background := canvas.NewRectangle(fill)
	background.CornerRadius = radius
	return container.NewStack(background, content)
}

func cardInfo(row cardInfoRow) fyne.CanvasObject {
	tile := cardRounded(container.NewCenter(widget.NewIcon(row.Icon)), color.NRGBA{R: row.Tint.R, G: row.Tint.G, B: row.Tint.B, A: 28}, 10)
	value := widget.NewLabel(row.Value)
	value.TextStyle.Bold = true
	value.Wrapping = fyne.TextWrapWord
	return container.NewBorder(nil, nil, container.NewGridWrap(fyne.NewSize(34, 34), tile), nil,
		container.NewVBox(cardLeftText(row.Label, 12, false, cardMuted), value))
}

func cardInfoBlock(rows []cardInfoRow) fyne.CanvasObject {
	items := make([]fyne.CanvasObject, 0, len(rows)*2)
	for index, row := range rows {
		if index > 0 {
			items = append(items, widget.NewSeparator())
		}
		items = append(items, cardInfo(row))
	}
	return cardRounded(container.NewPadded(container.NewVBox(items...)), cardSurface, 18)
}

func cardActions(actions []cardAction, tap func(cardAction)) (fyne.CanvasObject, []*widget.Button) {
	items := make([]fyne.CanvasObject, 0, len(actions)+1)
	buttons := make([]*widget.Button, 0, len(actions))
	items = append(items, layout.NewSpacer())
	for _, entry := range actions {
		action := entry
		button := widget.NewButton(action.Label, func() { tap(action) })
		if action.Icon != nil {
			button = widget.NewButtonWithIcon(action.Label, action.Icon, func() { tap(action) })
		}
		if action.Primary {
			button.Importance = widget.HighImportance
		}
		if action.Disabled {
			button.Disable()
		}
		buttons = append(buttons, button)
		items = append(items, button)
	}
	return container.NewHBox(items...), buttons
}

// buildCard assembles the shared card shell. Every modal in the application is
// expected to go through this function rather than styling itself.
//
// The layout is a border so the body owns all remaining height and can scroll
// when the window is too small for the content.
func buildCard(spec cardSpec, tap func(cardAction)) (fyne.CanvasObject, []*widget.Button, fyne.CanvasObject) {
	top := make([]fyne.CanvasObject, 0, 8)
	if spec.Title != "" {
		top = append(top, cardText(spec.Title, 24, true, cardInk))
	}
	if spec.Subtitle != "" {
		top = append(top, cardText(spec.Subtitle, 13, false, cardMuted))
	}
	if spec.Extra != nil {
		top = append(top, cardSpace(10), container.NewCenter(spec.Extra))
	}
	if len(spec.Info) > 0 {
		top = append(top, cardSpace(12), cardInfoBlock(spec.Info))
	}
	var buttons []*widget.Button
	bottom := make([]fyne.CanvasObject, 0, 3)
	if len(spec.Actions) > 0 {
		actions, built := cardActions(spec.Actions, tap)
		buttons = built
		bottom = append(bottom, cardSpace(16), actions)
	}
	if spec.Footer != "" {
		bottom = append(bottom, cardSpace(10), cardText(spec.Footer, 11, false, cardHint))
	}
	var body, host fyne.CanvasObject = layout.NewSpacer(), nil
	if spec.Body != nil {
		// Wrapped labels size themselves; forms and lists need a scroll host.
		body = spec.Body
		if _, label := spec.Body.(*widget.Label); !label {
			host = container.NewVScroll(spec.Body)
			body = host
		}
	}
	content := container.NewBorder(container.NewVBox(top...), container.NewVBox(bottom...), nil, nil, body)
	inner := container.New(layout.NewCustomPaddedLayout(cardPaddingY, cardPaddingY, cardPaddingX, cardPaddingX), content)
	return cardRounded(inner, cardFill, cardRadius), buttons, host
}

// fitCardBody wraps a plain message label at the card width so its height is
// measured with real line breaks instead of a single long line.
func fitCardBody(body fyne.CanvasObject, width float32) {
	label, ok := body.(*widget.Label)
	if !ok || label.Wrapping == fyne.TextWrapOff {
		return
	}
	inner := max(64, width-2*cardPaddingX)
	label.Resize(fyne.NewSize(inner, label.MinSize().Height))
}

// cardDialog replaces the stock Fyne dialog for every modal surface.
type cardDialog struct {
	popup    *widget.PopUp
	stack    *fyne.Container
	spec     cardSpec
	buttons  []*widget.Button
	host     fyne.CanvasObject
	respond  func(bool)
	onClosed func()
	closed   bool
}

func newCardDialog(spec cardSpec, parent fyne.Window, respond func(bool)) *cardDialog {
	d := &cardDialog{spec: spec, respond: respond}
	width := spec.Width
	if width <= 0 {
		width = cardDefaultWidth
	}
	if spec.Body != nil {
		fitCardBody(spec.Body, width)
	}
	d.stack = container.NewStack()
	d.popup = widget.NewModalPopUp(d.stack, parent.Canvas())
	card, buttons, host := buildCard(spec, d.choose)
	d.buttons, d.host = buttons, host
	d.stack.Add(card)
	d.resize()
	return d
}

// SetActionLabel renames one of the card's buttons.
func (d *cardDialog) SetActionLabel(index int, value string) {
	if index < 0 || index >= len(d.buttons) {
		return
	}
	d.buttons[index].SetText(value)
}

// SetActionDisabled enables or disables one of the card's buttons.
func (d *cardDialog) SetActionDisabled(index int, disabled bool) {
	if index < 0 || index >= len(d.buttons) {
		return
	}
	if disabled {
		d.buttons[index].Disable()
		return
	}
	d.buttons[index].Enable()
}

func (d *cardDialog) choose(action cardAction) {
	d.popup.Hide()
	d.fire(action.Accept)
	if action.OnTapped != nil {
		action.OnTapped()
	}
}

func (d *cardDialog) fire(accepted bool) {
	if d.closed {
		return
	}
	d.closed = true
	if respond := d.respond; respond != nil {
		d.respond = nil
		respond(accepted)
	}
	if closed := d.onClosed; closed != nil {
		d.onClosed = nil
		closed()
	}
}

// Show presents the card, sizing it to its content.
func (d *cardDialog) Show() { d.resize(); d.popup.Show() }

// Hide dismisses the card without accepting it.
func (d *cardDialog) Hide() {
	d.popup.Hide()
	d.fire(false)
}

// Confirm accepts the card, mirroring the stock confirmation dialog.
func (d *cardDialog) Confirm() {
	d.popup.Hide()
	d.fire(true)
}

// SetOnClosed registers a callback invoked once when the card disappears.
func (d *cardDialog) SetOnClosed(callback func()) { d.onClosed = callback }

// Resize overrides the preferred width and height; zero keeps the automatic size.
func (d *cardDialog) Resize(size fyne.Size) {
	if size.Width > 0 {
		d.spec.Width = size.Width
	}
	if size.Height > 0 {
		d.spec.Height = size.Height
	}
	d.resize()
}

func (d *cardDialog) resize() {
	card := d.stack.Objects
	if len(card) == 0 {
		return
	}
	minSize := card[0].MinSize()
	width := d.spec.Width
	if width <= 0 {
		width = cardDefaultWidth
	}
	if minSize.Width > width {
		width = minSize.Width
	}
	height := d.spec.Height
	if height <= 0 {
		height = minSize.Height
		// The scroll host reports a fixed minimum, so restore the real body height.
		if d.host != nil && d.spec.Body != nil {
			height += d.spec.Body.MinSize().Height - d.host.MinSize().Height
		}
	}
	if canvas := d.popup.Canvas; canvas != nil {
		available := canvas.Size()
		if available.Width > 64 && width > available.Width-32 {
			width = available.Width - 32
		}
		if available.Height > 96 && height > available.Height-56 {
			height = available.Height - 56
		}
	}
	d.popup.Resize(fyne.NewSize(width, height))
}

// newCardCustom mirrors dialog.NewCustom with the shared card shell.
func newCardCustom(title, dismiss string, content fyne.CanvasObject, parent fyne.Window) *cardDialog {
	return newCardDialog(cardSpec{
		Title:   title,
		Body:    content,
		Width:   cardDefaultWidth,
		Actions: []cardAction{{Label: dismiss}},
	}, parent, nil)
}

// newCardCustomConfirm mirrors dialog.NewCustomConfirm with the shared card shell.
func newCardCustomConfirm(title, confirm, dismiss string, content fyne.CanvasObject, callback func(bool), parent fyne.Window) *cardDialog {
	return newCardDialog(cardSpec{
		Title: title,
		Body:  content,
		Width: cardFormWidth,
		Actions: []cardAction{
			{Label: dismiss},
			{Label: confirm, Primary: true, Accept: true},
		},
	}, parent, callback)
}

// newCardCustomNoButtons hosts content that renders its own controls.
func newCardCustomNoButtons(title string, content fyne.CanvasObject, parent fyne.Window) *cardDialog {
	return newCardDialog(cardSpec{Title: title, Body: content, Width: cardFormWidth}, parent, nil)
}

// newCardForm mirrors dialog.NewForm with the shared card shell.
func newCardForm(title, confirm, dismiss string, items []*widget.FormItem, callback func(bool), parent fyne.Window) *cardDialog {
	return newCardDialog(cardSpec{
		Title: title,
		Body:  widget.NewForm(items...),
		Width: cardFormWidth,
		Actions: []cardAction{
			{Label: dismiss},
			{Label: confirm, Primary: true, Accept: true},
		},
	}, parent, callback)
}

// showCardForm presents a card form immediately.
func showCardForm(title, confirm, dismiss string, items []*widget.FormItem, callback func(bool), parent fyne.Window) {
	newCardForm(title, confirm, dismiss, items, callback, parent).Show()
}

// cardFrame gives dialogs that draw their own controls the shared card surface.
func cardFrame(content fyne.CanvasObject) fyne.CanvasObject {
	return cardRounded(container.New(layout.NewCustomPaddedLayout(14, 14, 14, 14), content), cardFill, cardRadius)
}
