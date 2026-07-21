// Package fixturelib is a library — NOT a binary. Should NOT trigger
// produces-binaries emission.
package fixturelib

const Version = "1.0.0"

func Greet() string {
	return "fixture-libonly"
}
