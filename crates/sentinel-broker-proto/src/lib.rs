// SPDX-FileCopyrightText: 2026 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Typed IPC protocol for the Sentinel privilege-separation broker.
//!
//! # Why a broker (the architecture this crate anchors)
//!
//! Today `pam_sentinel.so` makes the "remember" decision and owns the
//! root timestamp store **in-process**, inside whatever privileged
//! binary (`sudo`, `polkit-agent-helper-1`, `su`) `dlopen`s it. That
//! binary's whole address space is the blast radius.
//!
//! The hardening target (research-blueprinted; the `pam_sss` /
//! OpenSSH-monitor model) splits that in two:
//!
//! * a **thin PAM shim** — relays the request over a Unix socket and
//!   does nothing privileged itself; and
//! * a **long-lived, sandboxed root broker** (`sentinel-broker`) — owns
//!   the timestamp store, the remember decision, and (future) a per-boot
//!   HMAC key, behind seccomp + dropped capabilities + `forbid(unsafe)`.
//!
//! This crate is the **typed wire contract** between the two. It is the
//! one piece both ends compile, so the request/response shape can't
//! drift. It is `#![forbid(unsafe_code)]` and pulls only `serde` +
//! `postcard` (compact binary) + `thiserror`.
//!
//! # Trust & framing
//!
//! The broker authenticates peers out-of-band via `SO_PEERCRED`
//! (uid 0 only — only the root PAM shim connects). Even so, the codec is
//! defensive: every frame is length-prefixed (`u32` LE) and **bounded by
//! [`MAX_FRAME_LEN`] before any allocation**, so a bogus length can't
//! OOM the root daemon. Decoding is fail-closed — the shim treats any
//! [`Response::Error`] or transport error as "not fresh / not recorded",
//! i.e. it falls back to showing the dialog.

#![forbid(unsafe_code)]

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Wire protocol version. Bump on any breaking change to the message
/// shapes; the broker reports its version in [`Response::Pong`].
pub const PROTOCOL_VERSION: u16 = 1;

/// Hard cap on a single framed message. Messages are tiny (a couple of
/// `u32`s plus a service name and a command line), so 64 KiB is already
/// generous; the cap exists to bound allocation in the root broker when
/// a peer sends a hostile length prefix.
pub const MAX_FRAME_LEN: usize = 64 * 1024;

/// Identity + target a remember grant is bound to. Mirrors the timestamp
/// store's binding: the human `loginuid`, the kernel audit `sessionid`,
/// the PAM `service`, and the **full** elevated command (not just the
/// program — see `pam_sentinel::proc_info::ProcessInfo::remember_command`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RememberKey {
    pub loginuid: u32,
    pub sessionid: u32,
    pub service: String,
    pub command: String,
}

impl RememberKey {
    /// A grant may only be recorded/trusted when it is tied to a real
    /// login session **and** a concrete command. Unbindable keys
    /// (`loginuid`/`sessionid == u32::MAX`, or an empty command) must be
    /// rejected by the broker before it touches the store — this is the
    /// IPC-side mirror of the store's own `is_bindable`, so an unbound
    /// grant can't even be acted on.
    pub fn is_bindable(&self) -> bool {
        self.loginuid != u32::MAX && self.sessionid != u32::MAX && !self.command.is_empty()
    }
}

/// A freshness query: is there a live grant for `key` within `ttl_secs`?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RememberQuery {
    pub key: RememberKey,
    pub ttl_secs: u32,
}

/// Shim → broker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    /// Is there a fresh remember grant for this key within the ttl?
    CheckRemember(RememberQuery),
    /// Record/refresh a grant — sent only after an opt-in Allow.
    RecordRemember(RememberKey),
    /// Liveness/version probe.
    Ping,
}

/// Broker → shim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    /// Result of [`Request::CheckRemember`].
    Remember { fresh: bool },
    /// [`Request::RecordRemember`] acknowledged.
    Recorded,
    /// [`Request::Ping`] reply, carrying the broker's [`PROTOCOL_VERSION`].
    Pong { protocol: u16 },
    /// The broker refused or failed. The shim treats this fail-closed
    /// (as not-fresh / not-recorded) and falls back to the dialog.
    Error(String),
}

