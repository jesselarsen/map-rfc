use map_identity::Keypair;
use map_pay::codec::AssetAmount;
use map_pay::credit_ledger::*;
use map_pay::{Amount, Asset};

fn usd(m: i64) -> AssetAmount {
    AssetAmount { asset: Asset::new("iso4217:USD").unwrap(), amount: Amount::new(-2, m) }
}

fn main() {
    // --- A. negative-amount pay(): drain the PAYEE's balance ---------------
    let mut op = LedgerOperator::new(Keypair::from_seed([9u8; 32]), -2);
    let victim_kp = Keypair::from_seed([2u8; 32]);
    let attacker = Keypair::from_seed([1u8; 32]).agent_id();
    let victim = victim_kp.agent_id();
    op.credit(&victim, 100_000);
    println!("A) before: attacker={} victim={}", op.balance(&attacker), op.balance(&victim));
    // Attacker is the "payer"; amount is -50000.
    let r = op.pay([7u8; 16], &attacker, &victim, &usd(-50_000), 1);
    println!("A) pay(-50000) -> {:?}", r.as_ref().map(|_| "Ok(entry)"));
    println!("A) after:  attacker={} victim={}", op.balance(&attacker), op.balance(&victim));
    // And the entry VERIFIES as a settlement proof for the invoice:
    if let Ok(entry) = &r {
        let ok = verify_entry(entry, &op.agent_id(), [7u8; 16], &attacker, &victim, &usd(-50_000));
        println!("A) verify_entry on the negative entry -> {ok}");
    }

    // --- B. negative-amount hold(): escrow that pays the PAYER ------------
    let mut op2 = LedgerOperator::new(Keypair::from_seed([9u8; 32]), -2);
    let payer_kp = Keypair::from_seed([1u8; 32]);
    let payer = payer_kp.agent_id();
    let payee = Keypair::from_seed([2u8; 32]).agent_id();
    op2.credit(&payee, 100_000);
    println!("B) before: payer={} payee={} escrow={}", op2.balance(&payer), op2.balance(&payee), op2.escrow_total());
    let h = op2.hold([7u8; 16], &payer, &payee, &usd(-50_000), 100, 1);
    println!("B) hold(-50000) -> {:?}", h.as_ref().map(|_| "Ok(entry)"));
    println!("B) held:   payer={} payee={} escrow={}", op2.balance(&payer), op2.balance(&payee), op2.escrow_total());
    let auth = payer_kp.sign(&release_auth_bytes([7u8; 16], [8u8; 16], &usd(-50_000)));
    let rel = op2.release([8u8; 16], [7u8; 16], &auth, &usd(-50_000), 50);
    println!("B) release(-50000) -> {:?}", rel.as_ref().map(|_| "Ok(entry)"));
    println!("B) after:  payer={} payee={} escrow={}", op2.balance(&payer), op2.balance(&payee), op2.escrow_total());

    // --- C. refund entry asset/amount fidelity (pay 5.2.4) ----------------
    let mut op3 = LedgerOperator::new(Keypair::from_seed([9u8; 32]), -2);
    let p = Keypair::from_seed([1u8; 32]).agent_id();
    let q = Keypair::from_seed([2u8; 32]).agent_id();
    op3.credit(&p, 500);
    op3.hold([7u8; 16], &p, &q, &usd(500), 100, 1).unwrap();
    let rf = op3.refund([7u8; 16], None, 100).unwrap();
    let txt: String = rf.entry_bytes.iter().map(|b| if b.is_ascii_graphic() { *b as char } else { '.' }).collect();
    println!("C) refund entry printable = {txt}");
    println!("C) contains 'credit:ledger' = {}", txt.contains("credit:ledger"));
    println!("C) contains 'iso4217:USD'   = {}", txt.contains("iso4217:USD"));
}
