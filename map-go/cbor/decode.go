package cbor

import (
	"bytes"
	"errors"
	"fmt"
	"unicode/utf8"
)

// Strict decoder: accepts ONLY the deterministic profile of
// draft-map-core-00 §4.2 and rejects every deviation (the malleability
// defense; the NV-003 class of vectors is its acceptance test).
// Rejected deviations: non-minimal integer/length heads, indefinite
// lengths, unsorted or duplicate map keys (bytewise on encoded form),
// floats and simple values other than booleans, invalid UTF-8 in text,
// reserved additional-info values, trailing bytes, and excessive nesting.

// ErrNonCanonical wraps all deterministic-profile violations
// (core §9 class 1000, MALFORMED).
var ErrNonCanonical = errors.New("cbor: non-canonical or malformed")

const maxDepth = 64

// Decode strictly decodes exactly one data item covering all of data.
func Decode(data []byte) (Value, error) {
	v, n, err := decodeItem(data, 0)
	if err != nil {
		return nil, err
	}
	if n != len(data) {
		return nil, fmt.Errorf("%w: %d trailing bytes", ErrNonCanonical, len(data)-n)
	}
	return v, nil
}

// decodeHead reads a head at data[pos:], enforcing minimal-length encoding.
// Returns major type, argument, and bytes consumed.
func decodeHead(data []byte, pos int) (byte, uint64, int, error) {
	if pos >= len(data) {
		return 0, 0, 0, fmt.Errorf("%w: truncated head", ErrNonCanonical)
	}
	ib := data[pos]
	major, ai := ib>>5, ib&0x1f
	switch {
	case ai < 24:
		return major, uint64(ai), 1, nil
	case ai == 24, ai == 25, ai == 26, ai == 27:
		n := 1 << (ai - 24) // 1, 2, 4, 8 argument bytes
		if pos+1+n > len(data) {
			return 0, 0, 0, fmt.Errorf("%w: truncated head argument", ErrNonCanonical)
		}
		var arg uint64
		for _, b := range data[pos+1 : pos+1+n] {
			arg = arg<<8 | uint64(b)
		}
		// Minimal-length rule (RFC 8949 §4.2.1).
		var min uint64
		switch ai {
		case 24:
			min = 24
		case 25:
			min = 0x100
		case 26:
			min = 0x10000
		case 27:
			min = 0x100000000
		}
		if arg < min {
			return 0, 0, 0, fmt.Errorf("%w: non-minimal head (ai=%d, arg=%d)", ErrNonCanonical, ai, arg)
		}
		return major, arg, 1 + n, nil
	case ai == 31:
		return 0, 0, 0, fmt.Errorf("%w: indefinite length (ai=31)", ErrNonCanonical)
	default: // 28-30 reserved
		return 0, 0, 0, fmt.Errorf("%w: reserved additional info %d", ErrNonCanonical, ai)
	}
}

func decodeItem(data []byte, depth int) (Value, int, error) {
	if depth > maxDepth {
		return nil, 0, fmt.Errorf("%w: nesting too deep", ErrNonCanonical)
	}
	major, arg, n, err := decodeHead(data, 0)
	if err != nil {
		return nil, 0, err
	}
	switch major {
	case 0:
		return Uint(arg), n, nil
	case 1:
		if arg > 1<<62 { // -1-arg must fit Int; MAP never uses larger
			return nil, 0, fmt.Errorf("%w: negative integer out of modeled range", ErrNonCanonical)
		}
		return Int(-1 - int64(arg)), n, nil
	case 2, 3:
		if arg > uint64(len(data)-n) {
			return nil, 0, fmt.Errorf("%w: truncated string", ErrNonCanonical)
		}
		s := data[n : n+int(arg)]
		if major == 2 {
			return Bytes(append([]byte(nil), s...)), n + int(arg), nil
		}
		if !utf8.Valid(s) {
			return nil, 0, fmt.Errorf("%w: invalid UTF-8 in text string", ErrNonCanonical)
		}
		return Text(s), n + int(arg), nil
	case 4:
		// Each element takes at least one byte: a count exceeding the
		// remaining input is truncated (and guards the allocation below).
		if arg > uint64(len(data)-n) {
			return nil, 0, fmt.Errorf("%w: array count exceeds input", ErrNonCanonical)
		}
		arr := make(Array, 0, min(int(arg), 1024))
		pos := n
		for i := uint64(0); i < arg; i++ {
			v, m, err := decodeItem(data[pos:], depth+1)
			if err != nil {
				return nil, 0, err
			}
			arr = append(arr, v)
			pos += m
		}
		return arr, pos, nil
	case 5:
		// Each entry takes at least two bytes (key + value).
		if arg > uint64(len(data)-n)/2 {
			return nil, 0, fmt.Errorf("%w: map count exceeds input", ErrNonCanonical)
		}
		mp := make(Map, 0, min(int(arg), 1024))
		pos := n
		var prevKey []byte
		for i := uint64(0); i < arg; i++ {
			kStart := pos
			k, m, err := decodeItem(data[pos:], depth+1)
			if err != nil {
				return nil, 0, err
			}
			pos += m
			// Keys must be strictly increasing bytewise on their encoded
			// form (sorted + no duplicates, RFC 8949 §4.2.1). Since every
			// accepted item is canonical, its raw bytes ARE its encoding.
			kEnc := data[kStart:pos]
			if prevKey != nil && bytes.Compare(prevKey, kEnc) >= 0 {
				return nil, 0, fmt.Errorf("%w: map keys not strictly sorted", ErrNonCanonical)
			}
			prevKey = append([]byte(nil), kEnc...)
			v, m, err := decodeItem(data[pos:], depth+1)
			if err != nil {
				return nil, 0, err
			}
			pos += m
			mp = append(mp, Pair{k, v})
		}
		return mp, pos, nil
	case 6:
		v, m, err := decodeItem(data[n:], depth+1)
		if err != nil {
			return nil, 0, err
		}
		return Tag{Num: arg, Content: v}, n + m, nil
	default: // major 7
		if n == 1 {
			switch data[0] {
			case 0xf4:
				return Bool(false), 1, nil
			case 0xf5:
				return Bool(true), 1, nil
			}
		}
		return nil, 0, fmt.Errorf("%w: simple/float value outside MAP profile (0x%02x)", ErrNonCanonical, data[0])
	}
}
