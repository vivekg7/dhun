package library

import (
	"os"
	"strings"
	"testing"
)

// TestFoldsAsTheAppDoes reads the cases the Android and Mac apps' tests read,
// so the three copies of the folding cannot drift apart.
func TestFoldsAsTheAppDoes(t *testing.T) {
	data, err := os.ReadFile("../../../api/search-fold.tsv")
	if err != nil {
		t.Fatal(err)
	}
	for _, line := range strings.Split(string(data), "\n") {
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		c := strings.Split(line, "\t")
		words := FoldWords(c[0])
		keys := make([]string, len(words))
		for i, w := range words {
			keys[i] = FoldKey(w)
		}
		if got := strings.Join(words, " "); got != c[1] {
			t.Errorf("%s: words %q, want %q", c[0], got, c[1])
		}
		if got := strings.Join(keys, " "); got != c[2] {
			t.Errorf("%s: keys %q, want %q", c[0], got, c[2])
		}
	}
}
