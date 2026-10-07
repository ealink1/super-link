package ui

import (
	"embed"
	"fmt"
	"image/color"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/theme"
)

// The upstream icon geometry is preserved; sources.json records every asset.
//
//go:embed assets/gonavi/*.svg assets/gonavi/*.png
var sourceIcons embed.FS

func icon(name string) fyne.Resource {
	file := name + ".svg"
	raw, err := sourceIcons.ReadFile("assets/gonavi/" + file)
	if err != nil && strings.HasPrefix(name, "db-") {
		file = name + ".png"
		raw, err = sourceIcons.ReadFile("assets/gonavi/" + file)
	}
	if err != nil {
		if strings.HasPrefix(name, "db-") {
			// Database badges stay white in both themes; theme foreground icons
			// disappear against that white background in dark mode.
			return coloredIcon("database", "#64748b")
		}
		return theme.DocumentIcon()
	}
	resource := fyne.NewStaticResource(file, raw)
	if strings.HasPrefix(name, "db-") {
		if strings.HasSuffix(file, ".svg") {
			shade := databaseColors[strings.TrimPrefix(name, "db-")]
			if shade != "" {
				source := strings.ReplaceAll(string(raw), "currentColor", shade)
				if end := strings.Index(source, ">"); end >= 0 && !strings.Contains(source[:end], "fill=") {
					source = strings.Replace(source, "<svg", `<svg fill="`+shade+`"`, 1)
				}
				return fyne.NewStaticResource(file, []byte(source))
			}
		}
		return resource
	}
	// Fyne's ThemedResource replaces fill="none", filling outline icons. Keep
	// the original geometry and resolve only currentColor instead.
	return &outlineIcon{name: name, source: raw}
}

type outlineIcon struct {
	name    string
	source  []byte
	inverse bool
	shade   string
}

func (r *outlineIcon) color() string {
	if r.shade != "" {
		return r.shade
	}
	if r.inverse {
		return "#ffffff"
	}
	value := color.NRGBAModel.Convert(theme.ForegroundColor()).(color.NRGBA)
	return fmt.Sprintf("#%02x%02x%02x", value.R, value.G, value.B)
}

func coloredIcon(name, shade string) fyne.Resource {
	resource := icon(name)
	if outline, ok := resource.(*outlineIcon); ok {
		outline.shade = shade
	}
	return resource
}
func (r *outlineIcon) Name() string { return r.name + "-" + r.color() + ".svg" }
func (r *outlineIcon) Content() []byte {
	return []byte(strings.ReplaceAll(string(r.source), "currentColor", r.color()))
}
func whiteIcon(name string) fyne.Resource {
	resource := icon(name)
	if outline, ok := resource.(*outlineIcon); ok {
		outline.inverse = true
	}
	return resource
}
