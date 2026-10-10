package domain

// ConnectionGroupOptions contains public presentation and routing preferences.
type ConnectionGroupOptions struct {
	Color       string `json:"color,omitempty"`
	Description string `json:"description,omitempty"`
	Default     bool   `json:"default,omitempty"`
}
