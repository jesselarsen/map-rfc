use map_core::cbor::Value;
use map_core::envelope::{seal, Body};
use map_core::{msg_type, MsgId};
use map_identity::Keypair;
use map_pay::codec::{AssetAmount, MethodOffer, PayAgree, PayAgreeRef, PayInvoice, PayQuote, Usage as PayUsage, pay_ref_digest};
use map_pay::dispute::*;
use map_pay::{Amount, Asset};
use map_tasks::{ChunkKind, TaskId};

const TASK: [u8; 32] = [0xE; 32];
const QID: [u8; 16] = [0x50; 16];
const INV: [u8; 16] = [0x60; 16];
const TS: u64 = 1_780_000_000_000;

fn payer() -> Keypair { Keypair::from_seed([1; 32]) }
fn payee() -> Keypair { Keypair::from_seed([2; 32]) }
fn parties() -> Parties { Parties { payer: payer().agent_id(), payee: payee().agent_id(), operator: Some(Keypair::from_seed([4;32]).agent_id()) } }
fn usd(m: i64) -> AssetAmount { AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-2, m) } }

fn item(kp: &Keypair, kind: u64, mt: u16, content: Value, msg_id: u8) -> EvidenceItem {
    let body = Body { version: 0x0100, msg_type: mt, msg_id: MsgId([msg_id; 16]), timestamp_ms: TS,
        recipient: None, ttl_ms: None, session_id: None, seq: None, content };
    EvidenceItem { kind, bytes: seal(kp, &body, &[]).unwrap(), cb: None }
}
fn quote(terms: u64) -> PayQuote {
    PayQuote { quote_id: QID, pricing_model: 1, rate_table: vec![(1, usd(3)), (2, usd(5))],
        total: Some(usd(500)), methods: vec![MethodOffer { uri: "pay:credit-ledger".into(), params: vec![] }],
        expiry_ms: TS + 60_000, terms: Some(terms), ext: None, task_binding: Some(TASK) }
}
fn anchored(terms: u64, claim: u64) -> (PayDispute, Vec<EvidenceItem>) {
    let q = item(&payee(), EV_QUOTE, msg_type::PAY_QUOTE, quote(terms).to_value(), 0x10);
    let agree = PayAgree { quote_id: QID, quote_digest: pay_ref_digest(&q.bytes), method: "pay:credit-ledger".into(), authorized_cap: None };
    let a = item(&payer(), EV_AGREE, msg_type::PAY_AGREE, agree.to_value(), 0x11);
    let d = PayDispute { dispute_id: [0xD; 16], invoice_id: INV, claim,
        agree_ref: PayAgreeRef { quote_id: QID, digest: pay_ref_digest(&a.bytes) },
        task: Some(TASK), statement: None, timestamp_ms: TS };
    (d, vec![q, a])
}
fn checkpoint_item(usage: PayUsage, seq: u64) -> EvidenceItem {
    let root = [0x77; 32];
    let sig = payer().sign(&checkpoint_ack_signing_input(&TASK, seq, &root));
    let cp = CountersignedCheckpoint { task: TASK, through_seq: seq, root, usage: Some(usage), countersig: sig };
    EvidenceItem { kind: EV_CHECKPOINT, bytes: cp.to_bytes(), cb: None }
}
fn complete_item(kp: &Keypair, root: [u8; 32], msg_id: u8) -> EvidenceItem {
    let c = map_tasks::msg::Complete { task: TaskId(TASK), outcome: 0, transcript_root: root, usage: Default::default(), ext: None };
    item(kp, EV_COMPLETE, msg_type::COMPLETE, c.to_value(), msg_id)
}
fn v(d: &PayDispute, cl: &[EvidenceItem], rs: &[EvidenceItem]) -> Verdict {
    verify_dispute(d, cl, rs, &parties(), &DisputeConfig::default())
}

fn main() {
    // 1) claim 3 overbilling against a TERMS-1 prepaid invoice equal to the quoted
    //    total -- pay 5.2.2 says claim 3 "does not apply".
    let (d, mut cl) = anchored(1, CLAIM_OVERBILLING);
    let inv = PayInvoice { invoice_id: INV, quote_id: QID, amount: usd(500), watermark: None,
        method_payload: vec![], timestamp_ms: TS, phase: None };
    cl.push(item(&payee(), EV_INVOICE, msg_type::PAY_INVOICE, inv.to_value(), 0x12));
    cl.push(checkpoint_item(PayUsage { tokens_in: 10, tokens_out: 20, ttft_ms: 0, wall_ms: 0 }, 4));
    println!("1) terms-1 prepaid, invoice==quoted total 500, checkpoint bills 130 -> {:?}", v(&d, &cl, &[]));

    // 2) claim 1 output-mismatch where the PAYER forges the "delivered" chunk.
    let (d1, mut cl1) = anchored(0, CLAIM_OUTPUT_MISMATCH);
    let real = map_tasks::msg::ChunkMsg { task: TaskId(TASK), seq: 0, kind: ChunkKind::OutputText, content: b"the real delivered output".to_vec(), fin: false };
    // The honest provider's COMPLETE commits to the real transcript root.
    let mut tb = map_tasks::TranscriptBuilder::new();
    tb.push(&map_tasks::Chunk { seq: 0, kind: ChunkKind::OutputText, content: b"the real delivered output".to_vec() });
    let honest_root = tb.root();
    let _ = real;
    // The payer fabricates a CHUNK envelope signed by ITSELF and presents it.
    let forged = map_tasks::msg::ChunkMsg { task: TaskId(TASK), seq: 0, kind: ChunkKind::OutputText, content: b"tampered".to_vec(), fin: false };
    cl1.push(item(&payer(), EV_CHUNK, msg_type::CHUNK, forged.to_value(), 0x20));
    cl1.push(complete_item(&payee(), honest_root, 0x21));
    println!("2) claim 1 with a PAYER-signed forged chunk vs honest root -> {:?}", v(&d1, &cl1, &[]));

    // 3) claim 0 non-delivery: payee delivered NOTHING but signs a COMPLETE with a
    //    made-up root. Payer presents its settled receipt.
    let (d0, mut cl0) = anchored(1, CLAIM_NON_DELIVERY);
    let rcpt = map_pay::codec::PayReceipt { invoice_id: INV, amount: usd(500), timestamp_ms: TS, proof_digest: None, kind: None };
    cl0.push(item(&payee(), EV_RECEIPT, msg_type::PAY_RECEIPT, rcpt.to_value(), 0x30));
    println!("3a) claim 0, payee produces nothing            -> {:?}", v(&d0, &cl0, &[]));
    let bogus = complete_item(&payee(), [0xAA; 32], 0x31);
    println!("3b) claim 0, payee mints one bogus COMPLETE     -> {:?}", v(&d0, &cl0, &[bogus]));
    // and the payer's fallback claim 1 with no chunks to show:
    let (d1b, mut cl1b) = anchored(1, CLAIM_OUTPUT_MISMATCH);
    cl1b.push(complete_item(&payee(), [0xAA; 32], 0x32));
    println!("3c) payer's fallback claim 1, no chunks to show -> {:?}", v(&d1b, &cl1b, &[]));
}
