package session

import "github.com/jesselarsen/map-rfc/map-go/identity"

// Anti-replay state (core §7). Both rejects map to wire code 1003.

// DefaultSkewMs is the ±clock-skew window (core §4.3 rule 5).
const DefaultSkewMs = 120_000

// DefaultWindowMs is the msg-id uniqueness cache lifetime (core §7.1).
const DefaultWindowMs = 600_000

// ReplayCheck is the outcome of ReplayCache.Check.
type ReplayCheck int

const (
	// Fresh: unseen and in-window; recorded.
	Fresh ReplayCheck = iota
	// Replayed: this (sender, msg-id) was already seen in the window.
	Replayed
	// OutOfWindow: the message timestamp is outside ±skew.
	OutOfWindow
)

type replayKey struct {
	sender identity.AgentID
	msgID  [16]byte
}

// ReplayCache enforces per-(sender, msg-id) uniqueness within a timestamp
// window (core §7.1).
//
// Expiry is amortized O(1): entries are also queued in arrival order, and
// each Check pops only the expired head of the queue. The v1 cache scanned
// the whole map on every check, which is quadratic under load (finding A8).
type ReplayCache struct {
	skewMs   uint64
	windowMs uint64
	seen     map[replayKey]uint64
	order    []arrival // arrival order
	head     int       // order[head:] is live
}

type arrival struct {
	key replayKey
	at  uint64
}

// NewReplayCache builds a cache with the default skew/window.
func NewReplayCache() *ReplayCache {
	return &ReplayCache{skewMs: DefaultSkewMs, windowMs: DefaultWindowMs, seen: map[replayKey]uint64{}}
}

// Len is the number of ids currently remembered.
func (c *ReplayCache) Len() int { return len(c.seen) }

// Check is check-and-record: a Fresh result records the id. `msgTsMs` is the
// envelope timestamp; `nowMs` the local clock.
func (c *ReplayCache) Check(sender identity.AgentID, msgID []byte, msgTsMs, nowMs uint64) ReplayCheck {
	lo := saturatingSub(nowMs, c.skewMs)
	hi := nowMs + c.skewMs
	if msgTsMs < lo || msgTsMs > hi {
		return OutOfWindow
	}
	cutoff := saturatingSub(nowMs, c.windowMs)
	for c.head < len(c.order) && c.order[c.head].at < cutoff {
		a := c.order[c.head]
		if c.seen[a.key] == a.at {
			delete(c.seen, a.key)
		}
		c.head++
	}
	if c.head > 1024 && c.head*2 > len(c.order) {
		c.order = append(c.order[:0:0], c.order[c.head:]...) // amortized compaction
		c.head = 0
	}
	var key replayKey
	key.sender = sender
	copy(key.msgID[:], msgID)
	if _, ok := c.seen[key]; ok {
		return Replayed
	}
	c.seen[key] = nowMs
	c.order = append(c.order, arrival{key, nowMs})
	return Fresh
}

func saturatingSub(a, b uint64) uint64 {
	if b > a {
		return 0
	}
	return a - b
}

// SeqTracker enforces dense monotonic sequence numbers from 0 per direction
// (core §7.2). Anything else is rejected (1003).
type SeqTracker struct {
	next uint64
}

// Accept advances the tracker iff seq is exactly the expected next value.
func (t *SeqTracker) Accept(seq uint64) bool {
	if seq == t.next {
		t.next++
		return true
	}
	return false
}

// Expected returns the next sequence number this direction will accept.
func (t *SeqTracker) Expected() uint64 { return t.next }
