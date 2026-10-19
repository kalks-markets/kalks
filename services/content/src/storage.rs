//! Where media bytes live. `Storage` is the seam: `DiskStorage` (default, `CONTENT_STORAGE=disk`) keeps files under
//! `CONTENT_STORAGE_DIR`; `R2Storage` (`CONTENT_STORAGE=r2`, cargo feature `r2`) talks to Cloudflare R2 or any
//! S3-compatible bucket with SigV4-signed PUT / GET / DELETE. Keys are `<tenant>/<file id>/<variant>.<ext>` and
//! never contain `..`. Media is public content: nothing here is encrypted.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use crate::config::Config;

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = anyhow::Result<T>> + Send + 'a>>;

pub trait Storage: Send + Sync {
    /// Stores `bytes` under `key` (overwrites).
    fn put<'a>(&'a self, key: &'a str, bytes: Vec<u8>, mime: &'a str) -> BoxFut<'a, ()>;
    /// The bytes under `key`; None when missing.
    fn get<'a>(&'a self, key: &'a str) -> BoxFut<'a, Option<Vec<u8>>>;
    /// Removes `key` (missing is fine).
    fn delete<'a>(&'a self, key: &'a str) -> BoxFut<'a, ()>;
    fn name(&self) -> &'static str;
}

fn check_key(key: &str) -> anyhow::Result<()> {
    if key.is_empty() || key.contains("..") || key.starts_with('/') || !key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.')) {
        anyhow::bail!("bad storage key");
    }
    Ok(())
}

/// The configured backend.
pub fn from_config(cfg: &Config) -> anyhow::Result<Arc<dyn Storage>> {
    match cfg.storage.as_str() {
        #[cfg(feature = "r2")]
        "r2" => Ok(Arc::new(R2Storage::new(&cfg.r2)?)),
        #[cfg(not(feature = "r2"))]
        "r2" => anyhow::bail!("this build has no R2 support (cargo feature `r2`)"),
        _ => Ok(Arc::new(DiskStorage::new(&cfg.storage_dir))),
    }
}

/* ------------------------------------------------------------------ */
/* Local disk                                                          */
/* ------------------------------------------------------------------ */

pub struct DiskStorage {
    root: PathBuf,
}

impl DiskStorage {
    pub fn new(root: &str) -> Self {
        Self { root: PathBuf::from(root) }
    }
}

impl Storage for DiskStorage {
    fn put<'a>(&'a self, key: &'a str, bytes: Vec<u8>, _mime: &'a str) -> BoxFut<'a, ()> {
        Box::pin(async move {
            check_key(key)?;
            let path = self.root.join(key);
            if let Some(dir) = path.parent() {
                tokio::fs::create_dir_all(dir).await?;
            }
            // write then rename: a reader never sees half a file
            let tmp = path.with_extension("part");
            tokio::fs::write(&tmp, bytes).await?;
            tokio::fs::rename(&tmp, &path).await?;
            Ok(())
        })
    }

    fn get<'a>(&'a self, key: &'a str) -> BoxFut<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            check_key(key)?;
            match tokio::fs::read(self.root.join(key)).await {
                Ok(b) => Ok(Some(b)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    fn delete<'a>(&'a self, key: &'a str) -> BoxFut<'a, ()> {
        Box::pin(async move {
            check_key(key)?;
            match tokio::fs::remove_file(self.root.join(key)).await {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
                _ => Ok(()),
            }
        })
    }

    fn name(&self) -> &'static str {
        "disk"
    }
}

/* ------------------------------------------------------------------ */
/* Cloudflare R2 / S3 (SigV4)                                          */
/* ------------------------------------------------------------------ */

/// SigV4 helpers (pure; the signature is checked against AWS's published example in the tests).
pub mod sigv4 {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::{Digest, Sha256};

    pub fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    pub fn sha256_hex(b: &[u8]) -> String {
        hex(&Sha256::digest(b))
    }

    fn hmac(key: &[u8], msg: &str) -> Vec<u8> {
        let mut m = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("hmac accepts any key size");
        m.update(msg.as_bytes());
        m.finalize().into_bytes().to_vec()
    }

    /// URI-encodes one path segment the way S3 expects (unreserved characters kept).
    pub fn encode_segment(s: &str) -> String {
        s.bytes().map(|b| if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
    }

    /// `Authorization` header value. `headers`: lowercase name → trimmed value, sorted by name, all signed.
    #[allow(clippy::too_many_arguments)]
    pub fn authorization(method: &str, path: &str, query: &str, headers: &[(String, String)], payload_hash: &str, amz_date: &str, region: &str, access_key: &str, secret: &str) -> String {
        let date = &amz_date[..8];
        let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
        let signed: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
        let signed = signed.join(";");
        let canonical = format!("{method}\n{path}\n{query}\n{canonical_headers}\n{signed}\n{payload_hash}");
        let scope = format!("{date}/{region}/s3/aws4_request");
        let to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}", sha256_hex(canonical.as_bytes()));
        let k = hmac(format!("AWS4{secret}").as_bytes(), date);
        let k = hmac(&k, region);
        let k = hmac(&k, "s3");
        let k = hmac(&k, "aws4_request");
        let sig = hex(&hmac(&k, &to_sign));
        format!("AWS4-HMAC-SHA256 Credential={access_key}/{scope},SignedHeaders={signed},Signature={sig}")
    }
}

