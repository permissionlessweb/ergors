//go:build tools
// +build tools

// This file exists solely to cause `go mod vendor` to download
// proto dependencies (k8s.io/apimachinery) into the vendor directory.
// The proto files are then accessible for protobuf compilation.
package proto

import (
	_ "k8s.io/apimachinery/pkg/api/resource"
)
