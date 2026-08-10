// Fixture for milestone 231 (workspace-mode preflight regression).
// Synthetic module name per memory `feedback_fixture_synthetic_package_names`.
package shared

// Version returns a fixed version string. Present so module-a and
// module-b can import this package and thereby produce a real
// production import edge for the `go mod why` classifier to analyze.
func Version() string {
	return "mikebomfixture-shared-v1.0.0"
}
