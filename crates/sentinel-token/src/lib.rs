//! HMAC-SHA256 bypass token shared between `sentinel-polkit-agent` and
//! `pam-sentinel`.
//!
//! The agent generates a per-session secret at startup and persists it in
//! `$XDG_RUNTIME_DIR/sentinel-agent.secret` (mode `0600`). When the user
//! clicks **Allow** on the dialog, the agent computes
//! `HMAC-SHA256(secret, cookie || "|" || action_id)` and exports the
//! base64-encoded token to the `polkit-agent-helper-1` child via the
//! `SENTINEL_AGENT_AUTH` env var. `pam_sentinel.so` reads the same secret
//! file, recomputes the HMAC, and constant-time compares — a match returns
//! `PAM_SUCCESS` immediately, breaking the recursion loop where the PAM
//! stack would otherwise spawn another dialog under the agent.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::Sha256;
use std::fs;
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

type HmacSha256 = Hmac<Sha256>;

const SECRET_LEN: usize = 32;
const SECRET_BASENAME: &str = "sentinel-agent.secret";

/// HMAC issuer/verifier backed by a 32-byte secret.
pub struct Issuer {
    secret: [u8; SECRET_LEN],
}

impl Issuer {
    /// Generate a fresh secret and persist it under
    /// `$XDG_RUNTIME_DIR/sentinel-agent.secret` for `uid` (mode `0600`).
    /// If a secret already exists at the path it is overwritten.
    pub fn generate_and_persist(uid: u32) -> io::Result<Self> {
        let mut secret = [0u8; SECRET_LEN];
        rand::rng().fill_bytes(&mut secret);

        let path = secret_path_for_uid(uid);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Atomic write: temp file + rename. Mode 0600 set on creation.
        let tmp = path.with_extension("secret.tmp");
        {
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true).mode(0o600);
            let mut f = opts.open(&tmp)?;
            io::Write::write_all(&mut f, &secret)?;
            f.sync_all()?;
        }
        // Belt-and-suspenders: re-chmod in case umask interfered.
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        fs::rename(&tmp, &path)?;

        Ok(Self { secret })
    }

    /// Load the secret persisted for `uid`. Returns `Err(NotFound)` if no
    /// agent is running for that user (caller treats this as
    /// "fall through to normal flow").
    pub fn load_for_uid(uid: u32) -> io::Result<Self> {
        let path = secret_path_for_uid(uid);
        let bytes = fs::read(&path)?;
        if bytes.len() != SECRET_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected {SECRET_LEN}-byte secret, got {}", bytes.len()),
            ));
        }
        let mut secret = [0u8; SECRET_LEN];
        secret.copy_from_slice(&bytes);
        Ok(Self { secret })
    }

    /// Compute the base64url-no-pad HMAC token for `(cookie, action_id)`.
    pub fn token(&self, cookie: &str, action_id: &str) -> String {
        let tag = self.tag(cookie, action_id);
        URL_SAFE_NO_PAD.encode(tag)
    }

    /// Constant-time verify a base64-encoded token.
    pub fn verify(&self, cookie: &str, action_id: &str, token_b64: &str) -> bool {
        let Ok(provided) = URL_SAFE_NO_PAD.decode(token_b64.as_bytes()) else {
            return false;
        };
        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("any key length is valid");
        mac.update(cookie.as_bytes());
        mac.update(b"|");
        mac.update(action_id.as_bytes());
        // hmac::Mac::verify_slice is constant-time.
        mac.verify_slice(&provided).is_ok()
    }

    fn tag(&self, cookie: &str, action_id: &str) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("any key length is valid");
        mac.update(cookie.as_bytes());
        mac.update(b"|");
        mac.update(action_id.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }
}

/// Path to the secret file for `uid`. Honours `XDG_RUNTIME_DIR` if set,
/// otherwise falls back to `/run/user/<uid>/`.
pub fn secret_path_for_uid(uid: u32) -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join(SECRET_BASENAME);
        }
    }
    PathBuf::from(format!("/run/user/{uid}")).join(SECRET_BASENAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_roundtrip() {
        let issuer = Issuer { secret: [42u8; SECRET_LEN] };
        let t = issuer.token("cookie-123", "org.example.action");
        assert!(issuer.verify("cookie-123", "org.example.action", &t));
        assert!(!issuer.verify("cookie-124", "org.example.action", &t));
        assert!(!issuer.verify("cookie-123", "org.example.action.x", &t));
    }

    #[test]
    fn separator_resists_concatenation_collisions() {
        // (cookie="ab", action="c") and (cookie="a", action="bc") must
        // produce different tags despite identical concatenation.
        let issuer = Issuer { secret: [1u8; SECRET_LEN] };
        let t1 = issuer.token("ab", "c");
        let t2 = issuer.token("a", "bc");
        assert_ne!(t1, t2);
    }

    #[test]
    fn malformed_base64_rejected() {
        let issuer = Issuer { secret: [0u8; SECRET_LEN] };
        assert!(!issuer.verify("c", "a", "!!!not-base64!!!"));
        assert!(!issuer.verify("c", "a", ""));
    }

    #[test]
    fn wrong_length_token_rejected() {
        let issuer = Issuer { secret: [0u8; SECRET_LEN] };
        // Valid base64 but wrong length (1 byte).
        let short = URL_SAFE_NO_PAD.encode([0u8; 1]);
        assert!(!issuer.verify("c", "a", &short));
    }
}
