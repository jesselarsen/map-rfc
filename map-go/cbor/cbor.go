// Package cbor implements exactly the CBOR profile MAP signs and hashes:
// RFC 8949 §4.2.1 core deterministic encoding (draft-map-core-01 §4.2).
// It is hand-rolled on purpose (TASK-015 §FR-1): the encoding profile IS
// the thing under test, so no general-purpose CBOR library is used.
//
// Supported data model (all MAP normative structures fit in it):
// unsigned/negative integers, byte strings, text strings, booleans,
// arrays, maps, and tags. Floats, null, undefined, other simple values,
// and indefinite lengths are outside the profile: the encoder cannot
// produce them and the strict decoder rejects them (core §4.2:
// "Floating-point values MUST NOT appear in signed structures").
package cbor

import (
	"bytes"
	"fmt"
	"sort"
)

// Value is a CBOR data item in the MAP profile.
type Value interface{ isValue() }

// Uint is a major-type-0 unsigned integer.
type Uint uint64

// Int is an integer that may be negative; negatives use major type 1.
type Int int64

// Bytes is a definite-length byte string (major type 2).
type Bytes []byte

// Text is a definite-length UTF-8 text string (major type 3).
type Text string

// Array is a definite-length array (major type 4).
type Array []Value

// Pair is one map entry.
type Pair struct {
	K, V Value
}

// Map is a definite-length map (major type 5). The encoder sorts entries
// bytewise on the encoded key (RFC 8949 §4.2.1) regardless of input order.
type Map []Pair

// Tag is a tagged value (major type 6), e.g. tag 18 COSE_Sign1,
// tag 4 decimal fraction.
type Tag struct {
	Num     uint64
	Content Value
}

// Bool is a major-type-7 boolean (0xf4/0xf5) — the only simple values in
// the MAP profile.
type Bool bool

func (Uint) isValue()  {}
func (Int) isValue()   {}
func (Bytes) isValue() {}
func (Text) isValue()  {}
func (Array) isValue() {}
func (Map) isValue()   {}
func (Tag) isValue()   {}
func (Bool) isValue()  {}

// Encode returns the deterministic encoding of v.
func Encode(v Value) []byte {
	return appendValue(nil, v)
}

// appendHead writes a minimal-length head (RFC 8949 §4.2.1: "shortest form").
func appendHead(b []byte, major byte, arg uint64) []byte {
	m := major << 5
	switch {
	case arg < 24:
		return append(b, m|byte(arg))
	case arg <= 0xff:
		return append(b, m|24, byte(arg))
	case arg <= 0xffff:
		return append(b, m|25, byte(arg>>8), byte(arg))
	case arg <= 0xffffffff:
		return append(b, m|26, byte(arg>>24), byte(arg>>16), byte(arg>>8), byte(arg))
	default:
		return append(b, m|27, byte(arg>>56), byte(arg>>48), byte(arg>>40),
			byte(arg>>32), byte(arg>>24), byte(arg>>16), byte(arg>>8), byte(arg))
	}
}

func appendValue(b []byte, v Value) []byte {
	switch x := v.(type) {
	case Uint:
		return appendHead(b, 0, uint64(x))
	case Int:
		if x >= 0 {
			return appendHead(b, 0, uint64(x))
		}
		return appendHead(b, 1, uint64(-1-x))
	case Bytes:
		b = appendHead(b, 2, uint64(len(x)))
		return append(b, x...)
	case Text:
		b = appendHead(b, 3, uint64(len(x)))
		return append(b, x...)
	case Array:
		b = appendHead(b, 4, uint64(len(x)))
		for _, e := range x {
			b = appendValue(b, e)
		}
		return b
	case Map:
		b = appendHead(b, 5, uint64(len(x)))
		type kv struct{ k, v []byte }
		enc := make([]kv, len(x))
		for i, p := range x {
			enc[i] = kv{appendValue(nil, p.K), appendValue(nil, p.V)}
		}
		sort.Slice(enc, func(i, j int) bool {
			return bytes.Compare(enc[i].k, enc[j].k) < 0
		})
		for i := 1; i < len(enc); i++ {
			if bytes.Equal(enc[i].k, enc[i-1].k) {
				panic("cbor: duplicate map key")
			}
		}
		for _, e := range enc {
			b = append(b, e.k...)
			b = append(b, e.v...)
		}
		return b
	case Tag:
		b = appendHead(b, 6, x.Num)
		return appendValue(b, x.Content)
	case Bool:
		if x {
			return append(b, 0xf5)
		}
		return append(b, 0xf4)
	default:
		panic(fmt.Sprintf("cbor: unsupported value %T", v))
	}
}
