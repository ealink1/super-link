package application

import (
	"context"
	"errors"
	"strings"
	"sync"
	"time"

	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/infra/secrets"
	"github.com/ealink1/super-link/internal/infra/state"
	"github.com/google/uuid"
)

type Profiles struct {
	mu    sync.Mutex
	Store *state.Store
	Vault secrets.Vault
}

func (p *Profiles) List(ctx context.Context) ([]domain.Profile, error) { return p.Store.Profiles(ctx) }
func (p *Profiles) Get(ctx context.Context, id string) (domain.Profile, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.get(ctx, id)
}
func (p *Profiles) get(ctx context.Context, id string) (domain.Profile, error) {
	profile, err := p.Store.Profile(ctx, id)
	if err != nil {
		return profile, err
	}
	data, err := p.Vault.Get(profile.SecretRef)
	if err != nil {
		return profile, err
	}
	profile.Config, err = secrets.Join(profile.Config, data)
	return profile, err
}
func (p *Profiles) Save(ctx context.Context, profile domain.Profile) (domain.Profile, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return profile, err
	}
	if err := profile.ValidatePresentation(); err != nil {
		return profile, err
	}
	descriptor, err := domain.Resolve(profile.Config.Type)
	if err != nil {
		return profile, err
	}
	profile.Config.Type = descriptor.Key
	profile.Name = strings.TrimSpace(profile.Name)
	if profile.Name == "" {
		return profile, errors.New("connection name is required")
	}
	if profile.Config.Timeout <= 0 {
		profile.Config.Timeout = 15
	}
	if profile.Config.Timeout > 120 {
		return profile, errors.New("connection timeout must be at most 120 seconds")
	}
	if profile.Config.QueryTimeout < 0 || profile.Config.QueryTimeout > 3600 {
		return profile, errors.New("query timeout must be between 0 and 3600 seconds")
	}
	if profile.ID == "" {
		profile.ID = uuid.NewString()
	}
	profile.Config.ID = profile.ID
	profile.Config.ReadOnly = profile.ReadOnly
	var old domain.Profile
	if profile.Revision > 0 {
		old, err = p.Store.Profile(ctx, profile.ID)
		if err != nil {
			return profile, err
		}
		if old.Revision != profile.Revision {
			return profile, domain.ErrConflict
		}
	}
	if profile.Revision == 0 {
		if strings.TrimSpace(profile.Group) == "" {
			options, err := p.groupOptions(ctx)
			if err != nil {
				return profile, err
			}
			profile.Group = defaultConnectionGroup(options)
		}
		profile.CreatedAt = time.Now().UTC()
	} else {
		profile.CreatedAt = old.CreatedAt
	}
	public, bundle, err := secrets.Split(profile.Config)
	if err != nil {
		return profile, err
	}
	ref, err := p.Vault.Put(bundle, profile.Config.SavePassword)
	if err != nil {
		return profile, err
	}
	saved := profile
	saved.Config = public
	saved.SecretRef = ref
	saved, err = p.Store.SaveProfile(ctx, saved)
	if err != nil {
		_ = p.Vault.Delete(ref)
		return profile, err
	}
	// Old data stays recoverable until the metadata transaction commits.
	if old.SecretRef != "" {
		_ = p.Vault.Delete(old.SecretRef)
	}
	saved.Config = profile.Config
	return saved, nil
}
func (p *Profiles) Delete(ctx context.Context, id string, revision int64) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	old, err := p.Store.Profile(ctx, id)
	if err != nil {
		return err
	}
	if err = p.Store.DeleteProfile(ctx, id, revision); err != nil {
		return err
	}
	return p.Vault.Delete(old.SecretRef)
}
