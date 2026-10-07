package ui

import (
	"bytes"
	"image"
	"os"
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/fyne-io/oksvg"
	"github.com/srwiley/rasterx"
)

func TestDatabaseIconGallery(t *testing.T) {
	if os.Getenv("SUPERLINK_UI_CAPTURE_DIR") == "" {
		t.Skip("optional software render capture")
	}
	app := test.NewApp()
	defer app.Quit()
	window := app.NewWindow("Database logos")
	defer window.Close()
	grid := container.NewGridWithColumns(6)
	for _, descriptor := range domain.Catalog() {
		badge := newDatabaseBadge("db-" + descriptor.Key)
		badge.maxSize = 48
		grid.Add(container.NewVBox(container.NewGridWrap(fyne.NewSquareSize(64), badge), widget.NewLabel(descriptor.Name)))
	}
	window.SetContent(grid)
	window.Resize(fyne.NewSize(960, 720))
	window.Show()
	for _, dark := range []bool{false, true} {
		app.Settings().SetTheme(Theme{Dark: dark})
		name := "database-icons-light.png"
		if dark {
			name = "database-icons-dark.png"
		}
		captureConnectionIndicators(t, window, name)
	}
}

func TestDatabaseIconsDecodeAndContainArtwork(t *testing.T) {
	for _, descriptor := range domain.Catalog() {
		t.Run(descriptor.Key, func(t *testing.T) {
			name := descriptor.Key
			if name == "cache" {
				name = "iris"
			}
			resource := icon("db-" + name)
			if name != "trino" && name != "custom" && !strings.HasPrefix(resource.Name(), "db-") {
				t.Fatal("database brand resource is missing; a generic fallback is not sufficient")
			}
			var decoded image.Image
			if strings.HasSuffix(resource.Name(), ".svg") {
				vector, err := oksvg.ReadReplacingCurrentColor(bytes.NewReader(resource.Content()), "#000000")
				if err != nil {
					t.Fatal(err)
				}
				pixels := image.NewRGBA(image.Rect(0, 0, 48, 48))
				vector.SetTarget(0, 0, 48, 48)
				vector.Draw(rasterx.NewDasher(48, 48, rasterx.NewScannerGV(48, 48, pixels, pixels.Bounds())), 1)
				decoded = pixels
			} else {
				var err error
				decoded, _, err = image.Decode(bytes.NewReader(resource.Content()))
				if err != nil {
					t.Fatal(err)
				}
			}
			bounds := decoded.Bounds()
			visible := 0
			for y := bounds.Min.Y; y < bounds.Max.Y; y++ {
				for x := bounds.Min.X; x < bounds.Max.X; x++ {
					r, g, b, a := decoded.At(x, y).RGBA()
					if a > 0x8000 && (r < 0xf000 || g < 0xf000 || b < 0xf000) {
						visible++
					}
				}
			}
			if visible == 0 {
				t.Fatal("database logo contains no visible artwork")
			}
		})
	}
}
