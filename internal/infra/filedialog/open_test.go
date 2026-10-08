package filedialog

import (
	"reflect"
	"testing"
)

func TestOpenCommandKeepsFilenameAsLiteralArgument(t *testing.T) {
	filename := `/tmp/a "quote";$(touch surprise).txt`
	for _, platform := range []string{"darwin", "linux", "windows"} {
		_, args := localOpenCommand(platform, filename)
		if !reflect.DeepEqual(args[len(args)-1:], []string{filename}) {
			t.Fatal("filename became shell code", platform, args)
		}
	}
}
