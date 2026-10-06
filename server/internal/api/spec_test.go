package api

import (
	"os"
	"regexp"
	"slices"
	"strings"
	"testing"
)

// TestSpecCoversEveryRoute keeps api/openapi.yaml, the contract the clients
// are written against, in step with the routes the server actually serves.
func TestSpecCoversEveryRoute(t *testing.T) {
	data, err := os.ReadFile("../../../api/openapi.yaml")
	if err != nil {
		t.Fatal(err)
	}
	// Paths are two-space-indented keys under `paths:`; methods are four.
	pathRe := regexp.MustCompile(`^  (/\S*):\s*$`)
	methodRe := regexp.MustCompile(`^    (get|post|put|patch|delete):\s*$`)
	var spec []string
	inPaths, current := false, ""
	for _, line := range strings.Split(string(data), "\n") {
		switch {
		case line == "paths:":
			inPaths = true
		case inPaths && line != "" && !strings.HasPrefix(line, " "):
			inPaths = false
		case inPaths && pathRe.MatchString(line):
			current = pathRe.FindStringSubmatch(line)[1]
		case inPaths && methodRe.MatchString(line):
			spec = append(spec, strings.ToUpper(methodRe.FindStringSubmatch(line)[1])+" "+current)
		}
	}

	var served []string
	for _, rt := range (&Server{}).routes() {
		served = append(served, rt.pattern)
	}
	slices.Sort(spec)
	slices.Sort(served)
	for _, r := range served {
		if !slices.Contains(spec, r) {
			t.Errorf("route %q is served but not in api/openapi.yaml", r)
		}
	}
	for _, r := range spec {
		if !slices.Contains(served, r) {
			t.Errorf("api/openapi.yaml documents %q, which the server does not serve", r)
		}
	}
}
