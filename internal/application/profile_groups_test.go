package application

import (
	"errors"
	"strings"
	"testing"

	"github.com/ealink1/super-link/internal/domain"
)

func TestProfileGroupsPersistAndMoveAtomically(t *testing.T) {
	profiles := testProfiles(t)
	first := saveProfile(t, profiles, "postgres", false)
	second := saveProfile(t, profiles, "postgres", false)
	if first.CreatedAt.IsZero() {
		t.Fatal("new profile has no creation time")
	}
	if err := profiles.AddGroup(t.Context(), "  生产环境  "); err != nil {
		t.Fatal(err)
	}
	groups, err := profiles.Groups(t.Context())
	if err != nil || len(groups) != 1 || groups[0] != "生产环境" {
		t.Fatal(groups, err)
	}
	if err := profiles.AddGroup(t.Context(), "生产环境"); err == nil {
		t.Fatal("duplicate group allowed")
	}
	current, err := profiles.Get(t.Context(), second.ID)
	if err != nil {
		t.Fatal(err)
	}
	current.Name = "changed"
	current, err = profiles.Save(t.Context(), current)
	if err != nil {
		t.Fatal(err)
	}
	if !current.CreatedAt.Equal(second.CreatedAt) {
		t.Fatal("edit replaced creation time")
	}
	err = profiles.MoveToGroup(t.Context(), []domain.Profile{first, second}, "生产环境")
	if !errors.Is(err, domain.ErrConflict) {
		t.Fatal("stale selection did not conflict", err)
	}
	stored, err := profiles.Get(t.Context(), first.ID)
	if err != nil || stored.Group != first.Group {
		t.Fatal("partial move was committed", stored.Group, err)
	}
	if err := profiles.MoveToGroup(t.Context(), []domain.Profile{stored, current}, "生产环境"); err != nil {
		t.Fatal(err)
	}
	stored, err = profiles.Get(t.Context(), first.ID)
	if err != nil || stored.Group != "生产环境" || !stored.CreatedAt.Equal(first.CreatedAt) {
		t.Fatal("move failed", stored, err)
	}
}

func TestCreateChildGroupMovesConnectionsAtomically(t *testing.T) {
	profiles := testProfiles(t)
	p := saveProfile(t, profiles, "postgres", false)
	if err := profiles.AddGroup(t.Context(), "生产环境"); err != nil {
		t.Fatal(err)
	}
	if err := profiles.CreateGroup(t.Context(), "国内", "生产环境", []domain.Profile{p}); err != nil {
		t.Fatal(err)
	}
	stored, err := profiles.Get(t.Context(), p.ID)
	if err != nil || stored.Group != "国内" {
		t.Fatal("creation did not assign selected connection", err)
	}
	parents, err := profiles.GroupParents(t.Context())
	if err != nil || parents["国内"] != "生产环境" {
		t.Fatal("parent not persisted", parents, err)
	}
	if err := profiles.CreateGroup(t.Context(), "失败分组", "生产环境", []domain.Profile{p}); !errors.Is(err, domain.ErrConflict) {
		t.Fatal("stale selection allowed", err)
	}
	groups, err := profiles.Groups(t.Context())
	if err != nil {
		t.Fatal(err)
	}
	for _, name := range groups {
		if name == "失败分组" {
			t.Fatal("group committed without connection assignment")
		}
	}
	parents, err = profiles.GroupParents(t.Context())
	if err != nil || parents["失败分组"] != "" {
		t.Fatal("partial parent metadata committed")
	}
	if err := profiles.CreateGroup(t.Context(), "空分组", "不存在", nil); err == nil {
		t.Fatal("invalid parent accepted")
	}
}

func TestGroupOptionsPersistAndDefaultOnlyAppliesToNewConnections(t *testing.T) {
	profiles := testProfiles(t)
	existing := saveProfile(t, profiles, "postgres", false)
	option := domain.ConnectionGroupOptions{Color: "#10b981", Description: "  生产数据库  ", Default: true}
	if err := profiles.CreateGroupWithOptions(t.Context(), "生产环境", "", option); err != nil {
		t.Fatal(err)
	}
	options, err := profiles.GroupOptions(t.Context())
	if err != nil || options["生产环境"].Description != "生产数据库" || options["生产环境"].Color != option.Color || !options["生产环境"].Default {
		t.Fatal("options not saved", options, err)
	}
	created := saveProfile(t, profiles, "postgres", false)
	if created.Group != "生产环境" {
		t.Fatal("default not applied", created.Group)
	}
	existing.Name = "edited"
	existing, err = profiles.Save(t.Context(), existing)
	if err != nil || existing.Group != "" {
		t.Fatal("editing reassigns an existing ungrouped connection", existing.Group, err)
	}
	explicit := existing
	explicit.ID, explicit.Revision, explicit.Group = "", 0, "自定义分组"
	explicit, err = profiles.Save(t.Context(), explicit)
	if err != nil || explicit.Group != "自定义分组" {
		t.Fatal("default replaced explicit group", explicit.Group, err)
	}
	if err := profiles.CreateGroupWithOptions(t.Context(), "测试环境", "生产环境", domain.ConnectionGroupOptions{Default: true}); err != nil {
		t.Fatal(err)
	}
	options, err = profiles.GroupOptions(t.Context())
	if err != nil || options["生产环境"].Default || !options["测试环境"].Default {
		t.Fatal("more than one default group", options, err)
	}
	// Duplicate failure must retain the previous default and options.
	if err := profiles.CreateGroupWithOptions(t.Context(), "生产环境", "", option); err == nil {
		t.Fatal("duplicate succeeded")
	}
	created = saveProfile(t, profiles, "postgres", false)
	if created.Group != "测试环境" {
		t.Fatal("failed creation replaced default", created.Group)
	}
}

func TestGroupOptionsRejectInvalidInput(t *testing.T) {
	profiles := testProfiles(t)
	for _, input := range []struct{ name, description, color string }{
		{"华", "", ""}, {strings.Repeat("华", 21), "", ""},
		{"合法名称", strings.Repeat("华", 61), ""}, {"合法名称", "", "not-a-color"},
	} {
		if err := profiles.CreateGroupWithOptions(t.Context(), input.name, "", domain.ConnectionGroupOptions{Description: input.description, Color: input.color}); err == nil {
			t.Fatal("invalid preferences accepted", input)
		}
	}
	if err := profiles.CreateGroupWithOptions(t.Context(), strings.Repeat("华", 20), "", domain.ConnectionGroupOptions{Description: strings.Repeat("华", 60)}); err != nil {
		t.Fatal("Unicode limits not respected", err)
	}
}
