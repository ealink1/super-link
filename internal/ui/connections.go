package ui

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

type connectionEditor struct {
	owner                                                                         *Window
	original                                                                      domain.Profile
	modal                                                                         *widget.PopUp
	name, group, host, port, user, password, database, uri, dsn, driver, extra    *widget.Entry
	kind, environment                                                             *widget.Select
	readonly, persist                                                             *widget.Check
	sshEnabled, proxyEnabled, tlsEnabled                                          *widget.Check
	sshHost, sshPort, sshUser, sshPassword, sshKey, knownHosts, fingerprint       *widget.Entry
	proxyType                                                                     *widget.Select
	proxyHost, proxyPort, proxyUser, proxyPassword, ca, cert, key                 *widget.Entry
	tlsMode                                                                       *widget.Select
	hint                                                                          *widget.Label
	params, timeout, queryTimeout, hosts, allow, include, exclude                 *widget.Entry
	topology                                                                      *widget.Select
	protectEdit, protectStructure, protectScript, protectImport, connectAfterSave *widget.Check
	iconType, iconColor                                                           string
	testCancel                                                                    context.CancelFunc
	closed, saving                                                                bool
}

func (w *Window) editProfile(p domain.Profile) {
	w.editProfileWithHint(p, "")
}

func (w *Window) editProfileWithHint(p domain.Profile, hint string) {
	if p.ID == "" && p.Config.Type == "" {
		w.typePicker()
		return
	}
	editor := &connectionEditor{owner: w, original: p}
	editor.show()
	if hint != "" {
		editor.hint.SetText(hint)
	}
}
func (e *connectionEditor) entry(text string, password bool) *widget.Entry {
	item := widget.NewEntry()
	item.Password = password
	item.SetText(text)
	return item
}
func (e *connectionEditor) initFields() {
	p := e.original
	c := p.Config
	e.name = e.entry(p.Name, false)
	e.group = e.entry(p.Group, false)
	if e.owner != nil && p.Revision == 0 && p.Group == "" {
		for name, option := range e.owner.profileGroupOptions {
			if option.Default {
				e.group.SetText(name)
				break
			}
		}
	}
	e.host = e.entry(c.Host, false)
	e.port = e.entry(strconv.Itoa(c.Port), false)
	e.user = e.entry(c.User, false)
	e.password = e.entry(c.Password, true)
	e.database = e.entry(c.Database, false)
	e.uri = e.entry(c.URI, true)
	e.dsn = e.entry(c.DSN, true)
	e.driver = e.entry(c.Driver, false)
	names := []string{}
	selected := ""
	for _, d := range domain.Catalog() {
		names = append(names, d.Name)
		if d.Key == c.Type {
			selected = d.Name
		}
	}
	e.kind = widget.NewSelect(names, func(name string) {
		for _, d := range domain.Catalog() {
			if d.Name == name {
				e.port.SetText(strconv.Itoa(d.Port))
				if e.original.ID == "" && (d.Key == "sqlite" || d.Key == "duckdb") && (e.host.Text == "127.0.0.1" || e.host.Text == "localhost") {
					e.host.SetText(":memory:")
				}
				if e.host.Text == ":memory:" && d.Key != "sqlite" && d.Key != "duckdb" {
					e.host.SetText("127.0.0.1")
				}
				e.hint.SetText(protocolHint(d))
				break
			}
		}
	})
	e.hint = widget.NewLabel("")
	e.hint.Wrapping = fyne.TextWrapWord
	e.kind.SetSelected(selected)
	e.port.SetText(strconv.Itoa(c.Port))
	e.environment = widget.NewSelect([]string{"dev", "test", "prod"}, nil)
	env := p.Environment
	if env == "" {
		env = "dev"
	}
	e.environment.SetSelected(env)
	e.readonly = widget.NewCheck("只读保护（服务端也应使用只读账号）", nil)
	e.readonly.SetChecked(p.ReadOnly)
	e.persist = widget.NewCheck("记住密码", nil)
	e.persist.SetChecked(c.SavePassword || p.ID == "")
}
func (e *connectionEditor) collect() (domain.Profile, error) {
	p := e.original
	cfg, err := overlayConfig(p.Config, e.extra.Text)
	if err != nil {
		return p, fmt.Errorf("高级配置 JSON：%w", err)
	}
	for _, d := range domain.Catalog() {
		if d.Name == e.kind.Selected {
			cfg.Type = d.Key
		}
	}
	port, err := parsePort(e.port.Text)
	if err != nil {
		return p, err
	}
	cfg.Port = port
	cfg.Host = strings.TrimSpace(e.host.Text)
	cfg.User = e.user.Text
	cfg.Password = e.password.Text
	cfg.Database = e.database.Text
	cfg.URI = e.uri.Text
	cfg.DSN = e.dsn.Text
	cfg.Driver = e.driver.Text
	cfg.SavePassword = e.persist.Checked
	cfg.UseSSH = e.sshEnabled.Checked
	cfg.SSH.Host = e.sshHost.Text
	cfg.SSH.Port, err = parsePort(e.sshPort.Text)
	if err != nil {
		return p, err
	}
	cfg.SSH.User = e.sshUser.Text
	cfg.SSH.Password = e.sshPassword.Text
	cfg.SSH.KeyPath = e.sshKey.Text
	cfg.SSH.KnownHostsPath = e.knownHosts.Text
	cfg.SSH.HostKeyFingerprint = e.fingerprint.Text
	cfg.UseProxy = e.proxyEnabled.Checked
	cfg.Proxy.Type = e.proxyType.Selected
	cfg.Proxy.Host = e.proxyHost.Text
	cfg.Proxy.Port, err = parsePort(e.proxyPort.Text)
	if err != nil {
		return p, err
	}
	cfg.Proxy.User = e.proxyUser.Text
	cfg.Proxy.Password = e.proxyPassword.Text
	cfg.UseSSL = e.tlsEnabled.Checked
	cfg.SSLMode = e.tlsMode.Selected
	cfg.SSLCAPath = e.ca.Text
	cfg.SSLCertPath = e.cert.Text
	cfg.SSLKeyPath = e.key.Text
	if err = e.collectExtended(&p, &cfg); err != nil {
		return p, err
	}
	if err = preferConnectionURI(&cfg); err != nil {
		return p, err
	}
	p.Name = e.name.Text
	p.Group = e.group.Text
	p.Environment = e.environment.Selected
	p.ReadOnly = e.readonly.Checked
	cfg.ReadOnly = p.ReadOnly
	p.Config = cfg
	if strings.TrimSpace(p.Name) == "" {
		return p, errors.New("请填写连接名称")
	}
	return p, nil
}

