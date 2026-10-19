//! Preview tokens: short-lived, HMAC-signed grants to read drafts on the public content endpoint for one tenant
//! and surface. Format `<base64url(tenant|surface|expiry unix)>.<base64url(HMAC-SHA256)>`. Nothing is stored:
//! a token simply stops working when it expires (or when CONTENT_PREVIEW_SECRET changes).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// Lifetime of a preview token.
pub const TTL_SECS: i64 = 30 * 60;

fn sign(secret: &str, payload: &str) -> Vec<u8> {
    let mut m = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key size");
    m.update(b"content-preview\0");
    m.update(payload.as_bytes());
    m.finalize().into_bytes().to_vec()
}

/// A token for `tenant` / `surface` valid until `exp` (unix seconds).
pub fn issue(secret: &str, tenant: &str, surface: &str, exp: i64) -> String {
    let payload = format!("{tenant}|{surface}|{exp}");
    format!("{}.{}", URL_SAFE_NO_PAD.encode(&payload), URL_SAFE_NO_PAD.encode(sign(secret, &payload)))
}

/// True when `token` is a valid, unexpired grant for exactly this tenant and surface.
pub fn verify(secret: &str, token: &str, tenant: &str, surface: &str, now: i64) -> bool {
    let Some((p, s)) = token.split_once('.') else { return false };
    let (Ok(payload), Ok(sig)) = (URL_SAFE_NO_PAD.decode(p), URL_SAFE_NO_PAD.decode(s)) else { return false };
    let Ok(payload) = String::from_utf8(payload) else { return false };
    if !bool::from(sign(secret, &payload).ct_eq(&sig)) {
        return false;
    }
    let mut it = payload.split('|');
    let (Some(t), Some(su), Some(exp), None) = (it.next(), it.next(), it.next(), it.next()) else { return false };
    t == tenant && su == surface && exp.parse::<i64>().is_ok_and(|e| e > now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_scoped_and_expire() {
        let tok = issue("s3cret", "kalks", "website", 1_000);
        assert!(verify("s3cret", &tok, "kalks", "website", 999));
        assert!(!verify("s3cret", &tok, "kalks", "website", 1_000), "expired");
        assert!(!verify("s3cret", &tok, "other", "website", 10), "other tenant");
        assert!(!verify("s3cret", &tok, "kalks", "app", 10), "other surface");
        assert!(!verify("different", &tok, "kalks", "website", 10), "other secret");
        // a tampered payload fails the signature
        let (_, sig) = tok.split_once('.').unwrap();
        let forged = format!("{}.{sig}", URL_SAFE_NO_PAD.encode("kalks|website|9999999999"));
        assert!(!verify("s3cret", &forged, "kalks", "website", 10));
        assert!(!verify("s3cret", "garbage", "kalks", "website", 10));
    }
}
