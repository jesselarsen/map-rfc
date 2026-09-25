//! MAP conformance suite (draft-map-ops-01 §6). A green run of this crate IS
//! the conformance statement for reference implementation #1: it reproduces the
//! normative vectors TV-001/TV-002 byte-for-byte and rejects the ops §6.2
//! catalog of malformed/hostile inputs with the correct error class.
//!
//! Every check is a pure `case_*() -> Result<(), String>` function; [`run_all`]
//! runs them all as data (no case aborts the others), and the `#[test]`
//! wrappers assert each case so `cargo test` pinpoints failures. This is the
//! executable statement of invariant I1 (verify received bytes) and the
//! deterministic-CBOR profile — if a positive case can only pass by
//! re-serializing before verification, that is a critical map-core bug (§11):
//! stop and escalate, do not adapt the vector.

pub mod checkpoint;
pub mod extended;
pub mod negative;
pub mod positive;
pub mod vectors;
pub mod vectors2;
pub mod vectors3;

/// Programmatic result for a future CLI/CI (FR-7).
#[derive(Debug, Default)]
pub struct Report {
    pub passed: Vec<&'static str>,
    pub failed: Vec<(&'static str, String)>,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        self.failed.is_empty()
    }
    pub fn total(&self) -> usize {
        self.passed.len() + self.failed.len()
    }
}

/// Every conformance case, in FR order.
pub fn cases() -> Vec<(&'static str, fn() -> Result<(), String>)> {
    vec![
        ("FR-1 key derivation", positive::case_fr1_keys),
        ("TV-001 produce", positive::case_tv001_produce),
        ("TV-001 consume", positive::case_tv001_consume),
        ("TV-002 produce", positive::case_tv002_produce),
        ("TV-002 consume", positive::case_tv002_consume),
        ("N1 tampered payload", negative::case_n1_tampered_payload),
        ("N2 wrong external_aad", negative::case_n2_wrong_aad),
        ("N3 non-canonical CBOR", negative::case_n3_non_canonical),
        ("N4 truncated envelope", negative::case_n4_truncated),
        ("N5 wrong outer tag", negative::case_n5_wrong_tag),
        ("N6 stale descriptor seq", negative::case_n6_stale_seq),
        ("N7 wrong-shard key", negative::case_n7_wrong_shard),
        ("N8 duplicate receipt", negative::case_n8_duplicate_receipt),
        ("N9 swapped roles", negative::case_n9_swapped_roles),
        ("N10 wrong role domain", negative::case_n10_role_domain),
        // Envelope/HELLO/ERROR/descriptor vectors and the derived constants.
        // Task, pay, dispute and delegation vectors are RETIRED(v1) with their
        // message types (spec/test-vectors/map-test-vectors.md).
        ("TV-003 HELLO_ACK", extended::case_tv003_hello_ack),
        ("TV-004 ERROR", extended::case_tv004_error),
        ("TV-005 DESCRIPTOR", extended::case_tv005_descriptor),
        ("Derived constants", extended::case_derived_constants),
        ("NV-001 tampered", extended::case_nv001_tampered),
        ("NV-002 wrong tag", extended::case_nv002_wrong_tag),
        ("NV-003 non-canonical", extended::case_nv003_non_canonical),
        ("NV-004 stale seq", extended::case_nv004_stale_seq),
        ("NV-005 swapped sigs", extended::case_nv005_swapped_sigs),
        // v2 (layer §7): countersigned checkpoints and resumption.
        ("TV-021 CheckpointArtifact produce", checkpoint::case_tv021_produce),
        ("TV-021 CheckpointArtifact consume", checkpoint::case_tv021_consume),
        ("NV-013 bad countersignature", checkpoint::case_nv013_bad_countersig),
        ("NV-014 root mismatch on resume", checkpoint::case_nv014_root_mismatch),
    ]
}

/// Run every case, collecting outcomes as data (§6/§10 — never aborts).
pub fn run_all() -> Report {
    let mut report = Report::default();
    for (id, case) in cases() {
        match case() {
            Ok(()) => report.passed.push(id),
            Err(why) => report.failed.push((id, why)),
        }
    }
    report
}

// --- shared helpers --------------------------------------------------------

/// Decode a compiled-in vector hex constant.
pub(crate) fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    hex::decode(s).map_err(|e| format!("bad vector hex: {e}"))
}

/// Byte-compare against an expected hex string, reporting length and the first
/// differing offset (§7).
pub(crate) fn assert_bytes(expected_hex: &str, got: &[u8]) -> Result<(), String> {
    let exp = from_hex(expected_hex)?;
    if exp.len() != got.len() {
        return Err(format!(
            "length mismatch: expected {} bytes, got {}",
            exp.len(),
            got.len()
        ));
    }
    for (i, (e, g)) in exp.iter().zip(got).enumerate() {
        if e != g {
            return Err(format!(
                "byte mismatch at offset {i}: expected {e:02x}, got {g:02x}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // One #[test] per case so `cargo test` names the failing case.
    macro_rules! case_test {
        ($name:ident, $case:path) => {
            #[test]
            fn $name() {
                if let Err(why) = $case() {
                    panic!("{why}");
                }
            }
        };
    }

    case_test!(fr1_keys, positive::case_fr1_keys);
    case_test!(tv001_produce, positive::case_tv001_produce);
    case_test!(tv001_consume, positive::case_tv001_consume);
    case_test!(tv002_produce, positive::case_tv002_produce);
    case_test!(tv002_consume, positive::case_tv002_consume);
    case_test!(n1_tampered_payload, negative::case_n1_tampered_payload);
    case_test!(n2_wrong_aad, negative::case_n2_wrong_aad);
    case_test!(n3_non_canonical, negative::case_n3_non_canonical);
    case_test!(n4_truncated, negative::case_n4_truncated);
    case_test!(n5_wrong_tag, negative::case_n5_wrong_tag);
    case_test!(n6_stale_seq, negative::case_n6_stale_seq);
    case_test!(n7_wrong_shard, negative::case_n7_wrong_shard);
    case_test!(n8_duplicate_receipt, negative::case_n8_duplicate_receipt);
    case_test!(n9_swapped_roles, negative::case_n9_swapped_roles);
    case_test!(n10_role_domain, negative::case_n10_role_domain);
    case_test!(tv003_hello_ack, extended::case_tv003_hello_ack);
    case_test!(tv004_error, extended::case_tv004_error);
    case_test!(tv005_descriptor, extended::case_tv005_descriptor);
    case_test!(derived_constants, extended::case_derived_constants);
    case_test!(nv001_tampered, extended::case_nv001_tampered);
    case_test!(nv002_wrong_tag, extended::case_nv002_wrong_tag);
    case_test!(nv003_non_canonical, extended::case_nv003_non_canonical);
    case_test!(nv004_stale_seq, extended::case_nv004_stale_seq);
    case_test!(nv005_swapped_sigs, extended::case_nv005_swapped_sigs);
    case_test!(tv021_produce, checkpoint::case_tv021_produce);
    case_test!(tv021_consume, checkpoint::case_tv021_consume);
    case_test!(nv013_bad_countersig, checkpoint::case_nv013_bad_countersig);
    case_test!(nv014_root_mismatch, checkpoint::case_nv014_root_mismatch);

    #[test]
    fn run_all_is_green_and_complete() {
        let report = run_all();
        assert!(
            report.is_ok(),
            "conformance failures: {:?}",
            report.failed
        );
        assert_eq!(report.passed.len(), 28);
        assert_eq!(report.total(), 28);
    }
}
