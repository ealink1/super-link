package ui

import (
	"fyne.io/fyne/v2/theme"
	"reflect"
	"testing"
)

func TestReadOnlyDDLTextContrast(t *testing.T) {
	for _, dark := range []bool{false, true} {
		base := Theme{Dark: dark}
		textTheme := ddlTextTheme{base}
		if !reflect.DeepEqual(textTheme.Color(theme.ColorNameDisabled, theme.VariantLight), base.Color(theme.ColorNameForeground, theme.VariantLight)) {
			t.Fatal("DDL text uses faded disabled color")
		}
		if textTheme.Size(theme.SizeNameText) != 14 {
			t.Fatal("DDL font size too small")
		}
	}
}
