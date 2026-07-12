package conformance

import (
	"bufio"
	"encoding/hex"
	"fmt"
	"os"
	"strings"
)

// Vector-file reader. The single source of truth is
// map-spec/test-vectors/map-test-vectors.md (TASK-015 §FR-3); this parser
// collects, per "## …" / "### …" section, every line that is entirely hex
// (the file indents each blob by two spaces on one line). The last blob of
// a TV/NV section is always the complete envelope/receipt.

// VectorFile holds hex blobs (decoded) grouped by section title.
type VectorFile struct {
	Sections map[string][][]byte
	Order    []string
}

// LoadVectors parses the markdown vector file at path.
func LoadVectors(path string) (*VectorFile, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()

	vf := &VectorFile{Sections: map[string][][]byte{}}
	section := ""
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 0, 64*1024), 64*1024)
	for sc.Scan() {
		line := sc.Text()
		if strings.HasPrefix(line, "## ") || strings.HasPrefix(line, "### ") {
			// Section id: first token after the #s (e.g. "TV-001", "NV-007",
			// "Derived", "Keys", "Negative", "Verification").
			title := strings.TrimLeft(line, "# ")
			section = strings.Fields(title)[0]
			if _, seen := vf.Sections[section]; !seen {
				vf.Order = append(vf.Order, section)
			}
			continue
		}
		t := strings.TrimSpace(line)
		if len(t) >= 32 && len(t)%2 == 0 && isHex(t) {
			b, err := hex.DecodeString(t)
			if err != nil {
				return nil, err
			}
			vf.Sections[section] = append(vf.Sections[section], b)
		}
	}
	if err := sc.Err(); err != nil {
		return nil, err
	}
	return vf, nil
}

func isHex(s string) bool {
	for _, c := range s {
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return false
		}
	}
	return true
}

// Envelope returns the last blob of a section — the complete envelope
// (or receipt) bytes.
func (vf *VectorFile) Envelope(section string) ([]byte, error) {
	blobs := vf.Sections[section]
	if len(blobs) == 0 {
		return nil, fmt.Errorf("vectors: no hex blobs in section %q", section)
	}
	return blobs[len(blobs)-1], nil
}

// firstDiff returns the offset of the first differing byte, or -1 if equal.
func firstDiff(a, b []byte) int {
	n := len(a)
	if len(b) < n {
		n = len(b)
	}
	for i := 0; i < n; i++ {
		if a[i] != b[i] {
			return i
		}
	}
	if len(a) != len(b) {
		return n
	}
	return -1
}
