package ui

import (
	_ "embed"

	"fyne.io/fyne/v2"
)

// These complete CFF faces remain lazy fallbacks. Latin text keeps its desktop
// font, while Chinese does not require parsing a large system TrueType face.
//
//go:embed assets/NaviUI-Regular.otf
var cjkRegularData []byte

//go:embed assets/NaviUI-Bold.otf
var cjkBoldData []byte

var cjkRegularFont = fyne.NewStaticResource("SuperLink-CJK-Regular.otf", cjkRegularData)
var cjkBoldFont = fyne.NewStaticResource("SuperLink-CJK-Bold.otf", cjkBoldData)

func (Theme) FontFallbacks(style fyne.TextStyle) []fyne.Resource {
	if style.Symbol {
		return nil
	}
	if style.Bold {
		return []fyne.Resource{cjkBoldFont}
	}
	return []fyne.Resource{cjkRegularFont}
}
