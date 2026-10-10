package application

import (
	"context"
	"encoding/json"
	"errors"
	"slices"
	"strings"

	"github.com/ealink1/super-link/internal/domain"
)

const profileGroupsKey = "connection.groups"
const profileGroupParentsKey = "connection.groupParents"

func (p *Profiles) Groups(ctx context.Context) ([]string, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.groups(ctx)
}

func (p *Profiles) groups(ctx context.Context) ([]string, error) {
	raw, err := p.Store.Setting(ctx, profileGroupsKey)
	if err != nil {
		return nil, err
	}
	var groups []string
	if raw != "" {
		err = json.Unmarshal([]byte(raw), &groups)
	}
	return groups, err
}

func (p *Profiles) AddGroup(ctx context.Context, name string) error {
	return p.CreateGroup(ctx, name, "", nil)
}

func (p *Profiles) GroupParents(ctx context.Context) (map[string]string, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.groupParents(ctx)
}
func (p *Profiles) groupParents(ctx context.Context) (map[string]string, error) {
	raw, err := p.Store.Setting(ctx, profileGroupParentsKey)
	if err != nil {
		return nil, err
	}
	parents := map[string]string{}
	if raw != "" {
		err = json.Unmarshal([]byte(raw), &parents)
	}
	if parents == nil {
		parents = map[string]string{}
	}
	return parents, err
}

func (p *Profiles) CreateGroup(ctx context.Context, name, parent string, selected []domain.Profile) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.createGroup(ctx, name, parent, selected, nil)
}

func (p *Profiles) createGroup(ctx context.Context, name, parent string, selected []domain.Profile, option *domain.ConnectionGroupOptions) error {
	name = strings.TrimSpace(name)
	if name == "" || name == "未分组" {
		return errors.New("请输入有效的分组名称")
	}
	groups, err := p.groups(ctx)
	if err != nil {
		return err
	}
	profiles, err := p.Store.Profiles(ctx)
	if err != nil {
		return err
	}
	for _, profile := range profiles {
		if profile.Group == name {
			return errors.New("分组名称已存在")
		}
	}
	if slices.Contains(groups, name) {
		return errors.New("分组名称已存在")
	}
	parents, err := p.groupParents(ctx)
	if err != nil {
		return err
	}
	parent = strings.TrimSpace(parent)
	if parent != "" {
		exists := slices.Contains(groups, parent)
		for _, profile := range profiles {
			exists = exists || profile.Group == parent
		}
		if !exists {
			return errors.New("父分组不存在，请刷新后重试")
		}
		if !slices.Contains(groups, parent) {
			groups = append(groups, parent)
		}
		parents[name] = parent
	}
	groups = append(groups, name)
	raw, err := json.Marshal(groups)
	if err != nil {
		return err
	}
	parentData, err := json.Marshal(parents)
	if err != nil {
		return err
	}
	settings := map[string]string{profileGroupsKey: string(raw), profileGroupParentsKey: string(parentData)}
	if option != nil {
		options, err := p.groupOptions(ctx)
		if err != nil {
			return err
		}
		if option.Default {
			for group, value := range options {
				value.Default = false
				options[group] = value
			}
		}
		options[name] = *option
		encoded, err := json.Marshal(options)
		if err != nil {
			return err
		}
		settings[profileGroupOptionsKey] = string(encoded)
	}
	return p.Store.CreateConnectionGroup(ctx, settings, selected, name)
}

// MoveToGroup updates only public metadata and commits the selection atomically.
func (p *Profiles) MoveToGroup(ctx context.Context, profiles []domain.Profile, group string) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.Store.MoveProfilesToGroup(ctx, profiles, strings.TrimSpace(group))
}