#[cfg(feature = "r2")]
pub struct R2Storage {
    http: reqwest::Client,
    cfg: crate::config::R2Config,
    host: String,
}

#[cfg(feature = "r2")]
impl R2Storage {
    pub fn new(cfg: &crate::config::R2Config) -> anyhow::Result<Self> {
        let host = cfg.endpoint.split("://").nth(1).unwrap_or(&cfg.endpoint).trim_end_matches('/').to_string();
        if host.is_empty() {
            anyhow::bail!("R2_ENDPOINT is not a URL");
        }
        let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(60)).build()?;
        Ok(Self { http, cfg: cfg.clone(), host })
    }

    /// Path-style request (`<endpoint>/<bucket>/<key>`), signed with the payload hash.
    async fn send(&self, method: reqwest::Method, key: &str, body: Option<(Vec<u8>, &str)>) -> anyhow::Result<reqwest::Response> {
        check_key(key)?;
        let path = format!("/{}/{}", sigv4::encode_segment(&self.cfg.bucket), key.split('/').map(sigv4::encode_segment).collect::<Vec<_>>().join("/"));
        let amz_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let payload_hash = sigv4::sha256_hex(body.as_ref().map(|b| b.0.as_slice()).unwrap_or(b""));
        let mut headers = vec![("host".to_string(), self.host.clone()), ("x-amz-content-sha256".to_string(), payload_hash.clone()), ("x-amz-date".to_string(), amz_date.clone())];
        if let Some((_, mime)) = &body {
            headers.push(("content-type".to_string(), mime.to_string()));
        }
        headers.sort();
        let auth = sigv4::authorization(method.as_str(), &path, "", &headers, &payload_hash, &amz_date, &self.cfg.region, &self.cfg.access_key_id, &self.cfg.secret_access_key);
        let mut rb = self.http.request(method, format!("{}{path}", self.cfg.endpoint)).header("authorization", auth);
        for (k, v) in &headers {
            if k != "host" {
                rb = rb.header(k.as_str(), v.as_str());
            }
        }
        if let Some((bytes, _)) = body {
            rb = rb.body(bytes);
        }
        Ok(rb.send().await?)
    }
}

#[cfg(feature = "r2")]
impl Storage for R2Storage {
    fn put<'a>(&'a self, key: &'a str, bytes: Vec<u8>, mime: &'a str) -> BoxFut<'a, ()> {
        Box::pin(async move {
            let r = self.send(reqwest::Method::PUT, key, Some((bytes, mime))).await?;
            if !r.status().is_success() {
                anyhow::bail!("R2 PUT {key}: HTTP {}", r.status());
            }
            Ok(())
        })
    }

    fn get<'a>(&'a self, key: &'a str) -> BoxFut<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            let r = self.send(reqwest::Method::GET, key, None).await?;
            match r.status().as_u16() {
                200 => Ok(Some(r.bytes().await?.to_vec())),
                404 => Ok(None),
                s => anyhow::bail!("R2 GET {key}: HTTP {s}"),
            }
        })
    }

    fn delete<'a>(&'a self, key: &'a str) -> BoxFut<'a, ()> {
        Box::pin(async move {
            let r = self.send(reqwest::Method::DELETE, key, None).await?;
            if !r.status().is_success() && r.status().as_u16() != 404 {
                anyhow::bail!("R2 DELETE {key}: HTTP {}", r.status());
            }
            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "r2"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigv4_matches_the_aws_example() {
        // "Example: GET Object" from the AWS S3 SigV4 documentation (header-based auth)
        let mut headers = vec![
            ("host".to_string(), "examplebucket.s3.amazonaws.com".to_string()),
            ("range".to_string(), "bytes=0-9".to_string()),
            ("x-amz-content-sha256".to_string(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()),
            ("x-amz-date".to_string(), "20130524T000000Z".to_string()),
        ];
        headers.sort();
        let auth = sigv4::authorization(
            "GET",
            "/test.txt",
            "",
            &headers,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "20130524T000000Z",
            "us-east-1",
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        assert!(auth.ends_with("Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"), "{auth}");
        assert!(auth.contains("SignedHeaders=host;range;x-amz-content-sha256;x-amz-date"));
    }

    #[test]
    fn keys_cannot_escape_the_root() {
        assert!(check_key("kalks/abc/sm.webp").is_ok());
        assert!(check_key("../etc/passwd").is_err());
        assert!(check_key("/abs").is_err());
        assert!(check_key("a b").is_err());
    }

    #[tokio::test]
    async fn disk_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let s = DiskStorage::new(dir.path().to_str().unwrap());
        s.put("t/f1/orig.png", b"abc".to_vec(), "image/png").await.unwrap();
        assert_eq!(s.get("t/f1/orig.png").await.unwrap().as_deref(), Some(&b"abc"[..]));
        s.delete("t/f1/orig.png").await.unwrap();
        assert!(s.get("t/f1/orig.png").await.unwrap().is_none());
        s.delete("t/f1/orig.png").await.unwrap();
    }
}
