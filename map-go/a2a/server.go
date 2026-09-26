package a2a

// A2A v1.0 JSON-RPC provider with the MAP extension (mirrors
// map-a2a::server). Serves GET /.well-known/agent-card.json and JSON-RPC
// SendMessage / GetTask on POST /a2a. Checkpoints use the mandatory A2A-21
// path: the task pauses in TASK_STATE_INPUT_REQUIRED with the proposal in
// Task.metadata."map/v2".checkpoint, and the requester's next SendMessage on
// that task carries the countersignature (and its voucherDigest). A client
// that does not send `A2A-Extensions: <MAP URI>` gets a plain task that runs
// to completion with no MAP metadata and no pauses (A2A-28).

import (
	"crypto/ed25519"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"strings"
	"sync"

	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/receipt"
	"github.com/jesselarsen/map-rfc/map-go/transcript"
)

// Workload maps a prompt to output words, deterministically.
type Workload func(prompt string) []string

// ServerConfig configures a Provider.
type ServerConfig struct {
	Key             ed25519.PrivateKey
	Card            map[string]any // the signed Agent Card
	Capability      string
	SupportedZones  []uint8
	CheckpointEvery uint64
	FailAfterParts  uint64 // 0 = never; else crash once this many Parts were produced
	Workload        Workload
	// Crash simulates the provider dying (default: os.Exit(3)). A test can
	// panic instead: net/http then drops the connection, which is what a
	// requester sees when a provider dies.
	Crash func()
}

type taskState struct {
	id, contextID   string
	words           []string
	cursor          int
	parts           []any
	state           string
	m               *mapState
	startedMs       uint64
	carriedField    string // "checkpoint" | "receipt" | ""
	carriedArtifact []byte
}

type mapState struct {
	task            *ProviderTask
	promptWords     uint64
	ownParts        uint64
	receiptProposal []byte
}

// Provider is the HTTP A2A provider.
type Provider struct {
	cfg      ServerConfig
	mu       sync.Mutex
	tasks    map[string]*taskState
	produced uint64
	nextID   int
}

// NewProvider returns a provider.
func NewProvider(cfg ServerConfig) *Provider {
	return &Provider{cfg: cfg, tasks: map[string]*taskState{}, nextID: 1}
}

func rpcOK(id, result any) map[string]any {
	return map[string]any{"jsonrpc": "2.0", "id": id, "result": result}
}

func rpcErr(id any, code int, msg string) map[string]any {
	return map[string]any{"jsonrpc": "2.0", "id": id, "error": map[string]any{"code": code, "message": msg}}
}

// mapErr surfaces a binding error (A2A-26) through JSON-RPC's own error
// object (A2A-27), with the MAP code in data."map/v2".error.
func mapErr(id any, code int, detail string) map[string]any {
	return map[string]any{"jsonrpc": "2.0", "id": id, "error": map[string]any{
		"code": -32602, "message": detail, "data": map[string]any{MetaKey: map[string]any{"error": ErrorJSON(code, detail)}},
	}}
}

// HandleRPC handles one JSON-RPC request body. mapActive reflects the
// A2A-Extensions request header.
func (p *Provider) HandleRPC(body map[string]any, mapActive bool) map[string]any {
	p.mu.Lock()
	defer p.mu.Unlock()
	id := body["id"]
	params, _ := body["params"].(map[string]any)
	switch body["method"] {
	case "SendMessage":
		msg, ok := params["message"].(map[string]any)
		if !ok {
			return rpcErr(id, -32602, "params.message is required")
		}
		return p.sendMessage(id, msg, mapActive)
	case "GetTask":
		tid, _ := params["id"].(string)
		t := p.tasks[tid]
		if t == nil {
			return rpcErr(id, -32001, "task not found")
		}
		return rpcOK(id, t.json())
	case nil:
		return rpcErr(id, -32600, "invalid request")
	default:
		return rpcErr(id, -32601, "method not found")
	}
}

