package a2a

// Provider and requester accountability flows over A2A (binding §5–§8;
// layer §6–§7), mirroring map-a2a::flow. Transport-agnostic: these types
// consume and produce A2A Part JSON and MAP artifact bytes; an HTTP layer
// (or the simulator) moves them. Signing order (layer §6.3 as bound by
// A2A-7): the provider proposes checkpoints and the final receipt; the
// requester verifies against its own observation and countersigns.

import (
	"bytes"
	"crypto/ed25519"
	"errors"
	"fmt"

	"github.com/jesselarsen/map-rfc/map-go/core"
	"github.com/jesselarsen/map-rfc/map-go/identity"
	"github.com/jesselarsen/map-rfc/map-go/receipt"
	"github.com/jesselarsen/map-rfc/map-go/transcript"
	"github.com/jesselarsen/map-rfc/map-go/trust"
)

// Flow errors. Checkpoint and receipt errors pass through wrapped.
var (
	ErrMismatch          = errors.New("binding: artifact does not match this task")
	ErrOutsideTolerance  = errors.New("binding: outside the countersigner's tolerance")
	ErrZoneWidened       = errors.New("binding: resumption would widen the zone (Z-3)")
	ErrNoResumptionPoint = errors.New("binding: no countersigned checkpoint to resume from")
)

func mismatch(what string) error { return fmt.Errorf("%w: %s", ErrMismatch, what) }

func agentOf(priv ed25519.PrivateKey) identity.AgentID {
	a, err := identity.FromPub(priv.Public().(ed25519.PublicKey))
	if err != nil {
		panic(err)
	}
	return a
}

// ProviderConfig is provider-side policy for one task.
type ProviderConfig struct {
	Capability      string
	Zone            uint8  // the zone label the task runs under (layer §10)
	CheckpointEvery uint64 // propose a checkpoint every this many Parts
}

// ProviderTask is the provider's side of one MAP-over-A2A task.
type ProviderTask struct {
	priv              ed25519.PrivateKey
	requester         identity.AgentID
	task              core.TaskID
	cfg               ProviderConfig
	builder           *transcript.TranscriptBuilder
	nextSeq           uint64
	sinceCheckpoint   uint64
	resumedFrom       []byte
	pending           *transcript.CheckpointArtifact
	lastCountersigned *transcript.CheckpointArtifact
	voucherDigest     []byte
}

// StartProvider begins a fresh task (TaskID per A2A-8).
func StartProvider(priv ed25519.PrivateKey, requester identity.AgentID, nonce [16]byte, a2aTaskID string, cfg ProviderConfig) *ProviderTask {
	return &ProviderTask{
		priv: priv, requester: requester, cfg: cfg,
		task:    TaskIDFor(nonce, agentOf(priv), a2aTaskID),
		builder: transcript.NewTranscript(),
	}
}

// ResumeProvider continues a task another provider started (layer §7.4,
// A2A-23..25): the checkpoint must be a valid resumption point for this
// requester whose root matches the prefix Parts, and the zone must not
// widen (Z-3).
func ResumeProvider(priv ed25519.PrivateKey, requester identity.AgentID, nonce [16]byte, a2aTaskID string, cfg ProviderConfig, checkpoint []byte, prefixParts []any) (*ProviderTask, error) {
	art, err := transcript.DecodeArtifact(checkpoint)
	if err != nil {
		return nil, err
	}
	prefix := make([]transcript.Chunk, len(prefixParts))
	for i, p := range prefixParts {
		if prefix[i], err = PartChunk(uint64(i), p); err != nil {
			return nil, err
		}
	}
	r, err := transcript.Resume(art, prefix)
	if err != nil {
		return nil, err
	}
	if r.Core.Requester != requester {
		return nil, mismatch("checkpoint requester")
	}
	if cfg.Zone > r.Core.Zone {
		return nil, ErrZoneWidened
	}
	t := StartProvider(priv, requester, nonce, a2aTaskID, cfg)
	t.builder, t.nextSeq, t.resumedFrom = r.Builder, r.NextSeq, r.ResumedFrom
	return t, nil
}

// TaskID is this task's MAP TaskID.
func (t *ProviderTask) TaskID() core.TaskID { return t.task }

// NextSeq is the seq the next delivered Part gets.
func (t *ProviderTask) NextSeq() uint64 { return t.nextSeq }

// SetVoucherDigest binds the requester's latest cumulative voucher (layer
// §9.3); subsequent checkpoints and the receipt carry it.
func (t *ProviderTask) SetVoucherDigest(d []byte) { t.voucherDigest = d }

