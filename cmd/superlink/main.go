package main

import (
	"flag"
	"fmt"
	"os"

	"fyne.io/fyne/v2/app"
	"github.com/ealink1/super-link/internal/bootstrap"
	"github.com/ealink1/super-link/internal/branding"
	"github.com/ealink1/super-link/internal/infra/update"
	"github.com/ealink1/super-link/internal/ui"
)

var version = "0.1.0"

// Supplied by the release build. A private signing key must never be embedded.
var releasePublicKey = ""

func main() {
	root := flag.String("data-root", "", "independent application data directory")
	bundle := flag.String("drivers", "", "bundled driver directory")
	healthFile := flag.String("health-file", "", "update health handshake file")
	healthToken := flag.String("health-token", "", "update health handshake token")
	showVersion := flag.Bool("version", false, "print version")
	flag.Parse()
	if *showVersion {
		fmt.Println(version)
		return
	}
	services, err := bootstrap.Open(*root, *bundle, releasePublicKey)
	if err != nil {
		fmt.Fprintln(os.Stderr, "SuperLink startup:", err)
		os.Exit(1)
	}
	defer services.Close()
	application := app.NewWithID("io.github.ealink1.superlink")
	application.SetIcon(branding.Icon())
	window := ui.New(application, ui.Dependencies{Profiles: services.Profiles, Engine: services.Engine, Drivers: services.Drivers, Releases: services.Releases, Root: services.Root, Version: version, Close: services.Close})
	window.Show()
	window.CheckUpdatesOnStartup()
	go func() {
		<-window.Ready()
		if err := update.MarkHealthy(*healthFile, *healthToken, version); err != nil {
			fmt.Fprintln(os.Stderr, "update health:", err)
		}
	}()
	application.Run()
	if err := window.FlushAfterRun(); err != nil {
		fmt.Fprintln(os.Stderr, "workspace shutdown:", err)
	}
}
