// SPDX-License-Identifier: GPL-3.0-or-later
//! Authenticated, bounded local bytes. No remote bind and no eval/console RPC.
use crate::{LinkError, Result};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    fmt,
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    time::Duration,
};
type HmacSha256 = Hmac<Sha256>;
const MAGIC: &[u8; 8] = b"ECASL001";
const HEADER: usize = 39;
const DOMAIN: &[u8] = b"ExternalCAS local protocol v1\0";
pub const DEFAULT_MAX_FRAME: usize = 8 * 1024 * 1024;
pub struct PairingSecret([u8; 32]);
impl fmt::Debug for PairingSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PairingSecret([redacted])")
    }
}
impl PairingSecret {
    pub fn generate() -> Result<Self> {
        let mut value = [0; 32];
        getrandom::fill(&mut value)
            .map_err(|_| LinkError::Invalid("OS randomness unavailable".into()))?;
        Ok(Self(value))
    }
    /// A secret is exchanged only through an explicit user-controlled local
    /// pairing action; never place it in logs, URLs or source-controlled files.
    pub fn from_bytes(value: [u8; 32]) -> Result<Self> {
        if value == [0; 32] {
            return Err(LinkError::Invalid("Empty pairing secret".into()));
        }
        Ok(Self(value))
    }
    pub fn pairing_bytes(&self) -> [u8; 32] {
        self.0
    }
}
pub fn new_nonce() -> Result<[u8; 16]> {
    let mut value = [0; 16];
    getrandom::fill(&mut value)
        .map_err(|_| LinkError::Invalid("OS randomness unavailable".into()))?;
    Ok(value)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    DesktopToGame = 0,
    GameToDesktop = 1,
}
pub struct AuthenticatedCodec {
    key: PairingSecret,
    session_nonce: [u8; 16],
    outgoing: Direction,
    send_sequence: u64,
    receive_sequence: u64,
    pub max_frame_bytes: usize,
}
impl AuthenticatedCodec {
    pub fn new(
        key: PairingSecret,
        session_nonce: [u8; 16],
        outgoing: Direction,
        max_frame_bytes: usize,
    ) -> Result<Self> {
        if session_nonce == [0; 16] || max_frame_bytes == 0 || max_frame_bytes > u32::MAX as usize {
            return Err(LinkError::Invalid("Session nonce/frame bound".into()));
        }
        Ok(Self {
            key,
            session_nonce,
            outgoing,
            send_sequence: 0,
            receive_sequence: 0,
            max_frame_bytes,
        })
    }
    pub fn encode<T: Serialize>(&mut self, value: &T) -> Result<Vec<u8>> {
        let mut buffer = BoundedBuffer {
            bytes: Vec::new(),
            limit: self.max_frame_bytes,
            overflowed: false,
        };
        let encoded = serde_json::to_writer(&mut buffer, value);
        if buffer.overflowed {
            return Err(LinkError::Budget("Local protocol frame".into()));
        }
        encoded?;
        let payload = buffer.bytes;
        let sequence = self.send_sequence.checked_add(1).ok_or(LinkError::Replay)?;
        let mut frame = Vec::with_capacity(HEADER + payload.len() + 32);
        frame.extend(MAGIC);
        frame.extend(1u16.to_be_bytes());
        frame.extend(self.session_nonce);
        frame.push(self.outgoing as u8);
        frame.extend(sequence.to_be_bytes());
        frame.extend((payload.len() as u32).to_be_bytes());
        frame.extend(payload);
        let mut mac =
            HmacSha256::new_from_slice(&self.key.0).map_err(|_| LinkError::Authentication)?;
        mac.update(DOMAIN);
        mac.update(&frame);
        frame.extend(mac.finalize().into_bytes());
        self.send_sequence = sequence;
        Ok(frame)
    }
    pub fn write<T: Serialize>(&mut self, writer: &mut impl Write, value: &T) -> Result<()> {
        let frame = self.encode(value)?;
        writer.write_all(&frame)?;
        writer.flush()?;
        Ok(())
    }
    pub fn read<T: for<'de> Deserialize<'de>>(&mut self, reader: &mut impl Read) -> Result<T> {
        let mut header = [0; HEADER];
        reader.read_exact(&mut header)?;
        if &header[..8] != MAGIC
            || header[8..10] != 1u16.to_be_bytes()
            || header[10..26] != self.session_nonce
            || header[26] == self.outgoing as u8
            || header[26] > 1
        {
            return Err(LinkError::Authentication);
        }
        let sequence = u64::from_be_bytes(header[27..35].try_into().unwrap());
        let size = u32::from_be_bytes(header[35..39].try_into().unwrap()) as usize;
        if size > self.max_frame_bytes {
            return Err(LinkError::Budget("Received local protocol frame".into()));
        }
        let mut payload = vec![0; size];
        reader.read_exact(&mut payload)?;
        let mut signature = [0; 32];
        reader.read_exact(&mut signature)?;
        let mut mac =
            HmacSha256::new_from_slice(&self.key.0).map_err(|_| LinkError::Authentication)?;
        mac.update(DOMAIN);
        mac.update(&header);
        mac.update(&payload);
        mac.verify_slice(&signature)
            .map_err(|_| LinkError::Authentication)?;
        if sequence
            != self
                .receive_sequence
                .checked_add(1)
                .ok_or(LinkError::Replay)?
        {
            return Err(LinkError::Replay);
        }
        let value = serde_json::from_slice(&payload)?;
        self.receive_sequence = sequence;
        Ok(value)
    }
}
/// Binding is explicit. The caller schedules accepted socket work off the UI
/// and game threads; timeouts bound an inactive authenticated-peer request.
pub fn bind_loopback(port: u16) -> Result<TcpListener> {
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}
pub fn connect_loopback(address: SocketAddr, timeout: Duration) -> Result<TcpStream> {
    if !address.ip().is_loopback() || timeout.is_zero() {
        return Err(LinkError::Invalid(
            "Explicit loopback address/timeout required".into(),
        ));
    }
    let stream = TcpStream::connect_timeout(&address, timeout)?;
    configure_stream(&stream, timeout)?;
    Ok(stream)
}
pub fn configure_stream(stream: &TcpStream, timeout: Duration) -> Result<()> {
    if !stream.peer_addr()?.ip().is_loopback() || timeout.is_zero() {
        return Err(LinkError::Invalid("Local peer/timeout required".into()));
    }
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.set_nodelay(true)?;
    Ok(())
}

