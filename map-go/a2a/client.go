package a2a

// A2A v1.0 JSON-RPC requester with the MAP extension (mirrors
// map-a2a::client): opens a task with the map/v2 session metadata (A2A-7),
// countersigns each checkpoint the provider pauses on (A2A-21), countersigns
// the terminal receipt and returns it on a referenceTaskIds Message. If the
// provider disappears mid-task, the caller gets back the RequesterTask, whose
// last countersigned checkpoint is the resumption point for the next
// provider (A2A-23).

import (
	"bytes"
	"crypto/ed25519"
	"crypto/rand"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/jesselarsen/map-rfc/map-go/core"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

var httpClient = &http.Client{Timeout: 10 * time.Second}

// Client errors.
var (
	ErrTransport = errors.New("a2a client: transport")
	ErrRPC       = errors.New("a2a client: JSON-RPC error")
	ErrProtocol  = errors.New("a2a client: protocol")
)

// Endpoint is a provider found through its signed Agent Card.
type Endpoint struct {
	Base, RPCURL string
	Agent        identity.AgentID
	Card         map[string]any
}

// Discover fetches and verifies a provider's Agent Card: it must declare the
// MAP extension and its JWS must verify under the AgentID the extension
// names.
func Discover(base string) (*Endpoint, error) {
	base = strings.TrimRight(base, "/")
	resp, err := httpClient.Get(base + "/.well-known/agent-card.json")
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrTransport, err)
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrTransport, err)
	}
	v, err := ParseJSON(raw)
	card, ok := v.(map[string]any)
	if err != nil || !ok {
		return nil, fmt.Errorf("%w: card is not a JSON object", ErrProtocol)
	}
	agent, err := identity.ParseMultibase(MapExtensionAgent(card))
	if err != nil {
		return nil, fmt.Errorf("%w: card does not declare the MAP extension", ErrProtocol)
	}
	if _, err := VerifyCard(card, agent); err != nil {
		return nil, fmt.Errorf("%w: card signature: %v", ErrProtocol, err)
	}
	ifaces, _ := card["supportedInterfaces"].([]any)
	for _, it := range ifaces {
		m, _ := it.(map[string]any)
		if m["protocolBinding"] == "JSONRPC" {
			if u, ok := m["url"].(string); ok {
				return &Endpoint{Base: base, RPCURL: u, Agent: agent, Card: card}, nil
			}
		}
	}
	return nil, fmt.Errorf("%w: card has no JSONRPC interface", ErrProtocol)
}

// RPC makes one JSON-RPC call; mapActive activates the extension.
func RPC(url, method string, params any, mapActive bool) (map[string]any, error) {
	body, _ := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
	req, _ := http.NewRequest(http.MethodPost, url, bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("A2A-Version", "1.0")
	if mapActive {
		req.Header.Set("A2A-Extensions", MapExtensionURI)
	}
	resp, err := httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrTransport, err)
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(io.LimitReader(resp.Body, 8<<20))
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrTransport, err)
	}
	v, err := ParseJSON(raw)
	out, ok := v.(map[string]any)
	if err != nil || !ok {
		return nil, fmt.Errorf("%w: response is not a JSON object", ErrProtocol)
	}
	if e, ok := out["error"]; ok {
		b, _ := json.Marshal(e)
		return nil, fmt.Errorf("%w: %s", ErrRPC, b)
	}
	res, ok := out["result"].(map[string]any)
	if !ok {
		return nil, fmt.Errorf("%w: no result", ErrProtocol)
	}
	return res, nil
}

// Outcome is where a finished or abandoned task stands.
type Outcome struct {
	A2ATaskID string
	Task      *RequesterTask
	Acked     *core.Usage // usage in the last countersigned artifact
	Receipt   []byte      // dual-signed, if the task completed
	Err       error
}

// Pay is the requester's voucher hook: given acknowledged cumulative usage,
// sign the next cumulative voucher on the rail and return its digest.
type Pay func(provider identity.AgentID, u core.Usage) []byte

// Resume names the task being resumed (A2A-23).
type Resume struct {
	From          *RequesterTask
	FromA2ATaskID string
}

func message(parts []any, meta map[string]any) map[string]any {
	return map[string]any{
		"messageId":  fmt.Sprintf("m-%d", NowMs()),
		"role":       "ROLE_USER",
		"parts":      parts,
		"extensions": []any{MapExtensionURI},
		"metadata":   map[string]any{MetaKey: meta},
	}
}

