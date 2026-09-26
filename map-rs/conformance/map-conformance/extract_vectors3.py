"""Extract the v2 vectors (TV-021..TV-023, NV-013/014) from
spec/test-vectors/map-test-vectors.md into src/vectors3.rs (compiled-in
constants, copied verbatim).

Usage (from map-rs/conformance/map-conformance):
  python extract_vectors3.py ../../../spec/test-vectors/map-test-vectors.md src/vectors3.rs
"""
import re, sys
md, out = sys.argv[1], sys.argv[2]
t = open(md, encoding="utf-8").read()

def section(title_prefix):
    i = t.index(title_prefix)
    j = t.find("\n## ", i + 1)
    k = t.find("\n### ", i + 1)
    ends = [e for e in (j, k) if e != -1]
    return t[i:min(ends) if ends else len(t)]

def hexes(sec):
    return [l.strip() for l in sec.splitlines() if re.fullmatch(r"\s*[0-9a-f]{32,}\s*", l)]

tv = hexes(section("## TV-021"))
assert len(tv) == 3, tv  # root, core bytes, artifact
nv13 = hexes(section("### NV-013"))
nv14 = hexes(section("### NV-014"))
assert len(nv13) == 1 and len(nv14) == 1
tv22 = hexes(section("## TV-022"))
assert len(tv22) == 1, tv22
tv24 = hexes(section("## TV-024"))
assert len(tv24) == 2, tv24  # signature entry, envelope
tv25 = hexes(section("## TV-025"))
assert len(tv25) == 8, tv25  # task id, root, then (input, leaf) x 3
tv25_parts = "".join(f'    ("{tv25[2 + 2 * i]}", "{tv25[3 + 2 * i]}"),\n' for i in range(3))
tv23 = re.findall(r"^case (\d\d) (\S+) -- .*\n\s+([0-9a-f]+)\s*$", section("## TV-023"), re.M)
assert len(tv23) == 13 and [int(n) for n, _, _ in tv23] == list(range(1, 14)), tv23
tv23_rs = "".join(f'    ("{v}", "{h}"),\n' for _, v, h in tv23)
rs = f'''//! v2 checkpoint vectors (TV-021, NV-013, NV-014), copied EXACTLY from
//! `spec/test-vectors/map-test-vectors.md` by a script. Do not edit these to
//! make a test pass: regenerate the vectors and re-extract.

/// TV-021 transcript root over the three-chunk prefix (raw 32 bytes).
pub const TV021_ROOT_HEX: &str = "{tv[0]}";
/// TV-021 CheckpointCore bytes.
pub const TV021_CORE_HEX: &str = "{tv[1]}";
/// TV-021 dual-signed CheckpointArtifact.
pub const TV021_ARTIFACT_HEX: &str = "{tv[2]}";
/// TV-021 transcript prefix contents (kind 0, seq 0..=2).
pub const TV021_PREFIX: [&str; 3] = ["Hello, ", "accountable ", "MAP!"];
/// NV-013: TV-021 with a corrupted requester countersignature.
pub const NV013_HEX: &str = "{nv13[0]}";
/// NV-014: validly dual-signed, but key 5 does not match TV-021's prefix.
pub const NV014_HEX: &str = "{nv14[0]}";
/// TV-022: signed A2A Agent Card (canonical JSON bytes), Key B, alg EdDSA.
pub const TV022_CARD_HEX: &str = "{tv22[0]}";
/// TV-024: v2 AgentDescriptor envelope embedding TV-022 (Key B, seq 1, ttl 1 h).
pub const TV024_ENV_HEX: &str = "{tv24[1]}";
/// TV-025: A2A TaskID (nonce 07x16, Key B, "task-1") and transcript root.
pub const TV025_TASK_ID_HEX: &str = "{tv25[0]}";
pub const TV025_ROOT_HEX: &str = "{tv25[1]}";
/// TV-025: (Part input UTF-8 JSON hex, leaf content hex), seq 0..2.
pub const TV025_PARTS: &[(&str, &str)] = &[
{tv25_parts}];
/// TV-023 evaluation time (ms) and skew bound (ms).
pub const TV023_NOW_MS: u64 = 1_790_000_000_000;
pub const TV023_SKEW_MS: u64 = 120_000;
/// TV-023: (expected verdict, receipt hex), cases 01..13 in order.
pub const TV023: &[(&str, &str)] = &[
{tv23_rs}];
'''
open(out, "w", encoding="utf-8", newline="\n").write(rs)
print("wrote", out)
