package a2a

// Binding primitives (spec/binding-a2a.md §3–§5): the metadata carriage key,
// TaskID derivation, and the A2A Part → transcript leaf mapping.

import (
	"crypto/sha256"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/core"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/transcript"
)

// MetaKey: all MAP artifacts ride under this one metadata key (A2A-7).
const MetaKey = "map/v2"

// Binding error codes (A2A-26), carried as metadata."map/v2".error.code.
const (
	CodeCountersignatureRefused     = 1
	CodeRootMismatch                = 2
	CodeZoneViolation               = 3
	CodeBadCheckpoint               = 4
	CodeTaskIDMismatch              = 5
	CodeUnsupportedExtensionVersion = 6
	CodeZoneMismatch                = 7
)

// TaskIDFor is A2A-8 (decision D6): SHA-256(enc(["MAPTaskID",
// requesterNonce, providerAgentID, a2aTaskId-bytes])).
func TaskIDFor(nonce [16]byte, provider identity.AgentID, a2aTaskID string) core.TaskID {
	enc := cbor.Encode(cbor.Array{
		cbor.Text("MAPTaskID"), cbor.Bytes(nonce[:]), cbor.Bytes(provider[:]), cbor.Bytes([]byte(a2aTaskID)),
	})
	return core.TaskID(sha256.Sum256(enc))
}

// PartLeafContent is A2A-14: the JCS of the Part EXCLUDING its `metadata`
// member (MAP artifacts ride there).
func PartLeafContent(part any) ([]byte, error) {
	p := deepCopy(part)
	if m, ok := p.(map[string]any); ok {
		delete(m, "metadata")
	}
	return Canonicalize(p)
}

// PartChunk is A2A-12/13: the transcript chunk for the seq-th delivered
// artifact Part (kind 0).
func PartChunk(seq uint64, part any) (transcript.Chunk, error) {
	c, err := PartLeafContent(part)
	if err != nil {
		return transcript.Chunk{}, err
	}
	return transcript.Chunk{Seq: seq, Kind: transcript.ChunkOutputText, Content: c}, nil
}

// MapMeta returns obj.metadata."map/v2" as an object, or nil.
func MapMeta(obj any) map[string]any {
	o, _ := obj.(map[string]any)
	md, _ := o["metadata"].(map[string]any)
	m, _ := md[MetaKey].(map[string]any)
	return m
}

// SetMapMeta sets obj.metadata."map/v2".<field>, creating containers.
func SetMapMeta(obj map[string]any, field string, value any) {
	md, _ := obj["metadata"].(map[string]any)
	if md == nil {
		md = map[string]any{}
		obj["metadata"] = md
	}
	m, _ := md[MetaKey].(map[string]any)
	if m == nil {
		m = map[string]any{}
		md[MetaKey] = m
	}
	m[field] = value
}

// ArtifactToJSON is base64url of CBOR artifact bytes, as carried in metadata.
func ArtifactToJSON(b []byte) string { return b64.EncodeToString(b) }

// ArtifactFromJSON decodes a metadata artifact string; ok is false if v is
// not a valid base64url string.
func ArtifactFromJSON(v any) ([]byte, bool) {
	s, ok := v.(string)
	if !ok {
		return nil, false
	}
	b, err := b64.DecodeString(s)
	return b, err == nil
}

// ErrorJSON is a binding error object {"code": n, "detail": "..."}.
func ErrorJSON(code int, detail string) map[string]any {
	return map[string]any{"code": code, "detail": detail}
}
