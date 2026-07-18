package pay

import (
	"crypto/ed25519"
	"errors"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

const entryDomain = "MAPLedgerEntry"

// LedgerError enumerates credit-ledger settlement failures.
var (
	// ErrInsufficient: payer balance below the amount.
	ErrInsufficient = errors.New("pay: insufficient balance")
	// ErrExponentMismatch: amount exponent differs from the operator exponent.
	ErrExponentMismatch = errors.New("pay: exponent mismatch")
)

// EntryProof is a signed ledger entry — the PAY_PROOF payload for
// pay:credit-ledger. entry_bytes = enc(["MAPLedgerEntry", invoice-id, payer,
// payee, [asset,amount], ts]); sig = operator Ed25519 over those bytes.
type EntryProof struct {
	EntryBytes []byte
	Sig        [64]byte
}

// ToBytes encodes CBOR [entry_bytes, sig] — the PAY_PROOF field-3 payload.
func (p EntryProof) ToBytes() []byte {
	return cbor.Encode(cbor.Array{cbor.Bytes(p.EntryBytes), cbor.Bytes(p.Sig[:])})
}

// EntryProofFromBytes decodes the [entry_bytes, sig] array.
func EntryProofFromBytes(b []byte) (EntryProof, error) {
	v, err := cbor.Decode(b)
	if err != nil {
		return EntryProof{}, badField(3)
	}
	arr, ok := v.(cbor.Array)
	if !ok || len(arr) != 2 {
		return EntryProof{}, badField(3)
	}
	eb, ok := arr[0].(cbor.Bytes)
	if !ok {
		return EntryProof{}, badField(3)
	}
	sg, ok := arr[1].(cbor.Bytes)
	if !ok || len(sg) != 64 {
		return EntryProof{}, badField(3)
	}
	var proof EntryProof
	proof.EntryBytes = append([]byte(nil), eb...)
	copy(proof.Sig[:], sg)
	return proof, nil
}

func entryBytes(invoiceID [16]byte, payer, payee identity.AgentID, aa AssetAmount, ts uint64) []byte {
	return cbor.Encode(cbor.Array{
		cbor.Text(entryDomain),
		cbor.Bytes(invoiceID[:]),
		cbor.Bytes(payer.Bytes()),
		cbor.Bytes(payee.Bytes()),
		aa.ToValue(),
		cbor.Uint(ts),
	})
}

// LedgerOperator is an in-process credit-ledger operator. Balances are minor
// units (int64 mantissa) at one configured exponent. Escrow holds are NOT
// ported (out of TASK-029 scope); only the postpaid Pay is implemented.
type LedgerOperator struct {
	priv     ed25519.PrivateKey
	agent    identity.AgentID
	exponent int32
	balances map[identity.AgentID]int64
	seen     map[[16]byte]EntryProof
}

// NewLedgerOperator builds an operator from a 32-byte ed25519 seed at the
// given decimal exponent.
func NewLedgerOperator(seed []byte, exponent int32) (*LedgerOperator, error) {
	priv := ed25519.NewKeyFromSeed(seed)
	aid, err := identity.FromPub(priv.Public().(ed25519.PublicKey))
	if err != nil {
		return nil, err
	}
	return &LedgerOperator{
		priv:     priv,
		agent:    aid,
		exponent: exponent,
		balances: make(map[identity.AgentID]int64),
		seen:     make(map[[16]byte]EntryProof),
	}, nil
}

// AgentID returns the operator's identity (the payee verifies proofs against
// it).
func (op *LedgerOperator) AgentID() identity.AgentID { return op.agent }

// Credit adds minor units to an account's balance.
func (op *LedgerOperator) Credit(who identity.AgentID, minor int64) {
	op.balances[who] += minor
}

// Balance returns an account's minor-unit balance.
func (op *LedgerOperator) Balance(who identity.AgentID) int64 { return op.balances[who] }

// Pay moves amount from payer to payee and emits a signed entry. Idempotent by
// invoice-id: a re-seen invoice returns its ORIGINAL entry with NO balance
// movement. Insufficient balance / exponent mismatch → error.
func (op *LedgerOperator) Pay(invoiceID [16]byte, payer, payee identity.AgentID, aa AssetAmount, ts uint64) (EntryProof, error) {
	if existing, ok := op.seen[invoiceID]; ok {
		return existing, nil
	}
	if aa.Amount.Exponent != op.exponent {
		return EntryProof{}, ErrExponentMismatch
	}
	minor := aa.Amount.Mantissa
	if op.balances[payer] < minor {
		return EntryProof{}, ErrInsufficient
	}
	op.balances[payer] -= minor
	op.balances[payee] += minor
	eb := entryBytes(invoiceID, payer, payee, aa, ts)
	var proof EntryProof
	proof.EntryBytes = eb
	copy(proof.Sig[:], ed25519.Sign(op.priv, eb))
	op.seen[invoiceID] = proof
	return proof, nil
}

// VerifyEntry is the payee's proof check: operator Ed25519 signature over the
// entry bytes, and the signed entry's fields match the invoice being settled
// (domain, invoice-id, parties, [asset,amount]).
func VerifyEntry(proof EntryProof, operator identity.AgentID, invoiceID [16]byte, payer, payee identity.AgentID, aa AssetAmount) bool {
	if identity.VerifyStrict(operator.Pub(), proof.EntryBytes, proof.Sig[:]) != nil {
		return false
	}
	v, err := cbor.Decode(proof.EntryBytes)
	if err != nil {
		return false
	}
	arr, ok := v.(cbor.Array)
	if !ok || len(arr) != 6 {
		return false
	}
	dom, ok := arr[0].(cbor.Text)
	if !ok || string(dom) != entryDomain {
		return false
	}
	if !bytesEq(arr[1], invoiceID[:]) || !bytesEq(arr[2], payer.Bytes()) || !bytesEq(arr[3], payee.Bytes()) {
		return false
	}
	got, err := AssetAmountFromValue(arr[4])
	if err != nil || !got.Equal(aa) {
		return false
	}
	return true
}

func bytesEq(v cbor.Value, want []byte) bool {
	b, ok := v.(cbor.Bytes)
	if !ok || len(b) != len(want) {
		return false
	}
	for i := range b {
		if b[i] != want[i] {
			return false
		}
	}
	return true
}
