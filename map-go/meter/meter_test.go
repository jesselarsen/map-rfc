package meter

import (
	"bytes"
	"encoding/hex"
	"errors"
	"testing"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/core"
)

func TestAmountDisplayAndRoundtrip(t *testing.T) {
	cases := []struct {
		a Amount
		s string
	}{
		{Amount{-2, 1995}, "19.95"}, {Amount{-2, 5}, "0.05"}, {Amount{0, 42}, "42"},
		{Amount{3, 5}, "5000"}, {Amount{-4, 12}, "0.0012"},
	}
	for _, c := range cases {
		if c.a.String() != c.s {
			t.Errorf("%v renders %q, want %q", c.a, c.a.String(), c.s)
		}
		v, err := cbor.Decode(cbor.Encode(c.a.ToValue()))
		if err != nil {
			t.Fatal(err)
		}
		if got, err := AmountFromValue(v); err != nil || got != c.a {
			t.Errorf("roundtrip %v: %v %v", c.a, got, err)
		}
	}
}

// M-4 / R4-01: a negative mantissa on the wire is rejected.
func TestNegativeAmountIsRejected(t *testing.T) {
	neg := cbor.Tag{Num: 4, Content: cbor.Array{cbor.Int(-2), cbor.Int(-5)}}
	if _, err := AmountFromValue(neg); !errors.Is(err, ErrBadAmount) {
		t.Fatalf("negative mantissa accepted: %v", err)
	}
}

func TestAssetsValidated(t *testing.T) {
	for s, ok := range map[string]bool{"iso4217:USD": true, "credit:example-mesh": true, "USD": false, "erc20:": false} {
		if _, err := NewAsset(s); (err == nil) != ok {
			t.Errorf("%q: err %v", s, err)
		}
	}
}

func micro(m uint64) AssetAmount { return AssetAmount{"iso4217:USD", Amount{-6, m}} }

var usage = core.Usage{TokensIn: 1523, TokensOut: 812, TTFTMs: 431, WallMs: 42000}

func TestBilledModels(t *testing.T) {
	rates := []Rate{{UnitTokenIn, micro(3000)}, {UnitTokenOut, micro(5000)}}
	// 1523×3000 + 812×5000 = 8_629_000 (the v1 TV-016 figure).
	if got, err := Billed(1, rates, usage); err != nil || got != micro(8_629_000) {
		t.Fatalf("model 1: %v %v", got, err)
	}
	if got, _ := Billed(2, []Rate{{UnitMs, micro(2)}}, usage); got != micro(84_000) {
		t.Fatalf("model 2: %v", got)
	}
	if got, _ := Billed(3, []Rate{{UnitInvocation, micro(500_000)}}, usage); got != micro(500_000) {
		t.Fatalf("model 3: %v", got)
	}
	if _, err := Billed(1, rates[:1], usage); !errors.Is(err, ErrMissingRate) {
		t.Fatalf("missing rate: %v", err)
	}
	if _, err := Billed(0, rates, usage); !errors.Is(err, ErrUnsupported) {
		t.Fatalf("model 0: %v", err)
	}
	huge := core.Usage{TokensIn: 1 << 63}
	if _, err := Billed(1, rates, huge); !errors.Is(err, ErrOverflow) {
		t.Fatalf("overflow: %v", err)
	}
}

func usdc(m uint64) AssetAmount { return AssetAmount{ERC20("base", "0xusdc"), Amount{-6, m}} }

func TestExactUnitsOrRejection(t *testing.T) {
	if n, err := ToAtomicUnits(Amount{-6, 5200}, 6); err != nil || n != 5200 {
		t.Fatalf("%d %v", n, err)
	}
	if _, err := ToAtomicUnits(Amount{-2, 5}, 6); !errors.Is(err, ErrInexactUnits) {
		t.Fatalf("inexact conversion accepted: %v", err)
	}
}

// Layer §9.5: 2 and 8 µunits per token in/out; checkpoint usage 1000 in /
// 400 out = 5200; one more interval of credit = 8000; ceiling 13 200.
func TestM6CeilingMatchesTheLayerWorkedExample(t *testing.T) {
	rates := []Rate{{UnitTokenIn, usdc(2)}, {UnitTokenOut, usdc(8)}}
	ceiling, err := VoucherCeiling(1, rates, core.Usage{TokensIn: 1000, TokensOut: 400}, usdc(8000))
	if err != nil || ceiling != usdc(13_200) {
		t.Fatalf("ceiling %v %v", ceiling, err)
	}
	if ok, _ := WithinCeiling(usdc(13_200), ceiling); !ok {
		t.Fatal("voucher at the ceiling rejected")
	}
	if ok, _ := WithinCeiling(usdc(13_201), ceiling); ok {
		t.Fatal("voucher above the ceiling accepted")
	}
}

func TestDigestsAreDeterministicAndFieldSensitive(t *testing.T) {
	a := DigestMPPTempo([]byte{1, 2, 3}, Amount{-6, 5200}, "ref-1", "tempo")
	if !bytes.Equal(a, DigestMPPTempo([]byte{1, 2, 3}, Amount{-6, 5200}, "ref-1", "tempo")) {
		t.Fatal("not deterministic")
	}
	if bytes.Equal(a, DigestMPPTempo([]byte{1, 2, 3}, Amount{-6, 5201}, "ref-1", "tempo")) {
		t.Fatal("amount not covered")
	}
	// Independent oracle: Python cbor2 canonical encoding + hashlib.
	if hex.EncodeToString(a) != "1220510f0bc555517e0242c4ccbfd4648445f3d6835221fa404cea50b11c1261ac7d" {
		t.Fatalf("digest %x differs from the cbor2 oracle", a)
	}
	if len(a) != 34 || a[0] != 0x12 || a[1] != 0x20 || len(DigestRailBytes([]byte("tx"))) != 34 {
		t.Fatal("not a sha2-256 Multihash")
	}
}
