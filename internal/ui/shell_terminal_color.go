package ui

import (
	"image/color"
	"math"
)

// Terminal glyphs are small and un-hinted, so the ink has to clear the WCAG AA
// text floor rather than the 3:1 large-text minimum.
const terminalInkContrast = 4.5

// Quantising back to 8-bit channels can land a hair short of the floor, so the
// luminance target carries a 3% margin in whichever direction the ink moves.
const terminalInkMargin = 0.97

// The vt emulator resolves ANSI colors through the fixed VGA palette in
// github.com/charmbracelet/x/ansi, which is authored for a dark screen: bright
// cyan lands at 1.2:1 on the day-mode background, which is what makes vim
// unreadable. Scaling the linear channels moves the ink away from the
// background luminance while keeping hue and saturation, so only lightness
// changes. Night mode keeps the palette as the program sent it.
func terminalInkColor(ink, background color.Color) color.Color {
	fg := terminalNRGBA(ink)
	if fg.A == 0 || currentAppearanceDark() {
		return fg
	}
	bgL := relativeLuminance(terminalNRGBA(background))
	fgL := relativeLuminance(fg)
	if contrastRatio(fgL, bgL) >= terminalInkContrast {
		return fg
	}
	if fgL == 0 {
		return fg // Pure black has no lightness left to scale in either direction.
	}
	if fgL < bgL {
		return scaleTerminalInk(fg, inkFloor(bgL)*terminalInkMargin/fgL)
	}
	// Lighter than its cell background: lift toward the ceiling. A near-white
	// ink on the light page has no headroom upward, so it darkens instead.
	if ceiling := terminalInkContrast*(bgL+0.05) - 0.05; ceiling <= 1 {
		return scaleTerminalInk(fg, ceiling/terminalInkMargin/fgL)
	}
	return scaleTerminalInk(fg, inkFloor(bgL)*terminalInkMargin/fgL)
}

// inkFloor is the luminance that lands exactly on terminalInkContrast below bgL.
func inkFloor(bgL float64) float64 { return math.Max((bgL+0.05)/terminalInkContrast-0.05, 0) }

func scaleTerminalInk(ink color.NRGBA, factor float64) color.NRGBA {
	channels := []float64{toLinearLight(ink.R), toLinearLight(ink.G), toLinearLight(ink.B)}
	for i, channel := range channels {
		channels[i] = min(max(channel*factor, 0), 1)
	}
	return color.NRGBA{
		R: toSrgb8(channels[0]), G: toSrgb8(channels[1]), B: toSrgb8(channels[2]), A: ink.A,
	}
}

func terminalNRGBA(shade color.Color) color.NRGBA {
	return color.NRGBAModel.Convert(shade).(color.NRGBA)
}

func relativeLuminance(shade color.NRGBA) float64 {
	return 0.2126*toLinearLight(shade.R) + 0.7152*toLinearLight(shade.G) + 0.0722*toLinearLight(shade.B)
}

func contrastRatio(first, second float64) float64 {
	return (math.Max(first, second) + 0.05) / (math.Min(first, second) + 0.05)
}

func toLinearLight(channel uint8) float64 {
	value := float64(channel) / 255
	if value <= 0.03928 {
		return value / 12.92
	}
	return math.Pow((value+0.055)/1.055, 2.4)
}

func toSrgb8(linear float64) uint8 {
	if linear <= 0.0031308 {
		return uint8(math.Round(linear * 12.92 * 255))
	}
	return uint8(math.Round((1.055*math.Pow(linear, 1/2.4) - 0.055) * 255))
}
