//! Binding primitives (spec/binding-a2a.md §3–§5): the metadata carriage key,
//! TaskID derivation, and the A2A Part → transcript leaf mapping.

use crate::{b64, jcs};
use map_core::cbor::{encode, Value as Cbor};
use map_core::TaskId;
use map_identity::AgentId;
use map_transcript::{Chunk, ChunkKind};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// All MAP artifacts ride under this single `metadata` key (A2A-7).
pub const META_KEY: &str = "map/v2";

/// Binding error codes carried as `metadata."map/v2".error.code` (A2A-26).
pub mod codes {
    pub const COUNTERSIGNATURE_REFUSED: u64 = 1;
    pub const ROOT_MISMATCH: u64 = 2;
    pub const ZONE_VIOLATION: u64 = 3;
    pub const BAD_CHECKPOINT: u64 = 4;
    pub const TASK_ID_MISMATCH: u64 = 5;
    pub const UNSUPPORTED_EXTENSION_VERSION: u64 = 6;
    pub const ZONE_MISMATCH: u64 = 7;
}

/// A2A-8 (decision D6): `TaskID = SHA-256(enc(["MAPTaskID", requesterNonce,
/// providerAgentID, a2aTaskId]))`, with the A2A task id as its UTF-8 bytes.
/// The requester's nonce is fixed before the provider allocates the A2A id,
/// and the provider's AgentID binds the value to one provider.
pub fn task_id(requester_nonce: &[u8; 16], provider: &AgentId, a2a_task_id: &str) -> TaskId {
    let input = encode(&Cbor::Array(vec![
        Cbor::Text("MAPTaskID".into()),
        Cbor::Bytes(requester_nonce.to_vec()),
        Cbor::Bytes(provider.as_bytes().to_vec()),
        Cbor::Bytes(a2a_task_id.as_bytes().to_vec()),
    ]))
    .expect("array of primitives encodes");
    TaskId(Sha256::digest(&input).into())
}

/// A2A-14: a leaf's content is the RFC 8785 canonical JSON of the Part
/// EXCLUDING its `metadata` member (MAP artifacts ride there, so including it
/// would make the transcript depend on the evidence about it).
pub fn part_leaf_content(part: &Value) -> Result<Vec<u8>, jcs::JcsError> {
    let mut p = part.clone();
    if let Some(m) = p.as_object_mut() {
        m.remove("metadata");
    }
    jcs::canonicalize(&p)
}

/// A2A-12/13: the transcript chunk for the `seq`-th delivered artifact Part
/// (kind 0 = artifact Part).
pub fn part_chunk(seq: u64, part: &Value) -> Result<Chunk, jcs::JcsError> {
    Ok(Chunk { seq, kind: ChunkKind::OutputText, content: part_leaf_content(part)? })
}

/// Read the MAP metadata object from an A2A object (Message, Task, event).
pub fn map_meta(obj: &Value) -> Option<&Map<String, Value>> {
    obj.get("metadata")?.get(META_KEY)?.as_object()
}

/// Set `metadata."map/v2".<field>` on an A2A object, creating containers.
pub fn set_map_meta(obj: &mut Value, field: &str, value: Value) {
    let root = obj.as_object_mut().expect("A2A objects are JSON objects");
    let meta = root.entry("metadata").or_insert_with(|| Value::Object(Map::new()));
    let map = meta
        .as_object_mut()
        .expect("metadata is an object")
        .entry(META_KEY)
        .or_insert_with(|| Value::Object(Map::new()));
    map.as_object_mut().expect("map/v2 is an object").insert(field.to_string(), value);
}

/// base64url of CBOR artifact bytes, as carried in metadata (A2A-7).
pub fn artifact_to_json(bytes: &[u8]) -> Value {
    Value::String(b64::encode(bytes))
}

pub fn artifact_from_json(v: &Value) -> Option<Vec<u8>> {
    b64::decode(v.as_str()?).ok()
}

/// A binding error object `{"code": n, "detail": "..."}` (A2A-26).
pub fn error_json(code: u64, detail: &str) -> Value {
    serde_json::json!({"code": code, "detail": detail})
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_identity::Keypair;
    use serde_json::json;

    #[test]
    fn task_id_binds_all_three_inputs() {
        let b = Keypair::from_seed([2; 32]).agent_id();
        let c = Keypair::from_seed([3; 32]).agent_id();
        let t = task_id(&[1; 16], &b, "task-1");
        assert_eq!(t, task_id(&[1; 16], &b, "task-1"));
        assert_ne!(t, task_id(&[2; 16], &b, "task-1"));
        assert_ne!(t, task_id(&[1; 16], &c, "task-1"));
        assert_ne!(t, task_id(&[1; 16], &b, "task-2"));
    }

    #[test]
    fn leaf_content_ignores_metadata_and_member_order() {
        let a = json!({"text": "hello", "metadata": {"map/v2": {"checkpoint": "x"}}});
        let b = json!({"text": "hello"});
        assert_eq!(part_leaf_content(&a).unwrap(), part_leaf_content(&b).unwrap());
        let c = json!({"mediaType": "text/plain", "text": "hi"});
        let d: Value = serde_json::from_str(r#"{"text":"hi","mediaType":"text/plain"}"#).unwrap();
        assert_eq!(part_leaf_content(&c).unwrap(), part_leaf_content(&d).unwrap());
        assert_ne!(part_leaf_content(&b).unwrap(), part_leaf_content(&json!({"text": "hellO"})).unwrap());
    }

    #[test]
    fn metadata_roundtrip() {
        let mut msg = json!({"messageId": "m1", "role": "ROLE_USER", "parts": []});
        set_map_meta(&mut msg, "zone", json!(2));
        set_map_meta(&mut msg, "checkpoint", artifact_to_json(&[1, 2, 3]));
        let m = map_meta(&msg).unwrap();
        assert_eq!(m["zone"], json!(2));
        assert_eq!(artifact_from_json(&m["checkpoint"]).unwrap(), vec![1, 2, 3]);
    }
}