// RunTask runs one task on ep to completion, or until the provider fails.
func RunTask(ep *Endpoint, priv ed25519.PrivateKey, prompt, capability string, zone uint8, resume *Resume, pay Pay) Outcome {
	var out Outcome
	out.Err = drive(ep, priv, prompt, capability, zone, resume, pay, &out)
	return out
}

func drive(ep *Endpoint, priv ed25519.PrivateKey, prompt, capability string, zone uint8, resume *Resume, pay Pay, out *Outcome) error {
	var nonce [16]byte
	if _, err := rand.Read(nonce[:]); err != nil {
		return err
	}
	pw := PromptWords(prompt)
	started := NowMs()
	meta := map[string]any{"agentId": agentOf(priv).Multibase(), "taskNonce": ArtifactToJSON(nonce[:]), "zone": int(zone)}
	parts := []any{map[string]any{"text": prompt}}
	if resume != nil {
		pkg, err := resume.From.ResumePackage()
		if err != nil {
			return err
		}
		meta["checkpoint"] = ArtifactToJSON(pkg.Checkpoint)
		parts = append(parts, map[string]any{"data": map[string]any{"mapTranscriptPrefix": pkg.PrefixParts}})
	}
	msg := message(parts, meta)
	if resume != nil {
		msg["referenceTaskIds"] = []any{resume.FromA2ATaskID}
	}
	result, err := RPC(ep.RPCURL, "SendMessage", map[string]any{"message": msg}, true)
	if err != nil {
		return err
	}
	seen := 0
	for {
		task, ok := result["task"].(map[string]any)
		if !ok {
			return fmt.Errorf("%w: SendMessage did not return a Task", ErrProtocol)
		}
		a2aID, _ := task["id"].(string)
		if a2aID == "" {
			return fmt.Errorf("%w: Task without id", ErrProtocol)
		}
		if out.Task == nil {
			// The provider allocates Task.id, so the TaskID (A2A-8) is
			// derivable only now.
			if resume == nil {
				out.Task = StartRequester(priv, ep.Agent, nonce, a2aID, capability, zone)
			} else {
				next, _, err := resume.From.ResumeOn(ep.Agent, nonce, a2aID)
				if err != nil {
					return err
				}
				out.Task = next
			}
			out.A2ATaskID = a2aID
		}
		arts, _ := task["artifacts"].([]any)
		var delivered []any
		if len(arts) > 0 {
			a0, _ := arts[0].(map[string]any)
			delivered, _ = a0["parts"].([]any)
		}
		for _, p := range delivered[seen:] {
			if err := out.Task.OnPart(p); err != nil {
				return err
			}
		}
		seen = len(delivered)
		observed := ObservedUsage(pw, uint64(seen))
		status, _ := task["status"].(map[string]any)
		carried := MapMeta(task)
		switch status["state"] {
		case "TASK_STATE_INPUT_REQUIRED":
			proposal, ok := ArtifactFromJSON(carried["checkpoint"])
			if !ok {
				return fmt.Errorf("%w: paused without a checkpoint", ErrProtocol)
			}
			cs, err := out.Task.OnCheckpoint(proposal, observed)
			if err != nil {
				return err
			}
			out.Acked = &observed
			next := message([]any{map[string]any{"text": "checkpoint countersigned"}}, map[string]any{
				"checkpoint": ArtifactToJSON(cs), "voucherDigest": ArtifactToJSON(pay(ep.Agent, observed)),
			})
			next["taskId"] = a2aID
			next["contextId"] = task["contextId"]
			if result, err = RPC(ep.RPCURL, "SendMessage", map[string]any{"message": next}, true); err != nil {
				return err
			}
		case "TASK_STATE_COMPLETED":
			proposal, ok := ArtifactFromJSON(carried["receipt"])
			if !ok {
				return fmt.Errorf("%w: completed without a receipt", ErrProtocol)
			}
			dual, err := out.Task.OnReceipt(proposal, observed, started, NowMs())
			if err != nil {
				return err
			}
			out.Acked = &observed
			ack := message([]any{map[string]any{"text": "Countersigned receipt attached."}}, map[string]any{
				"receipt": ArtifactToJSON(dual), "voucherDigest": ArtifactToJSON(pay(ep.Agent, observed)),
			})
			ack["referenceTaskIds"] = []any{a2aID}
			if _, err := RPC(ep.RPCURL, "SendMessage", map[string]any{"message": ack}, true); err != nil {
				return err
			}
			out.Receipt = dual
			return nil
		default:
			return fmt.Errorf("%w: unexpected task state %v", ErrProtocol, status["state"])
		}
	}
}
