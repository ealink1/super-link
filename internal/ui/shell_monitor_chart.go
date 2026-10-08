package ui

import (
	"image"
	"image/color"
	"math"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/widget"
)

type monitorPoint struct {
	Time               time.Time
	User, System, Used float64
}
type monitorChart struct {
	widget.BaseWidget
	points []monitorPoint
	scale  float64
	total  bool
}

func newTotalMonitorChart(points []monitorPoint) *monitorChart {
	c := newMonitorScaledChart(points, 100)
	c.total = true
	return c
}
func newMonitorChart(points []monitorPoint) *monitorChart { return newMonitorScaledChart(points, 100) }
func newMonitorScaledChart(points []monitorPoint, scale float64) *monitorChart {
	c := &monitorChart{points: append([]monitorPoint(nil), points...), scale: max(1, scale)}
	c.ExtendBaseWidget(c)
	return c
}
func (c *monitorChart) CreateRenderer() fyne.WidgetRenderer {
	raster := canvas.NewRaster(c.plot)
	raster.SetMinSize(fyne.NewSize(240, 100))
	return widget.NewSimpleRenderer(raster)
}

// Draw each whole series into one raster. Adjacent canvas.Line objects leave
// seams at their texture boundaries on native GL renderers.
func (c *monitorChart) plot(width, height int) image.Image {
	im := image.NewRGBA(image.Rect(0, 0, width, height))
	if width < 1 || height < 1 {
		return im
	}
	for _, y := range []int{height / 4, height / 2, 3 * height / 4} {
		for x := 0; x < width; x += 4 {
			im.SetRGBA(x, y, color.RGBA{22, 24, 25, 40})
		}
	}
	if len(c.points) < 2 {
		return im
	}
	end := c.points[len(c.points)-1].Time
	span := end.Sub(c.points[0].Time).Seconds()
	if span <= 0 {
		span = 60
	}
	if c.total {
		c.plotTotal(im, width, height, span)
		return im
	}
	for i := 1; i < len(c.points); i++ {
		for series := 0; series < 2; series++ {
			a, b := c.points[i-1].User, c.points[i].User
			shade := color.RGBA{0, 158, 170, 255}
			if series == 1 {
				a, b = c.points[i-1].System, c.points[i].System
				shade = color.RGBA{78, 111, 193, 255}
			}
			x1 := int(float64(width-1) * (1 - end.Sub(c.points[i-1].Time).Seconds()/span))
			x2 := int(float64(width-1) * (1 - end.Sub(c.points[i].Time).Seconds()/span))
			y1 := int(float64(height-1) * (1 - min(1, a/c.scale)))
			y2 := int(float64(height-1) * (1 - min(1, b/c.scale)))
			monitorPlotLine(im, max(0, x1), max(0, y1), max(0, x2), max(0, y2), shade)
		}
	}
	return im
}
func monitorPlotLine(im *image.RGBA, x1, y1, x2, y2 int, shade color.RGBA) {
	dx, dy := absMonitor(x2-x1), -absMonitor(y2-y1)
	sx, sy := -1, -1
	if x1 < x2 {
		sx = 1
	}
	if y1 < y2 {
		sy = 1
	}
	err := dx + dy
	for {
		im.SetRGBA(x1, y1, shade)
		im.SetRGBA(x1, y1+1, shade)
		if x1 == x2 && y1 == y2 {
			return
		}
		e := 2 * err
		if e >= dy {
			err += dy
			x1 += sx
		}
		if e <= dx {
			err += dx
			y1 += sy
		}
	}
}
func absMonitor(n int) int {
	if n < 0 {
		return -n
	}
	return n
}

func (c *monitorChart) plotTotal(im *image.RGBA, width, height int, span float64) {
	first := c.points[0].Time
	index := 0
	for x := 0; x < width; x++ {
		seconds := float64(x) / float64(max(1, width-1)) * span
		for index+1 < len(c.points)-1 && c.points[index+1].Time.Sub(first).Seconds() < seconds {
			index++
		}
		a, b := c.points[index], c.points[min(index+1, len(c.points)-1)]
		delta := b.Time.Sub(a.Time).Seconds()
		t := float64(0)
		if delta > 0 {
			t = min(1, max(0, (seconds-a.Time.Sub(first).Seconds())/delta))
		}
		t = t * t * (3 - 2*t)
		value := a.Used + (b.Used-a.Used)*t
		position := float64(height-1) * (1 - min(1, max(0, value/100)))
		y := int(position)
		for row := y + 1; row < height; row++ {
			alpha := uint8(26 * float64(height-row) / float64(max(1, height-y)))
			im.Set(x, row, color.NRGBA{17, 151, 161, alpha})
		}
		// Fractional coverage keeps the smooth curve crisp at both 1x and Retina.
		for row := y - 1; row <= y+2; row++ {
			coverage := min(1, max(0, 1.3-math.Abs(float64(row)-position)))
			if coverage > 0 {
				im.Set(x, row, color.NRGBA{17, 151, 161, uint8(255 * coverage)})
			}
		}
	}
}
