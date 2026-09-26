package discovery

// v2 AgentDescriptor (spec/layer.md §11.2), mirroring Rust
// map-discovery::v2: the stable-plane record that embeds the provider's A2A
// Agent Card, so one claim is signed twice (COSE over the whole descriptor,
// JWS over the card). This package treats the card as opaque bytes; the card
// checks (D-4: the card's JWS key is the subject AgentID) live in the A2A
// binding, keeping JSON out of discovery.
//
// DescriptorCore keys: 1 AgentID, 2 seq, 3 endpoints, 4 [CapEntry],
// 5 accepted zones, 6 card (RFC 8785 JCS bytes), 7 card signature (JCS bytes
// of the A2A AgentCardSignature object), ?12 validity (ms).

import (
	"fmt"
	"strings"

	"github.com/jesselarsen/map-rfc/map-go/cbor"
	"github.com/jesselarsen/map-rfc/map-go/identity"
)

// MsgDescriptorV2 is the v2 descriptor message type.
const MsgDescriptorV2 = 0x0110

// CapPrefix is the required capability URI prefix (layer §11.3).
const CapPrefix = "map:cap:"

// DescriptorV2 is the v2 descriptor content.
type DescriptorV2 struct {
	Agent         identity.AgentID
	Seq           uint64
	Endpoints     []string
	Capabilities  []string
	Zones         []uint8
	CardJCS       []byte
	CardSignature []byte
	ValidityMs    *uint64
}

// ToValue encodes the descriptor map.
func (d DescriptorV2) ToValue() cbor.Value {
	eps := make(cbor.Array, len(d.Endpoints))
	for i, e := range d.Endpoints {
		eps[i] = cbor.Text(e)
	}
	caps := make(cbor.Array, len(d.Capabilities))
	for i, c := range d.Capabilities {
		caps[i] = cbor.Map{{K: cbor.Uint(1), V: cbor.Text(c)}}
	}
	zones := make(cbor.Array, len(d.Zones))
	for i, z := range d.Zones {
		zones[i] = cbor.Uint(z)
	}
	m := cbor.Map{
		{K: cbor.Uint(1), V: cbor.Bytes(d.Agent[:])},
		{K: cbor.Uint(2), V: cbor.Uint(d.Seq)},
		{K: cbor.Uint(3), V: eps},
		{K: cbor.Uint(4), V: caps},
		{K: cbor.Uint(5), V: zones},
		{K: cbor.Uint(6), V: cbor.Bytes(d.CardJCS)},
		{K: cbor.Uint(7), V: cbor.Bytes(d.CardSignature)},
	}
	if d.ValidityMs != nil {
		m = append(m, cbor.Pair{K: cbor.Uint(12), V: cbor.Uint(*d.ValidityMs)})
	}
	return m
}

func badV2(k uint64) error { return fmt.Errorf("descriptor v2: field %d missing or invalid", k) }

// DescriptorV2FromValue decodes and validates shape: non-empty endpoints,
// capabilities (each a map:cap: URI) and zones (each <= 3).
func DescriptorV2FromValue(v cbor.Value) (DescriptorV2, error) {
	var d DescriptorV2
	m, ok := v.(cbor.Map)
	if !ok {
		return d, fmt.Errorf("descriptor v2: not a map")
	}
	get := func(k uint64) (cbor.Value, error) {
		x, ok := m.Get(k)
		if !ok {
			return nil, badV2(k)
		}
		return x, nil
	}
	x, err := get(1)
	if err != nil {
		return d, err
	}
	b, ok := x.(cbor.Bytes)
	if !ok {
		return d, badV2(1)
	}
	if d.Agent, err = identity.Parse(b); err != nil {
		return d, badV2(1)
	}
	if x, err = get(2); err != nil {
		return d, err
	}
	seq, ok := x.(cbor.Uint)
	if !ok {
		return d, badV2(2)
	}
	d.Seq = uint64(seq)
	arr := func(k uint64) (cbor.Array, error) {
		x, err := get(k)
		if err != nil {
			return nil, err
		}
		a, ok := x.(cbor.Array)
		if !ok || len(a) == 0 {
			return nil, badV2(k)
		}
		return a, nil
	}
	eps, err := arr(3)
	if err != nil {
		return d, err
	}
	for _, e := range eps {
		s, ok := e.(cbor.Text)
		if !ok {
			return d, badV2(3)
		}
		d.Endpoints = append(d.Endpoints, string(s))
	}
	caps, err := arr(4)
	if err != nil {
		return d, err
	}
	for _, c := range caps {
		cm, ok := c.(cbor.Map)
		if !ok {
			return d, badV2(4)
		}
		u, _ := cm.Get(1)
		s, ok := u.(cbor.Text)
		if !ok || !strings.HasPrefix(string(s), CapPrefix) || len(s) == len(CapPrefix) {
			return d, badV2(4)
		}
		d.Capabilities = append(d.Capabilities, string(s))
	}
	zones, err := arr(5)
	if err != nil {
		return d, err
	}
	for _, z := range zones {
		u, ok := z.(cbor.Uint)
		if !ok || u > 3 {
			return d, badV2(5)
		}
		d.Zones = append(d.Zones, uint8(u))
	}
	for _, k := range []uint64{6, 7} {
		x, err := get(k)
		if err != nil {
			return d, err
		}
		b, ok := x.(cbor.Bytes)
		if !ok {
			return d, badV2(k)
		}
		if k == 6 {
			d.CardJCS = b
		} else {
			d.CardSignature = b
		}
	}
	if x, ok := m.Get(12); ok {
		u, isU := x.(cbor.Uint)
		if !isU {
			return d, badV2(12)
		}
		vms := uint64(u)
		d.ValidityMs = &vms
	}
	return d, nil
}
