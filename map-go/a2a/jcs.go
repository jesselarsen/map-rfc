// Package a2a is the MAP-over-A2A v1.0 binding (spec/binding-a2a.md),
// mirroring Rust map-a2a: canonical JSON, Agent Card JWS signatures, v2
// descriptors embedding the card, TaskID derivation, the Part→transcript
// leaf rule, and the provider/requester accountability flows. HTTP transport
// is in server.go and client.go. Stdlib only.
//
// JSON values use the encoding/json `any` model decoded with UseNumber:
// map[string]any, []any, string, json.Number, bool, nil.
package a2a

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"sort"
	"strconv"
	"strings"
	"unicode/utf16"
	"unicode/utf8"
)

// ErrNonFinite: NaN or ±Infinity cannot be represented in JSON.
var ErrNonFinite = errors.New("jcs: non-finite number")

// ParseJSON decodes one JSON document into the `any` model, keeping
// numbers as json.Number and rejecting trailing data.
func ParseJSON(b []byte) (any, error) {
	dec := json.NewDecoder(bytes.NewReader(b))
	dec.UseNumber()
	var v any
	if err := dec.Decode(&v); err != nil {
		return nil, err
	}
	if dec.More() {
		return nil, errors.New("json: trailing data")
	}
	return v, nil
}

// Canonicalize returns the RFC 8785 (JCS) bytes of v:
//   - object members sorted by the UTF-16 code units of their names;
//   - strings escaped as ECMAScript JSON.stringify does (only `"`, `\` and
//     U+0000..U+001F; \b \t \n \f \r short forms, other controls \u00xx);
//   - numbers as ECMAScript Number.prototype.toString of the IEEE-754 double;
//   - no insignificant whitespace.
func Canonicalize(v any) ([]byte, error) {
	var b strings.Builder
	if err := writeValue(&b, v); err != nil {
		return nil, err
	}
	return []byte(b.String()), nil
}

func writeValue(b *strings.Builder, v any) error {
	switch x := v.(type) {
	case nil:
		b.WriteString("null")
	case bool:
		if x {
			b.WriteString("true")
		} else {
			b.WriteString("false")
		}
	case json.Number:
		f, err := strconv.ParseFloat(string(x), 64)
		if err != nil {
			return fmt.Errorf("jcs: number %q: %w", x, err)
		}
		s, err := ES6Number(f)
		if err != nil {
			return err
		}
		b.WriteString(s)
	case float64:
		s, err := ES6Number(x)
		if err != nil {
			return err
		}
		b.WriteString(s)
	case int:
		b.WriteString(strconv.Itoa(x)) // exact for the small ints builders use
	case uint64:
		s, _ := ES6Number(float64(x))
		b.WriteString(s)
	case string:
		return writeString(b, x)
	case []any:
		b.WriteByte('[')
		for i, e := range x {
			if i > 0 {
				b.WriteByte(',')
			}
			if err := writeValue(b, e); err != nil {
				return err
			}
		}
		b.WriteByte(']')
	case []string:
		arr := make([]any, len(x))
		for i, s := range x {
			arr[i] = s
		}
		return writeValue(b, arr)
	case map[string]any:
		keys := make([]string, 0, len(x))
		for k := range x {
			keys = append(keys, k)
		}
		sort.Slice(keys, func(i, j int) bool { return utf16Less(keys[i], keys[j]) })
		b.WriteByte('{')
		for i, k := range keys {
			if i > 0 {
				b.WriteByte(',')
			}
			if err := writeString(b, k); err != nil {
				return err
			}
			b.WriteByte(':')
			if err := writeValue(b, x[k]); err != nil {
				return err
			}
		}
		b.WriteByte('}')
	default:
		return fmt.Errorf("jcs: unsupported value type %T", v)
	}
	return nil
}

func utf16Less(a, b string) bool {
	ua, ub := utf16.Encode([]rune(a)), utf16.Encode([]rune(b))
	for i := 0; i < len(ua) && i < len(ub); i++ {
		if ua[i] != ub[i] {
			return ua[i] < ub[i]
		}
	}
	return len(ua) < len(ub)
}

func writeString(b *strings.Builder, s string) error {
	if !utf8.ValidString(s) {
		return errors.New("jcs: invalid UTF-8 in string")
	}
	b.WriteByte('"')
	for _, r := range s {
		switch r {
		case '"':
			b.WriteString(`\"`)
		case '\\':
			b.WriteString(`\\`)
		case '\b':
			b.WriteString(`\b`)
		case '\t':
			b.WriteString(`\t`)
		case '\n':
			b.WriteString(`\n`)
		case '\f':
			b.WriteString(`\f`)
		case '\r':
			b.WriteString(`\r`)
		default:
			if r < 0x20 {
				fmt.Fprintf(b, `\u%04x`, r)
			} else {
				b.WriteRune(r)
			}
		}
	}
	b.WriteByte('"')
	return nil
}

// ES6Number is ECMAScript Number::toString(x) for a finite double
// (ECMA-262 §6.1.6.1.20).
func ES6Number(x float64) (string, error) {
	if math.IsNaN(x) || math.IsInf(x, 0) {
		return "", ErrNonFinite
	}
	if x == 0 {
		return "0", nil // also -0
	}
	neg := x < 0
	// Shortest round-trip digits in "d.dddde±XX" form.
	sci := strconv.FormatFloat(math.Abs(x), 'e', -1, 64)
	mant, exp, _ := strings.Cut(sci, "e")
	digits := strings.ReplaceAll(mant, ".", "")
	e, err := strconv.Atoi(exp)
	if err != nil {
		return "", err
	}
	k := len(digits)
	n := e + 1 // value = 0.d1..dk × 10^n
	var body string
	switch {
	case k <= n && n <= 21:
		body = digits + strings.Repeat("0", n-k)
	case 0 < n && n <= 21:
		body = digits[:n] + "." + digits[n:]
	case -6 < n && n <= 0:
		body = "0." + strings.Repeat("0", -n) + digits
	default:
		sign := "+"
		if n-1 < 0 {
			sign = "-"
		}
		ex := n - 1
		if ex < 0 {
			ex = -ex
		}
		if k == 1 {
			body = fmt.Sprintf("%se%s%d", digits, sign, ex)
		} else {
			body = fmt.Sprintf("%s.%se%s%d", digits[:1], digits[1:], sign, ex)
		}
	}
	if neg {
		body = "-" + body
	}
	return body, nil
}
