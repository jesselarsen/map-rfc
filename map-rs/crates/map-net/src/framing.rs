//! `/map/1.0.0` stream framing: one MAP envelope per frame, length-prefixed
//! with a big-endian `u32` (TASK-012 FR-2; makes the TASK-005 §13 implicit
//! framing explicit for a stream transport).
//!
//! Read order is size-first (core §6.3): the length prefix is bounds-checked
//! against the negotiated maximum BEFORE any body allocation, so an oversize or
//! hostile prefix can never drive an allocation.

use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use std::io;

/// The 4-byte big-endian length prefix.
pub const LEN_PREFIX_BYTES: usize = 4;

fn oversize(len: usize, max: usize) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("frame length {len} exceeds negotiated maximum {max}"),
    )
}

/// Write one length-prefixed frame and flush. Frames larger than `u32::MAX`
/// are rejected (they cannot occur under any negotiated MAP size).
pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, frame: &[u8]) -> io::Result<()> {
    let len = u32::try_from(frame.len()).map_err(|_| oversize(frame.len(), u32::MAX as usize))?;
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(frame).await?;
    w.flush().await?;
    Ok(())
}

/// Read one length-prefixed frame, rejecting a prefix greater than `max` before
/// allocating the body (DoS bound — FR-2 / core §6.3).
pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R, max: usize) -> io::Result<Vec<u8>> {
    let mut lp = [0u8; LEN_PREFIX_BYTES];
    r.read_exact(&mut lp).await?;
    let len = u32::from_be_bytes(lp) as usize;
    if len > max {
        return Err(oversize(len, max));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use futures::io::Cursor;

    #[test]
    fn roundtrip_multiple_frames() {
        block_on(async {
            let mut buf = Cursor::new(Vec::new());
            write_frame(&mut buf, b"hello").await.unwrap();
            write_frame(&mut buf, b"").await.unwrap(); // empty frame is legal
            write_frame(&mut buf, &[0xABu8; 300]).await.unwrap();

            let mut rd = Cursor::new(buf.into_inner());
            assert_eq!(read_frame(&mut rd, 1024).await.unwrap(), b"hello");
            assert_eq!(read_frame(&mut rd, 1024).await.unwrap(), b"");
            assert_eq!(read_frame(&mut rd, 1024).await.unwrap(), vec![0xAB; 300]);
        });
    }

    #[test]
    fn oversize_prefix_rejected_before_body() {
        block_on(async {
            // A 0x00100000 (1 MiB) prefix with NO body bytes following: if the
            // reader allocated first it would then block/EOF on the body; instead
            // it must reject on the prefix alone.
            let mut framed = 1_048_576u32.to_be_bytes().to_vec();
            framed.extend_from_slice(b"tiny");
            let mut rd = Cursor::new(framed);
            let err = read_frame(&mut rd, 262_144).await.unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        });
    }

    #[test]
    fn truncated_body_is_error_not_panic() {
        block_on(async {
            let mut framed = 10u32.to_be_bytes().to_vec();
            framed.extend_from_slice(b"only4"); // 5 < 10 promised
            let mut rd = Cursor::new(framed);
            assert!(read_frame(&mut rd, 1024).await.is_err());
        });
    }
}
