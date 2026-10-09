package application

import (
	"context"
	"errors"
	"strings"
	"testing"
)

func TestSQLFileBatchMergesLiteralsAndPreservesBoundaries(t *testing.T) {
	insert := "INSERT INTO `db`.`items` (`id`, `text`) VALUES (1, CONVERT(X'78' USING utf8mb4));"
	input := strings.Repeat(insert, 300) + "SET @x=1;" + insert + "INSERT INTO `db`.`items` (`id`, `text`) VALUES (2, NOW());"
	var counts []int64
	err := executeSQLFileStatements(context.Background(), strings.NewReader(input), "mysql", func(text string, offset, count int64) error {
		counts = append(counts, count)
		if count > 1 && strings.Count(text, "INSERT INTO") != 1 {
			t.Fatal(text)
		}
		return nil
	})
	if err != nil || len(counts) != 6 || counts[0] != 128 || counts[1] != 128 || counts[2] != 44 || counts[3] != 1 || counts[4] != 1 || counts[5] != 1 {
		t.Fatal(counts, err)
	}
}
func TestSQLFileBatchStopsOnFailureWithoutRetry(t *testing.T) {
	failure := errors.New("failed")
	calls := 0
	err := executeSQLFileStatements(context.Background(), strings.NewReader(strings.Repeat("INSERT INTO `items` (`id`) VALUES (1);", 300)), "mysql", func(string, int64, int64) error { calls++; return failure })
	if !errors.Is(err, failure) || calls != 1 {
		t.Fatal(calls, err)
	}
}
func TestSQLFileBatchOtherDialectsRemainSequential(t *testing.T) {
	calls := 0
	err := executeSQLFileStatements(context.Background(), strings.NewReader(strings.Repeat("INSERT INTO `items` (`id`) VALUES (1);", 3)), "sqlite", func(_ string, _, count int64) error {
		calls++
		if count != 1 {
			t.Fatal(count)
		}
		return nil
	})
	if err != nil || calls != 3 {
		t.Fatal(calls, err)
	}
}
