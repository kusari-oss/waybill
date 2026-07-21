// Package baz is the milestone-116 fixture's primary binary.
//go:build linux || darwin

package main

import "fmt"

func main() {
	fmt.Println("baz")
}