/// Protocol / transport errors.
#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("frame too large: {0} bytes (max {MAX_FRAME_LEN})")]
    TooLarge(usize),
    #[error("encode: {0}")]
    Encode(postcard::Error),
    #[error("decode: {0}")]
    Decode(postcard::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Serialize a message to its postcard body (no frame header).
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, ProtoError> {
    postcard::to_allocvec(msg).map_err(ProtoError::Encode)
}

/// Deserialize a message from a postcard body.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtoError> {
    postcard::from_bytes(bytes).map_err(ProtoError::Decode)
}

/// Write a length-prefixed frame: `u32` LE body length, then the body.
/// Refuses to emit a body larger than [`MAX_FRAME_LEN`].
pub fn write_frame<W: std::io::Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), ProtoError> {
    let body = encode(msg)?;
    if body.len() > MAX_FRAME_LEN {
        return Err(ProtoError::TooLarge(body.len()));
    }
    w.write_all(&(body.len() as u32).to_le_bytes())?;
    w.write_all(&body)?;
    w.flush()?;
    Ok(())
}

/// Read one length-prefixed frame. The length is validated against
/// [`MAX_FRAME_LEN`] **before** allocating the body buffer, so a hostile
/// length prefix cannot OOM the reader.
pub fn read_frame<R: std::io::Read, T: DeserializeOwned>(r: &mut R) -> Result<T, ProtoError> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_FRAME_LEN {
        return Err(ProtoError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    decode(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> RememberKey {
        RememberKey {
            loginuid: 1000,
            sessionid: 3,
            service: "sudo".into(),
            command: "pacman -Syu".into(),
        }
    }

    #[test]
    fn request_round_trips() {
        for req in [
            Request::CheckRemember(RememberQuery {
                key: key(),
                ttl_secs: 300,
            }),
            Request::RecordRemember(key()),
            Request::Ping,
        ] {
            let bytes = encode(&req).unwrap();
            assert_eq!(decode::<Request>(&bytes).unwrap(), req);
        }
    }

    #[test]
    fn response_round_trips() {
        for resp in [
            Response::Remember { fresh: true },
            Response::Remember { fresh: false },
            Response::Recorded,
            Response::Pong {
                protocol: PROTOCOL_VERSION,
            },
            Response::Error("nope".into()),
        ] {
            let bytes = encode(&resp).unwrap();
            assert_eq!(decode::<Response>(&bytes).unwrap(), resp);
        }
    }

    #[test]
    fn framed_round_trip() {
        let req = Request::CheckRemember(RememberQuery {
            key: key(),
            ttl_secs: 60,
        });
        let mut buf = Vec::new();
        write_frame(&mut buf, &req).unwrap();
        let mut cur = std::io::Cursor::new(buf);
        assert_eq!(read_frame::<_, Request>(&mut cur).unwrap(), req);
    }

    #[test]
    fn oversized_length_is_rejected_before_alloc() {
        // A 4 GiB length prefix must error as TooLarge, not try to alloc.
        let mut framed = (u32::MAX).to_le_bytes().to_vec();
        framed.extend_from_slice(b"junk");
        let mut cur = std::io::Cursor::new(framed);
        let err = read_frame::<_, Request>(&mut cur).unwrap_err();
        assert!(matches!(err, ProtoError::TooLarge(_)));
    }

    #[test]
    fn truncated_body_errors() {
        // Header claims 10 bytes, only 2 follow.
        let mut framed = 10u32.to_le_bytes().to_vec();
        framed.extend_from_slice(&[0xAA, 0xBB]);
        let mut cur = std::io::Cursor::new(framed);
        assert!(read_frame::<_, Request>(&mut cur).is_err());
    }

    #[test]
    fn garbage_body_decode_errors_not_panics() {
        assert!(decode::<Request>(&[0xFF, 0xFF, 0xFF, 0xFF]).is_err());
    }

    #[test]
    fn bindable_rejects_unbound_keys() {
        assert!(key().is_bindable());
        let mut k = key();
        k.loginuid = u32::MAX;
        assert!(!k.is_bindable());
        let mut k = key();
        k.command.clear();
        assert!(!k.is_bindable());
    }
}