func (p *Provider) sendMessage(id any, msg map[string]any, mapActive bool) map[string]any {
	meta := MapMeta(msg)
	// 1. A countersigned receipt for a finished task: a Message that
	//    references the task instead of reopening it (A2A-7).
	if _, hasTask := msg["taskId"]; !hasTask {
		if rc, ok := ArtifactFromJSON(meta["receipt"]); ok {
			refs, _ := msg["referenceTaskIds"].([]any)
			tid := ""
			if len(refs) > 0 {
				tid, _ = refs[0].(string)
			}
			if err := p.acceptReceipt(tid, rc); err != nil {
				return mapErr(id, CodeCountersignatureRefused, err.Error())
			}
			return rpcOK(id, map[string]any{"message": map[string]any{
				"messageId": "ack-" + tid, "role": "ROLE_AGENT",
				"parts": []any{map[string]any{"text": "receipt countersignature accepted"}},
			}})
		}
	}
	// 2. Continuation of a paused task, carrying a countersigned checkpoint.
	if tid, ok := msg["taskId"].(string); ok {
		t := p.tasks[tid]
		if t == nil {
			return rpcErr(id, -32001, "task not found")
		}
		if cs, ok := ArtifactFromJSON(meta["checkpoint"]); ok {
			if t.m == nil {
				return rpcErr(id, -32602, "task is not running the MAP extension")
			}
			if err := t.m.task.OnCountersigned(cs); err != nil {
				return mapErr(id, CodeBadCheckpoint, err.Error())
			}
			// The requester's latest cumulative voucher (layer §9.3).
			if d, ok := ArtifactFromJSON(meta["voucherDigest"]); ok {
				t.m.task.SetVoucherDigest(d)
			}
		}
		return p.run(id, t)
	}
	// 3. A new task, possibly resuming another provider's task.
	return p.newTask(id, msg, mapActive)
}

func firstText(msg map[string]any) string {
	parts, _ := msg["parts"].([]any)
	for _, pt := range parts {
		if m, ok := pt.(map[string]any); ok {
			if s, ok := m["text"].(string); ok {
				return s
			}
		}
	}
	return ""
}

func (p *Provider) newTask(id any, msg map[string]any, mapActive bool) map[string]any {
	prompt := firstText(msg)
	tid := fmt.Sprintf("task-%d", p.nextID)
	p.nextID++
	ctx, _ := msg["contextId"].(string)
	if ctx == "" {
		ctx = "ctx-1"
	}
	t := &taskState{id: tid, contextID: ctx, words: p.cfg.Workload(prompt), state: "TASK_STATE_WORKING", startedMs: NowMs()}
	if mapActive {
		task, skip, code, err := p.startMap(tid, msg)
		if err != nil {
			return mapErr(id, code, err.Error())
		}
		t.cursor = skip
		t.m = &mapState{task: task, promptWords: PromptWords(prompt)}
	}
	p.tasks[tid] = t
	return p.run(id, t)
}

// startMap sets up MAP state from the opening Message's map/v2 metadata: a
// fresh task, or a resumption with a checkpoint and a mapTranscriptPrefix
// DataPart (A2A-23).
func (p *Provider) startMap(tid string, msg map[string]any) (*ProviderTask, int, int, error) {
	bad := func(s string) (*ProviderTask, int, int, error) { return nil, 0, CodeBadCheckpoint, errors.New(s) }
	m := MapMeta(msg)
	if m == nil {
		return bad("map/v2 metadata missing on the opening Message")
	}
	nb, ok := ArtifactFromJSON(m["taskNonce"])
	if !ok || len(nb) != 16 {
		return bad("taskNonce missing or not 16 bytes")
	}
	var nonce [16]byte
	copy(nonce[:], nb)
	as, _ := m["agentId"].(string)
	requester, err := identity.ParseMultibase(as)
	if err != nil {
		return bad("agentId missing or invalid")
	}
	zn, ok := m["zone"].(json.Number)
	zone, zerr := zn.Int64()
	if !ok || zerr != nil {
		return bad("zone missing")
	}
	accepted := false
	for _, z := range p.cfg.SupportedZones {
		accepted = accepted || int64(z) == zone
	}
	if !accepted {
		return nil, 0, CodeZoneViolation, fmt.Errorf("this provider accepts zones %v", p.cfg.SupportedZones)
	}
	pcfg := ProviderConfig{Capability: p.cfg.Capability, Zone: uint8(zone), CheckpointEvery: p.cfg.CheckpointEvery}
	cp, ok := ArtifactFromJSON(m["checkpoint"])
	if !ok {
		return StartProvider(p.cfg.Key, requester, nonce, tid, pcfg), 0, 0, nil
	}
	var prefix []any
	parts, _ := msg["parts"].([]any)
	for _, pt := range parts {
		pm, _ := pt.(map[string]any)
		d, _ := pm["data"].(map[string]any)
		if pre, ok := d["mapTranscriptPrefix"].([]any); ok {
			prefix = pre
		}
	}
	if prefix == nil {
		return bad("resumption without a mapTranscriptPrefix DataPart")
	}
	t, err := ResumeProvider(p.cfg.Key, requester, nonce, tid, pcfg, cp, prefix)
	switch {
	case errors.Is(err, transcript.ErrRootMismatch):
		return nil, 0, CodeRootMismatch, errors.New("prefix does not match the countersigned root")
	case errors.Is(err, ErrZoneWidened):
		return nil, 0, CodeZoneViolation, errors.New("resumption would widen the checkpoint's zone")
	case err != nil:
		return nil, 0, CodeBadCheckpoint, err
	}
	return t, int(t.NextSeq()), 0, nil
}

