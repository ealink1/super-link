package application

import (
	"context"
	"errors"
	"testing"

	"github.com/ealink1/super-link/internal/domain"
	adapter "github.com/ealink1/super-link/internal/infra/runtime"
)

type catalogFixture struct {
	fakeClient
	fail bool
}

func (*catalogFixture) Objects(context.Context, string) ([]domain.Object, error) {
	return []domain.Object{{Name: "items", Kind: "table"}}, nil
}
func (f *catalogFixture) Execute(context.Context, domain.Execution) ([]domain.Result, error) {
	if f.fail {
		return nil, errors.New("catalog unavailable")
	}
	return []domain.Result{{Rows: [][]any{{[]byte("items"), int64(589), int64(81920), "InnoDB", nil, nil, "utf8mb4_unicode_ci", "comment"}}}}, nil
}
func TestTableCatalogLoadsStatisticsAndRetainsTablesOnFailure(t *testing.T) {
	for _, fail := range []bool{false, true} {
		profiles := testProfiles(t)
		profile := saveProfile(t, profiles, "mysql", true)
		engine := NewEngine(profiles)
		fixture := &catalogFixture{fail: fail}
		engine.Factory = func(context.Context, domain.Profile) (adapter.Client, error) { return fixture, nil }
		catalog, err := engine.TableCatalog(context.Background(), profile, "main")
		_ = engine.Close()
		if err != nil || len(catalog.Objects) != 1 {
			t.Fatal("lost table list", err)
		}
		if fail {
			if catalog.StatisticsError == nil {
				t.Fatal("missing statistics error")
			}
		} else if len(catalog.Statistics["items"]) != 7 {
			t.Fatalf("statistics: %+v, error: %v", catalog.Statistics, catalog.StatisticsError)
		}
	}
}
