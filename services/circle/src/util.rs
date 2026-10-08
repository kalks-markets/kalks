//! Small helpers: random tokens, hashing, text clean-up, cursors.

use sha2::{Digest, Sha256};

/// Random token of `n` random bytes (hex).
pub fn token(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).expect("OS randomness unavailable");
    hex::encode(b)
}

pub fn sha256_hex(s: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(s.as_ref()))
}

/// Constant-time string comparison.
pub fn eq_ct(a: &str, b: &str) -> bool {
    use subtle::ConstantTimeEq;
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

/// Trims, drops control characters (keeps newlines / tabs) and caps the length in characters.
pub fn clean(s: &str, max: usize) -> String {
    let t: String = s.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').take(max).collect();
    // at most two consecutive blank lines
    let mut out = String::with_capacity(t.len());
    let mut newlines = 0;
    for c in t.trim().chars() {
        if c == '\n' {
            newlines += 1;
            if newlines > 2 {
                continue;
            }
        } else if !c.is_whitespace() {
            newlines = 0;
        }
        out.push(c);
    }
    out
}

/// One-line preview.
pub fn preview(s: &str, max: usize) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > max { format!("{}…", one.chars().take(max.saturating_sub(1)).collect::<String>()) } else { one }
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `limit` clamped.
pub fn clamp_limit(v: Option<i64>, default: i64, max: i64) -> i64 {
    v.unwrap_or(default).clamp(1, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_names() {
        assert_eq!(percent_decode("Julia%20Novak"), "Julia Novak");
        assert_eq!(percent_decode("A%C3%AFda"), "Aïda");
        assert_eq!(percent_decode("50%"), "50%");
        assert_eq!(percent_decode("5%2"), "5%2");
    }

    #[test]
    fn cleans_text() {
        assert_eq!(clean("  hi\u{0007} there\n ", 100), "hi there");
        assert_eq!(clean("abcdef", 3), "abc");
        assert_eq!(clean("a\n\n\n\n\nb", 100), "a\n\nb");
        assert_eq!(preview("a\n\nb   c", 140), "a b c");
        assert_eq!(preview("abcdef", 4), "abc…");
        assert!(eq_ct("abc", "abc") && !eq_ct("abc", "abd") && !eq_ct("abc", "ab"));
    }
}