/// Transport carries only typed local requests. Application-level Sim identity
/// and operation status must still be checked by the Rust controller/adapter.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", deny_unknown_fields)]
pub enum ControlMessage {
    Capabilities(crate::Capabilities),
    ObserveSelection {
        sequence: u64,
        identity: Option<crate::SimIdentity>,
    },
    Household {
        members: Vec<crate::HouseholdMember>,
    },
    SnapshotRequest {
        ticket: crate::SelectionTicket,
    },
    Snapshot {
        ticket: crate::SelectionTicket,
        snapshot: crate::Snapshot,
        origin_operation: Option<[u8; 16]>,
    },
    Pause {
        reason: String,
    },
    Disconnect,
    OperationStatus {
        operation_id: [u8; 16],
    },
    Apply(crate::ApplyRequest),
    ApplyAcknowledgment(crate::ApplyAcknowledgment),
    Rollback(crate::RollbackRequest),
    ReadbackRequest(crate::OperationReadbackRequest),
    Readback {
        operation_id: crate::OperationId,
        session_nonce: [u8; 16],
        snapshot: crate::Snapshot,
    },
    Refused {
        operation_id: Option<[u8; 16]>,
        reason: String,
    },
}
struct BoundedBuffer {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}
impl Write for BoundedBuffer {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(input.len())
            .is_none_or(|n| n > self.limit)
        {
            self.overflowed = true;
            return Err(std::io::Error::other("Local frame byte bound"));
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