func preferConnectionURI(cfg *connection.ConnectionConfig) error {
	if strings.TrimSpace(cfg.URI) == "" {
		return nil
	}
	if cfg.Type == "sqlite" || cfg.Type == "duckdb" || cfg.Type == "custom" {
		return errors.New("本地数据库请使用文件路径；自定义驱动请使用 DSN 字段")
	}
	// Upstream native clients treat basic fields as explicit overrides. Clearing
	// them here makes the user-facing URI field authoritative, including a URI
	// port that differs from the type's default and embedded credentials.
	cfg.Host, cfg.User, cfg.Password, cfg.Database = "", "", "", ""
	cfg.Port = 0
	cfg.Hosts = nil
	return nil
}
func parsePort(text string) (int, error) {
	if strings.TrimSpace(text) == "" {
		return 0, nil
	}
	port, err := strconv.Atoi(text)
	if err != nil || port < 0 || port > 65535 {
		return 0, errors.New("端口应为 0–65535 的整数")
	}
	return port, nil
}
func overlayConfig(base connection.ConnectionConfig, text string) (connection.ConnectionConfig, error) {
	raw, _ := json.Marshal(base)
	var values, patch map[string]any
	if err := json.Unmarshal(raw, &values); err != nil {
		return base, err
	}
	if err := json.Unmarshal([]byte(text), &patch); err != nil {
		return base, err
	}
	if patch == nil {
		return base, errors.New("configuration must be a JSON object")
	}
	mergeConfig(values, patch)
	raw, err := json.Marshal(values)
	if err != nil {
		return base, err
	}
	var cfg connection.ConnectionConfig
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	err = decoder.Decode(&cfg)
	return cfg, err
}
func mergeConfig(base, patch map[string]any) {
	for key, value := range patch {
		if object, ok := value.(map[string]any); ok {
			target, ok := base[key].(map[string]any)
			if !ok {
				target = map[string]any{}
				base[key] = target
			}
			mergeConfig(target, object)
		} else {
			base[key] = value
		}
	}
}
func (e *connectionEditor) save() {
	if e.saving {
		return
	}
	p, err := e.collect()
	if err != nil {
		e.owner.showError(err)
		return
	}
	e.hint.SetText("正在保存…")
	e.saving = true
	connect := e.connectAfterSave.Checked
	e.owner.cancelProfile(p.ID)
	e.owner.jobs.run(func(ctx context.Context) (any, error) {
		saved, err := e.owner.Profiles.Save(ctx, p)
		if err != nil {
			return nil, err
		}
		ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
		defer cancel()
		_ = e.owner.Engine.Disconnect(ctx, saved.ID)
		return saved, nil
	}, func(value any, err error) {
		e.saving = false
		if err != nil {
			e.hint.SetText(err.Error())
			return
		}
		saved := value.(domain.Profile)
		e.closed = true
		e.modal.Hide()
		e.owner.reload()
		e.owner.status.SetText("已保存：" + saved.Name)
		for _, space := range e.owner.workspaces {
			if space.profile.ID == saved.ID {
				space.profile = saved
				space.descriptor, _ = domain.Resolve(saved.Config.Type)
				space.item.Text = saved.Name
				space.item.Content = space.content()
				if !space.busy {
					space.finish()
				}
			}
		}
		e.owner.tabs.Refresh()
		for _, t := range e.owner.tables {
			if t.profile.ID == saved.ID {
				t.profile = saved
				if t.dirty() {
					t.status.SetText("连接配置已修改，请丢弃旧数据修改并重新加载。")
				} else {
					t.refresh()
				}
			}
		}
		for _, d := range e.owner.designers {
			if d.profile.ID == saved.ID {
				d.profile = saved
				d.status.SetText("连接配置已修改，请刷新结构后再保存。")
			}
		}
		if connect {
			e.owner.selected = saved.ID
			e.owner.jobs.run(func(ctx context.Context) (any, error) { return e.owner.Profiles.List(ctx) }, func(value any, err error) {
				if err != nil {
					return
				}
				e.owner.profiles = value.([]domain.Profile)
				e.owner.filter()
				e.owner.sidebar.tree.OpenBranch("connection:" + saved.ID)
			})
		}
	})
}
func protocolHint(d domain.Descriptor) string {
	hint := d.Name + " · "
	if d.Agent {
		hint += "需要对应的独立驱动 Agent。"
	}
	switch d.Family {
	case domain.SQL:
		hint += "可查询库表。SQLite / DuckDB 的主机栏填写文件路径或 :memory:。"
	case domain.Cache:
		hint += "RedisDB、集群和 Sentinel 可在高级配置中设置。"
	case domain.Message:
		hint += "消息读取会限量。RabbitMQ 使用 Management HTTP 端口；ACK 行为会显示在工作台。"
	default:
		hint += "协议特有参数可在高级配置中填写。"
	}
	return hint
}
