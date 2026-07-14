//! MAP Message Types registry, initial contents (draft-map-core-01 §5).
//! Code points are immutable once assigned (ops §3.3).

// 0x0001–0x00FF — Core
pub const HELLO: u16 = 0x0001;
pub const HELLO_ACK: u16 = 0x0002;
pub const PING: u16 = 0x0003;
pub const PONG: u16 = 0x0004;
pub const CLOSE: u16 = 0x0005;
pub const ERROR: u16 = 0x0006;
pub const FLOW_CREDIT: u16 = 0x0007;

// 0x0100–0x01FF — Discovery
pub const DESCRIPTOR: u16 = 0x0100;
pub const STATUS_QUERY: u16 = 0x0101;
pub const STATUS_REPORT: u16 = 0x0102;
pub const CAP_QUERY: u16 = 0x0103;
pub const CAP_RESPONSE: u16 = 0x0104;

// 0x0200–0x02FF — Tasks (0x0206 unassigned)
pub const OFFER: u16 = 0x0200;
pub const QUOTE: u16 = 0x0201;
pub const ASSIGN: u16 = 0x0202;
pub const ACCEPT: u16 = 0x0203;
pub const REJECT: u16 = 0x0204;
pub const CHUNK: u16 = 0x0205;
pub const TOOL_CALL: u16 = 0x0207;
pub const TOOL_GRANT: u16 = 0x0208;
pub const TOOL_RESULT: u16 = 0x0209;
pub const CHECKPOINT: u16 = 0x020A;
pub const CHECKPOINT_ACK: u16 = 0x020B;
pub const CANCEL: u16 = 0x020C;
pub const COMPLETE: u16 = 0x020D;
pub const RESUME: u16 = 0x020E;
pub const RESERVE: u16 = 0x020F;
pub const LEASE: u16 = 0x0210;
pub const LEASE_RELEASE: u16 = 0x0211;

// 0x0300–0x03FF — Trust
pub const RECEIPT: u16 = 0x0300;
pub const RECEIPT_ACK: u16 = 0x0301;
pub const EVIDENCE_QUERY: u16 = 0x0302;
pub const EVIDENCE_SET: u16 = 0x0303;
pub const CHALLENGE: u16 = 0x0304;
pub const CHALLENGE_RESPONSE: u16 = 0x0305;

// 0x0400–0x04FF — Pay
pub const PAY_QUOTE: u16 = 0x0400;
pub const PAY_AGREE: u16 = 0x0401;
pub const PAY_INVOICE: u16 = 0x0402;
pub const PAY_PROOF: u16 = 0x0403;
pub const PAY_RECEIPT: u16 = 0x0404;
pub const PAY_DISPUTE: u16 = 0x0405;
pub const PAY_DISPUTE_EVIDENCE: u16 = 0x0406;

// 0x0500–0x05FF — Identity
pub const ROTATE: u16 = 0x0500;
pub const REVOKE: u16 = 0x0501;
pub const DELEGATE: u16 = 0x0502;

/// Start of the private-use range (never appears in public registries).
pub const PRIVATE_USE_MIN: u16 = 0xF000;

/// Debug/diagnostic name lookup.
pub fn name(t: u16) -> Option<&'static str> {
    Some(match t {
        HELLO => "HELLO",
        HELLO_ACK => "HELLO_ACK",
        PING => "PING",
        PONG => "PONG",
        CLOSE => "CLOSE",
        ERROR => "ERROR",
        FLOW_CREDIT => "FLOW_CREDIT",
        DESCRIPTOR => "DESCRIPTOR",
        STATUS_QUERY => "STATUS_QUERY",
        STATUS_REPORT => "STATUS_REPORT",
        CAP_QUERY => "CAP_QUERY",
        CAP_RESPONSE => "CAP_RESPONSE",
        OFFER => "OFFER",
        QUOTE => "QUOTE",
        ASSIGN => "ASSIGN",
        ACCEPT => "ACCEPT",
        REJECT => "REJECT",
        CHUNK => "CHUNK",
        TOOL_CALL => "TOOL_CALL",
        TOOL_GRANT => "TOOL_GRANT",
        TOOL_RESULT => "TOOL_RESULT",
        CHECKPOINT => "CHECKPOINT",
        CHECKPOINT_ACK => "CHECKPOINT_ACK",
        CANCEL => "CANCEL",
        COMPLETE => "COMPLETE",
        RESUME => "RESUME",
        RESERVE => "RESERVE",
        LEASE => "LEASE",
        LEASE_RELEASE => "LEASE_RELEASE",
        RECEIPT => "RECEIPT",
        RECEIPT_ACK => "RECEIPT_ACK",
        EVIDENCE_QUERY => "EVIDENCE_QUERY",
        EVIDENCE_SET => "EVIDENCE_SET",
        CHALLENGE => "CHALLENGE",
        CHALLENGE_RESPONSE => "CHALLENGE_RESPONSE",
        PAY_QUOTE => "PAY_QUOTE",
        PAY_AGREE => "PAY_AGREE",
        PAY_INVOICE => "PAY_INVOICE",
        PAY_PROOF => "PAY_PROOF",
        PAY_RECEIPT => "PAY_RECEIPT",
        PAY_DISPUTE => "PAY_DISPUTE",
        PAY_DISPUTE_EVIDENCE => "PAY_DISPUTE_EVIDENCE",
        ROTATE => "ROTATE",
        REVOKE => "REVOKE",
        DELEGATE => "DELEGATE",
        _ => return None,
    })
}