// Deliver records one delivered Part. At the cadence it returns a
// provider-signed checkpoint proposal (C-1), else nil.
func (t *ProviderTask) Deliver(part any, cumulative core.Usage, nowMs uint64) ([]byte, error) {
	c, err := PartChunk(t.nextSeq, part)
	if err != nil {
		return nil, err
	}
	t.builder.Push(c)
	t.nextSeq++
	t.sinceCheckpoint++
	if t.sinceCheckpoint < t.cfg.CheckpointEvery {
		return nil, nil
	}
	t.sinceCheckpoint = 0
	art := transcript.Propose(t.priv, transcript.CheckpointCore{
		Task: t.task, Requester: t.requester, Provider: agentOf(t.priv),
		ThroughSeq: t.nextSeq - 1, Root: t.builder.Root(), Usage: cumulative,
		Zone: t.cfg.Zone, TimestampMs: nowMs,
		VoucherDigest: t.voucherDigest, ResumedFrom: t.resumedFrom,
	})
	t.pending = &art
	return art.Encode(), nil
}

// OnCountersigned accepts the requester's countersignature on the pending
// proposal.
func (t *ProviderTask) OnCountersigned(b []byte) error {
	art, err := transcript.DecodeArtifact(b)
	if err != nil {
		return err
	}
	if t.pending == nil {
		return mismatch("no pending checkpoint")
	}
	if !bytes.Equal(art.CoreBytes, t.pending.CoreBytes) {
		return mismatch("countersigned a different core")
	}
	if _, err := art.VerifyResumptionPoint(); err != nil {
		return err
	}
	t.lastCountersigned, t.pending = &art, nil
	return nil
}

// OwedUsage is layer C-5: the usage in the last countersigned checkpoint.
func (t *ProviderTask) OwedUsage() (core.Usage, bool) {
	if t.lastCountersigned == nil {
		return core.Usage{}, false
	}
	c, err := t.lastCountersigned.Core()
	return c.Usage, err == nil
}

// ProposeReceipt signs the final receipt first (A2A-7).
func (t *ProviderTask) ProposeReceipt(outcome uint64, cumulative core.Usage, startedMs, endedMs uint64) []byte {
	root := t.builder.Root()
	zone := t.cfg.Zone
	c := receipt.ReceiptCore{
		Task: t.task, Requester: t.requester, Provider: agentOf(t.priv), Capability: t.cfg.Capability,
		StartedMs: startedMs, EndedMs: endedMs, Outcome: outcome, Metrics: cumulative,
		PaymentRef: t.voucherDigest, TranscriptRoot: &root, Zone: &zone, ResumedFrom: t.resumedFrom,
	}
	cb := c.Encode()
	return receipt.Receipt{CoreBytes: cb, SigProv: receipt.Sign(t.priv, receipt.RoleProvider, cb)}.Encode()
}

// OnCountersignedReceipt accepts the requester's countersignature for
// proposed.
func (t *ProviderTask) OnCountersignedReceipt(proposed, countersigned []byte) (receipt.Receipt, error) {
	p, err := receipt.Decode(proposed)
	if err != nil {
		return receipt.Receipt{}, err
	}
	r, err := receipt.Decode(countersigned)
	if err != nil {
		return receipt.Receipt{}, err
	}
	if !bytes.Equal(r.CoreBytes, p.CoreBytes) {
		return receipt.Receipt{}, mismatch("countersigned a different receipt core")
	}
	v, err := r.Verify()
	if err != nil {
		return receipt.Receipt{}, err
	}
	if !v.Dual {
		return receipt.Receipt{}, mismatch("receipt not dual-signed")
	}
	return r, nil
}

// RequesterTask is the requester's side of one MAP-over-A2A task.
type RequesterTask struct {
	priv           ed25519.PrivateKey
	provider       identity.AgentID
	task           core.TaskID
	capability     string
	zone           uint8
	tolerance      trust.Tolerance
	parts          []any
	chunks         []transcript.Chunk
	builder        *transcript.TranscriptBuilder
	resumedFrom    []byte
	lastCheckpoint []byte
	lastCore       *transcript.CheckpointCore
}

// ResumePackage is what the requester hands a successor provider (A2A-23).
type ResumePackage struct {
	Checkpoint  []byte
	PrefixParts []any
}

// StartRequester begins the requester side of a task.
func StartRequester(priv ed25519.PrivateKey, provider identity.AgentID, nonce [16]byte, a2aTaskID, capability string, zone uint8) *RequesterTask {
	return &RequesterTask{
		priv: priv, provider: provider, capability: capability, zone: zone,
		task:      TaskIDFor(nonce, provider, a2aTaskID),
		tolerance: trust.DefaultTolerance(),
		builder:   transcript.NewTranscript(),
	}
}

// TaskID is this task's MAP TaskID.
func (t *RequesterTask) TaskID() core.TaskID { return t.task }

// PartsReceived counts transcript Parts, including a resumed prefix.
func (t *RequesterTask) PartsReceived() int { return len(t.parts) }

// TranscriptRoot is the root over every Part received.
func (t *RequesterTask) TranscriptRoot() [32]byte { return t.builder.Root() }

