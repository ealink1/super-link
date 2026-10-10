package application

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"unicode/utf8"

	"github.com/ealink1/super-link/internal/domain"
)

const profileGroupOptionsKey = "connection.groupOptions"

var connectionGroupColors = []string{"#4f6ef7", "#10b981", "#f59e0b", "#ef4444", "#8b5cf6", "#0ea5e9"}

func (p *Profiles) GroupOptions(ctx context.Context) (map[string]domain.ConnectionGroupOptions, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.groupOptions(ctx)
}

func (p *Profiles) groupOptions(ctx context.Context) (map[string]domain.ConnectionGroupOptions, error) {
	raw, err := p.Store.Setting(ctx, profileGroupOptionsKey)
	if err != nil {
		return nil, err
	}
	options := map[string]domain.ConnectionGroupOptions{}
	if raw != "" {
		err = json.Unmarshal([]byte(raw), &options)
	}
	if options == nil {
		options = map[string]domain.ConnectionGroupOptions{}
	}
	return options, err
}

func (p *Profiles) CreateGroupWithOptions(ctx context.Context, name, parent string, options domain.ConnectionGroupOptions) error {
	name = strings.TrimSpace(name)
	if length := utf8.RuneCountInString(name); length < 2 || length > 20 {
		return errors.New("分组名称须为 2–20 个字符")
	}
	options.Description = strings.TrimSpace(options.Description)
	if utf8.RuneCountInString(options.Description) > 60 {
		return errors.New("描述最多 60 个字符")
	}
	if options.Color == "" {
		options.Color = connectionGroupColors[0]
	}
	validColor := false
	for _, color := range connectionGroupColors {
		validColor = validColor || options.Color == color
	}
	if !validColor {
		return errors.New("请选择有效的标识色")
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.createGroup(ctx, name, parent, nil, &options)
}

func defaultConnectionGroup(options map[string]domain.ConnectionGroupOptions) string {
	for name, option := range options {
		if option.Default {
			return name
		}
	}
	return ""
}
