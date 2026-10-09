package painter

import (
	"image"
	"image/draw"
	"math"

	"github.com/go-text/typesetting/font"
	"github.com/go-text/typesetting/shaping"
)

// Use real italic outlines when available. CJK fallback faces are frequently
// upright only, so shear their rasterized glyphs around the shared baseline.
// Advances and caret positions are unchanged; scratch memory is bounded by
// the destination surface rather than the length of the source text.
func needsSyntheticItalic(run shaping.Output) bool {
	return run.Face != nil && run.Face.Font.Describe().Aspect.Style != font.StyleItalic
}

func compositeSyntheticItalic(dst draw.Image, source *image.RGBA, baseline int) {
	bounds := source.Bounds()
	for y := bounds.Min.Y; y < bounds.Max.Y; y++ {
		shift := int(math.Round(float64(baseline-y) * 0.2))
		row := image.Rect(bounds.Min.X+shift, y, bounds.Max.X+shift, y+1)
		draw.Draw(dst, row, source, image.Pt(bounds.Min.X, y), draw.Over)
	}
}
