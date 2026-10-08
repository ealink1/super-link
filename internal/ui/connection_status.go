package ui

import (
	"image/color"
	"maps"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
)

func (n *navigator) watchConnectionStatuses() {
	if n.owner.Engine == nil {
		return
	}
	n.connectionStatuses = n.owner.Engine.ConnectionStatuses()
	n.owner.jobs.watch(n.owner.Engine.ConnectionChanges(), func() {
		states := n.owner.Engine.ConnectionStatuses()
		if !maps.Equal(states, n.connectionStatuses) {
			n.connectionStatuses = states
			n.tree.Refresh()
		}
	})
}

func (n *navigator) resetDisconnectedConnection(id string) {
	node := n.nodes["connection:"+id]
	if node == nil {
		return
	}
	node.generation++
	if node.cancel != nil {
		node.cancel()
	}
	n.removeChildren(node)
	node.loaded, node.busy = false, false
	n.tree.CloseBranch(node.id)
	n.tree.Refresh()
}

type connectionDot struct {
	widget.BaseWidget
	status       application.ConnectionStatus
	halo, center *canvas.Circle
}

func newConnectionDot() *connectionDot {
	d := &connectionDot{halo: canvas.NewCircle(color.Transparent), center: canvas.NewCircle(color.Transparent)}
	d.ExtendBaseWidget(d)
	return d
}

func (d *connectionDot) setStatus(status application.ConnectionStatus) {
	d.status = status
	d.Refresh()
}

func (d *connectionDot) CreateRenderer() fyne.WidgetRenderer {
	return &connectionDotRenderer{dot: d}
}

type connectionDotRenderer struct{ dot *connectionDot }

func (*connectionDotRenderer) MinSize() fyne.Size { return fyne.NewSize(14, 14) }
func (r *connectionDotRenderer) Layout(size fyne.Size) {
	edge := min(float32(12), size.Width, size.Height)
	r.dot.halo.Resize(fyne.NewSize(edge, edge))
	r.dot.halo.Move(fyne.NewPos((size.Width-edge)/2, (size.Height-edge)/2))
	inner := edge * 0.6
	r.dot.center.Resize(fyne.NewSize(inner, inner))
	r.dot.center.Move(fyne.NewPos((size.Width-inner)/2, (size.Height-inner)/2))
}
func (r *connectionDotRenderer) Objects() []fyne.CanvasObject {
	return []fyne.CanvasObject{r.dot.halo, r.dot.center}
}
func (r *connectionDotRenderer) Refresh() {
	shade := theme.ErrorColor()
	switch r.dot.status {
	case application.ConnectionConnected:
		shade = theme.SuccessColor()
	case application.ConnectionConnecting:
		shade = theme.WarningColor()
	}
	center := color.NRGBAModel.Convert(shade).(color.NRGBA)
	r.dot.center.FillColor = center
	center.A = 48
	r.dot.halo.FillColor = center
	r.Layout(r.dot.Size())
	r.dot.halo.Refresh()
	r.dot.center.Refresh()
}
func (*connectionDotRenderer) Destroy() {}