// ResumePackage returns the last countersigned checkpoint and the Parts it
// covers.
func (t *RequesterTask) ResumePackage() (ResumePackage, error) {
	if t.lastCore == nil {
		return ResumePackage{}, ErrNoResumptionPoint
	}
	n := t.lastCore.ThroughSeq + 1
	return ResumePackage{Checkpoint: t.lastCheckpoint, PrefixParts: append([]any(nil), t.parts[:n]...)}, nil
}

// ResumeOn builds the requester for the successor task (new provider, new
// A2A task, new TaskID per A2A-25), continuing the transcript sequence.
func (t *RequesterTask) ResumeOn(newProvider identity.AgentID, nonce [16]byte, a2aTaskID string) (*RequesterTask, ResumePackage, error) {
	pkg, err := t.ResumePackage()
	if err != nil {
		return nil, pkg, err
	}
	next := StartRequester(t.priv, newProvider, nonce, a2aTaskID, t.capability, t.zone)
	next.tolerance = t.tolerance
	for _, p := range pkg.PrefixParts {
		if err := next.OnPart(p); err != nil {
			return nil, pkg, err
		}
	}
	art, err := transcript.DecodeArtifact(pkg.Checkpoint)
	if err != nil {
		return nil, pkg, err
	}
	next.resumedFrom = art.Digest()
	return next, pkg, nil
}

// OnPart records one delivered artifact Part.
func (t *RequesterTask) OnPart(part any) error {
	c, err := PartChunk(uint64(len(t.chunks)), part)
	if err != nil {
		return err
	}
	t.builder.Push(c)
	t.chunks = append(t.chunks, c)
	t.parts = append(t.parts, part)
	return nil
}

// OnCheckpoint verifies a proposal against what this requester actually
// received and observed, then countersigns it (C-2). It returns the
// dual-signed artifact bytes.
func (t *RequesterTask) OnCheckpoint(proposal []byte, observed core.Usage) ([]byte, error) {
	art, err := transcript.DecodeArtifact(proposal)
	if err != nil {
		return nil, err
	}
	v, err := art.Verify()
	if err != nil {
		return nil, err
	}
	if art.SigProv == nil {
		return nil, mismatch("checkpoint not signed by the provider")
	}
	c := v.Core
	switch {
	case c.Task != t.task:
		return nil, mismatch("task")
	case c.Requester != agentOf(t.priv) || c.Provider != t.provider:
		return nil, mismatch("parties")
	case c.Zone != t.zone:
		return nil, mismatch("zone")
	case !bytes.Equal(c.ResumedFrom, t.resumedFrom):
		return nil, mismatch("resumed-from")
	}
	n := c.ThroughSeq + 1
	if n > uint64(len(t.chunks)) {
		return nil, mismatch("checkpoint covers Parts not yet received")
	}
	if _, err := transcript.VerifyPrefix(c, t.chunks[:n]); err != nil {
		return nil, err
	}
	if t.lastCore != nil {
		if err := transcript.CheckSuccessor(*t.lastCore, c); err != nil {
			return nil, err
		}
	}
	if !t.tolerance.AcceptsCounters(c.Usage.Counters(), observed.Counters()) {
		return nil, ErrOutsideTolerance
	}
	art.Countersign(t.priv)
	out := art.Encode()
	t.lastCheckpoint, t.lastCore = out, &c
	return out, nil
}

// OnReceipt verifies the provider's receipt proposal and countersigns it.
// observed and the timestamps are this requester's own view.
func (t *RequesterTask) OnReceipt(proposal []byte, observed core.Usage, startedMs, endedMs uint64) ([]byte, error) {
	r, err := receipt.Decode(proposal)
	if err != nil {
		return nil, err
	}
	if r.SigProv == nil {
		return nil, mismatch("receipt not signed by the provider")
	}
	v, err := r.Verify()
	if err != nil {
		return nil, err
	}
	c := v.Core
	if c.Task != t.task || c.Requester != agentOf(t.priv) || c.Provider != t.provider {
		return nil, mismatch("task or parties")
	}
	if c.Capability != t.capability {
		return nil, mismatch("capability")
	}
	root := t.builder.Root()
	if c.TranscriptRoot == nil || *c.TranscriptRoot != root {
		return nil, mismatch("transcript root")
	}
	if c.Zone == nil || *c.Zone != t.zone || !bytes.Equal(c.ResumedFrom, t.resumedFrom) {
		return nil, mismatch("zone or resumed-from")
	}
	obs := c
	obs.Metrics, obs.StartedMs, obs.EndedMs = observed, startedMs, endedMs
	if !t.tolerance.Accepts(c, obs) {
		return nil, ErrOutsideTolerance
	}
	r.SigReq = receipt.Sign(t.priv, receipt.RoleRequester, r.CoreBytes)
	return r.Encode(), nil
}
