package db

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"
)

// TestOptionalDriverAgentAttachRequestCarriesSpec 验证代理附加请求的负载契约。
func TestOptionalDriverAgentAttachRequestCarriesSpec(t *testing.T) {
	var stdin optionalAgentTestWriteCloser
	client := &optionalDriverAgentClient{
		stdin:  &stdin,
		reader: bufio.NewReader(strings.NewReader(`{"id":1,"success":true}` + "\n")),
		driver: "duckdb",
	}

	spec := ExternalAttachSpec{
		Kind:       ExternalAttachKindMySQL,
		Host:       "10.0.0.1",
		Port:       3306,
		User:       "report",
		Password:   "secret",
		Database:   "orders",
		Alias:      "orders_db",
		ReadOnly:   true,
		SecretName: "superlink_attach_orders_db",
	}
	db := &OptionalDriverAgentDB{driverType: "duckdb", client: client}
	if err := db.AttachExternalDatabase(context.Background(), spec); err != nil {
		t.Fatalf("attach over agent = %v", err)
	}

	var request optionalAgentRequest
	if err := json.Unmarshal(stdin.Bytes(), &request); err != nil {
		t.Fatalf("request json = %s: %v", stdin.Bytes(), err)
	}
	if request.Method != optionalAgentMethodAttachExternalDatabase {
		t.Fatalf("method = %q", request.Method)
	}
	if request.AttachSpec == nil || request.AttachSpec.Host != "10.0.0.1" ||
		request.AttachSpec.Alias != "orders_db" || !request.AttachSpec.ReadOnly {
		t.Fatalf("attach spec = %+v", request.AttachSpec)
	}
}

// TestOptionalDriverAgentDetachRehydratesSentinel 验证“别名未附加”哨兵经代理后
// 仍满足 errors.Is（保存文件重跑的幂等语义依赖它）。
func TestOptionalDriverAgentDetachRehydratesSentinel(t *testing.T) {
	markResponse := func(flag bool) []byte {
		payload, err := json.Marshal(optionalAgentResponse{
			ID:                        1,
			Success:                   false,
			Error:                     ErrExternalAttachNotAttached.Error(),
			ExternalAttachNotAttached: flag,
		})
		if err != nil {
			t.Fatalf("marshal response: %v", err)
		}
		return append(payload, '\n')
	}

	cases := []struct {
		name         string
		response     []byte
		wantSentinel bool
	}{
		{name: "marked response rehydrates sentinel", response: markResponse(true), wantSentinel: true},
		{name: "legacy plain text response also rehydrates", response: []byte(`{"id":1,"success":false,"error":"external attach: alias not attached"}` + "\n"), wantSentinel: true},
		{name: "unrelated error stays untouched", response: []byte(`{"id":1,"success":false,"error":"engine boom"}` + "\n"), wantSentinel: false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			var stdin optionalAgentTestWriteCloser
			client := &optionalDriverAgentClient{
				stdin:  &stdin,
				reader: bufio.NewReader(bytes.NewReader(tc.response)),
				driver: "duckdb",
			}
			db := &OptionalDriverAgentDB{driverType: "duckdb", client: client}
			err := db.DetachExternalDatabase(context.Background(), "orders_db")
			if tc.wantSentinel && !errors.Is(err, ErrExternalAttachNotAttached) {
				t.Fatalf("err = %v, want sentinel", err)
			}
			if !tc.wantSentinel && errors.Is(err, ErrExternalAttachNotAttached) {
				t.Fatalf("err = %v, sentinel must not leak into unrelated errors", err)
			}
		})
	}
}

func TestOptionalDriverAgentListAttachmentsDecodesPayload(t *testing.T) {
	response, err := json.Marshal(map[string]any{
		"id":      1,
		"success": true,
		"data": []map[string]any{
			{"alias": "target", "connectionId": "conn-1", "kind": "duckdb", "readOnly": true},
		},
	})
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var stdin optionalAgentTestWriteCloser
	client := &optionalDriverAgentClient{
		stdin:  &stdin,
		reader: bufio.NewReader(bytes.NewReader(append(response, '\n'))),
		driver: "duckdb",
	}
	db := &OptionalDriverAgentDB{driverType: "duckdb", client: client}
	attachments, err := db.ListExternalAttachments(context.Background())
	if err != nil {
		t.Fatalf("list over agent = %v", err)
	}
	if len(attachments) != 1 || attachments[0].Alias != "target" ||
		attachments[0].ConnectionID != "conn-1" || !attachments[0].ReadOnly {
		t.Fatalf("attachments = %+v", attachments)
	}
}
