//! TV-025: the A2A binding's TaskID derivation (A2A-8) and Part → transcript
//! leaf rule (A2A-12..14). The Part inputs are non-canonical JSON carrying a
//! `metadata` member the leaf rule must drop; the leaves and root come from
//! the Python generator.

use crate::positive::key_b;
use crate::vectors3::{TV025_PARTS, TV025_ROOT_HEX, TV025_TASK_ID_HEX};
use crate::{assert_bytes, from_hex};
use map_a2a::binding::{part_chunk, part_leaf_content, task_id};
use map_transcript::TranscriptBuilder;

pub fn case_tv025_binding() -> Result<(), String> {
    assert_bytes(TV025_TASK_ID_HEX, &task_id(&[7; 16], &key_b().agent_id(), "task-1").0)?;
    let mut b = TranscriptBuilder::new();
    for (i, (input, leaf)) in TV025_PARTS.iter().enumerate() {
        let part: serde_json::Value =
            serde_json::from_slice(&from_hex(input)?).map_err(|e| format!("part {i}: {e}"))?;
        assert_bytes(leaf, &part_leaf_content(&part).map_err(|e| format!("part {i}: {e:?}"))?)
            .map_err(|e| format!("part {i} leaf: {e}"))?;
        b.push(&part_chunk(i as u64, &part).map_err(|e| format!("part {i}: {e:?}"))?);
    }
    assert_bytes(TV025_ROOT_HEX, &b.root())
}