// run emits Parts until the next checkpoint proposal (MAP) or completion.
func (p *Provider) run(id any, t *taskState) map[string]any {
	t.carriedField, t.carriedArtifact = "", nil
	t.state = "TASK_STATE_WORKING"
	for t.cursor < len(t.words) {
		part := map[string]any{"text": t.words[t.cursor] + " ", "mediaType": "text/plain"}
		t.cursor++
		t.parts = append(t.parts, part)
		p.produced++
		var proposal []byte
		if t.m != nil {
			t.m.ownParts++
			var err error
			proposal, err = t.m.task.Deliver(part, ObservedUsage(t.m.promptWords, t.m.ownParts), NowMs())
			if err != nil {
				return rpcErr(id, -32603, err.Error())
			}
		}
		if p.cfg.FailAfterParts > 0 && p.produced >= p.cfg.FailAfterParts {
			log.Printf("[provider] simulated crash after producing %d parts", p.produced)
			if p.cfg.Crash != nil {
				p.cfg.Crash()
			}
			os.Exit(3)
		}
		if proposal != nil {
			t.state = "TASK_STATE_INPUT_REQUIRED"
			t.carriedField, t.carriedArtifact = "checkpoint", proposal
			return rpcOK(id, map[string]any{"task": t.json()})
		}
	}
	t.state = "TASK_STATE_COMPLETED"
	if t.m != nil {
		pr := t.m.task.ProposeReceipt(receipt.OutcomeOK, ObservedUsage(t.m.promptWords, t.m.ownParts), t.startedMs, NowMs())
		t.m.receiptProposal = pr
		t.carriedField, t.carriedArtifact = "receipt", pr
	}
	return rpcOK(id, map[string]any{"task": t.json()})
}

func (p *Provider) acceptReceipt(tid string, countersigned []byte) error {
	t := p.tasks[tid]
	if t == nil {
		return errors.New("task not found")
	}
	if t.m == nil || t.m.receiptProposal == nil {
		return errors.New("no receipt proposed for this task")
	}
	if _, err := t.m.task.OnCountersignedReceipt(t.m.receiptProposal, countersigned); err != nil {
		return err
	}
	log.Printf("[provider] dual-signed receipt stored for %s", tid)
	return nil
}

func (t *taskState) json() map[string]any {
	parts := append([]any{}, t.parts...)
	task := map[string]any{
		"id": t.id, "contextId": t.contextID,
		"status":    map[string]any{"state": t.state},
		"artifacts": []any{map[string]any{"artifactId": "output", "name": "output", "parts": parts}},
	}
	if t.carriedField != "" {
		SetMapMeta(task, t.carriedField, ArtifactToJSON(t.carriedArtifact))
	}
	return task
}

// ServeHTTP implements http.Handler.
func (p *Provider) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	mapActive := false
	for _, h := range r.Header.Values("A2A-Extensions") {
		for _, u := range strings.Split(h, ",") {
			mapActive = mapActive || strings.TrimSpace(u) == MapExtensionURI
		}
	}
	var out any
	switch {
	case r.Method == http.MethodGet && r.URL.Path == "/.well-known/agent-card.json":
		out = p.cfg.Card
	case r.Method == http.MethodPost && r.URL.Path == "/a2a":
		raw, _ := io.ReadAll(io.LimitReader(r.Body, 8<<20))
		v, err := ParseJSON(raw)
		body, ok := v.(map[string]any)
		if err != nil || !ok {
			out = rpcErr(nil, -32700, "parse error")
		} else {
			out = p.HandleRPC(body, mapActive)
			if mapActive {
				w.Header().Set("A2A-Extensions", MapExtensionURI) // echo activation
			}
		}
	default:
		w.WriteHeader(http.StatusNotFound)
		out = map[string]any{"error": "not found"}
	}
	b, err := json.Marshal(out)
	if err != nil {
		b, _ = json.Marshal(rpcErr(nil, -32603, err.Error()))
	}
	_, _ = w.Write(b)
}
