package databaseexport

import (
	"github.com/ealink1/super-link/internal/domain"
	"time"
)

type Progress struct {
	Phase, Table               string
	Total, CompletedTables     int
	TotalSteps, CompletedSteps int
	Rows, TableRows, Bytes     int64
	Done                       bool
}

// Fraction measures completed table operations; publishing is required for 100%.
func (p Progress) Fraction() float64 {
	if p.Done {
		return 1
	}
	if p.TotalSteps == 0 {
		return 0
	}
	return min(0.99, float64(p.CompletedSteps)/float64(p.TotalSteps))
}

type progressConsumer struct {
	domain.RowConsumer
	update, report func()
	last           time.Time
}

func (c *progressConsumer) ConsumeRowValues(row []any) error {
	if err := c.RowConsumer.ConsumeRowValues(row); err != nil {
		return err
	}
	c.update()
	if time.Since(c.last) >= 100*time.Millisecond {
		c.last = time.Now()
		c.report()
	}
	return nil
}
