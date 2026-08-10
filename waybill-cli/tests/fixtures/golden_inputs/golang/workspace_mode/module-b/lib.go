// Fixture for milestone 231 (workspace-mode preflight regression).
// Package name uses the synthetic `mikebomfixture` prefix per memory
// `feedback_fixture_synthetic_package_names` — no real coordinates.
package b

import "example.com/mikebomfixture/shared"

// Helper wraps the shared library so the shared module is a Direct
// production dep of module-b.
func Helper() string {
	return shared.Version()
}
