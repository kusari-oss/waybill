// Fixture for milestone 231 (workspace-mode preflight regression).
// Package name uses the synthetic `mikebomfixture` prefix per memory
// `feedback_fixture_synthetic_package_names` — no real coordinates.
package main

import "example.com/mikebomfixture/shared"

func main() {
	_ = shared.Version()
}
