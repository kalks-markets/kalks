//! Media storage adapter, switchable without code changes (`CIRCLE_STORAGE`):
//!
//! - `local`: files under `CIRCLE_MEDIA_DIR`, served by Caddy at `/circle/media/<key>` on the app and trade hosts
//!   (until the founder provides Cloudflare R2).
//! - `s3`: any S3-compatible bucket (Cloudflare R2, Backblaze B2, AWS) signed with AWS Signature V4, public URLs
//!   from the CDN base `CIRCLE_S3_PUBLIC_URL`.
//!
//! Keys look like `m/2026/<random>/large.webp`; the random part is unguessable, so media URLs work as capabilities
//! (nothing is listed, and a removed item's files are deleted).

use crate::config::Config;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub enum Storage {
    Local { dir: PathBuf, url: String },
    S3(S3),
}

#[derive(Clone)]
pub struct S3 {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub public_url: String,
    pub http: reqwest::Client,
}

pub fn content_type(key: &str) -> &'static str {
    let ext = key.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "webp" => "image/webp",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "m3u8" => "application/vnd.apple.mpegurl",
        "ts" => "video/mp2t",
        "mp4" => "video/mp4",
        "m4a" => "audio/mp4",
        "ogg" | "opus" => "audio/ogg",
        "pdf" => "application/pdf",
        "txt" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }
}

/// Rejects keys that could escape the media directory.
fn safe_key(key: &str) -> anyhow::Result<&str> {
    if key.is_empty() || key.len() > 300 || key.starts_with('/') || key.contains("..") || key.contains('\\') || !key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_')) {
        anyhow::bail!("invalid storage key {key:?}");
    }
    Ok(key)
}

impl Storage {
    pub fn from_config(cfg: &Config, http: reqwest::Client) -> Self {
        if cfg.storage == "s3" {
            Storage::S3(S3 {
                endpoint: cfg.s3_endpoint.clone(),
                bucket: cfg.s3_bucket.clone(),
                region: cfg.s3_region.clone(),
                access_key: cfg.s3_access_key.clone(),
                secret_key: cfg.s3_secret_key.clone(),
                public_url: cfg.s3_public_url.clone(),
                http,
            })
        } else {
            Storage::Local { dir: PathBuf::from(&cfg.media_dir), url: cfg.media_url.clone() }
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Storage::Local { .. } => "local",
            Storage::S3(_) => "s3",
        }
    }

    /// Public URL of a stored key.
    pub fn url(&self, key: &str) -> String {
        match self {
            Storage::Local { url, .. } => format!("{url}/{key}"),
            Storage::S3(s) => format!("{}/{key}", s.public_url),
        }
    }

    pub async fn put(&self, key: &str, bytes: Vec<u8>) -> anyhow::Result<()> {
        let key = safe_key(key)?;
        match self {
            Storage::Local { dir, .. } => {
                let path = dir.join(key);
                if let Some(p) = path.parent() {
                    tokio::fs::create_dir_all(p).await?;
                }
                let tmp = path.with_extension("part");
                tokio::fs::write(&tmp, &bytes).await?;
                tokio::fs::rename(&tmp, &path).await?;
                Ok(())
            }
            Storage::S3(s) => s.put(key, bytes, content_type(key)).await,
        }
    }

    pub async fn put_file(&self, key: &str, path: &Path) -> anyhow::Result<()> {
        let bytes = tokio::fs::read(path).await?;
        self.put(key, bytes).await
    }

    pub async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let key = safe_key(key)?;
        match self {
            Storage::Local { dir, .. } => {
                match tokio::fs::remove_file(dir.join(key)).await {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
                // drop empty parent directories (best effort)
                let mut p = dir.join(key);
                while let Some(parent) = p.parent().map(Path::to_path_buf) {
                    if parent == *dir || tokio::fs::remove_dir(&parent).await.is_err() {
                        break;
                    }
                    p = parent;
                }
                Ok(())
            }
            Storage::S3(s) => s.delete(key).await,
        }
    }

    /// Reads a stored object back (moderation re-checks, tests).
    pub async fn get(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        let key = safe_key(key)?;
        match self {
            Storage::Local { dir, .. } => Ok(tokio::fs::read(dir.join(key)).await?),
            Storage::S3(s) => s.get(key).await,
        }
    }
}

// ---------------------------------------------------------------- S3 (Signature V4)

type HmacSha256 = Hmac<Sha256>;

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut m = <HmacSha256 as KeyInit>::new_from_slice(key).expect("hmac accepts any key size");
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

fn sha_hex(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

/// RFC 3986 encoding of a path (S3 style: every segment encoded, `/` kept).
pub fn uri_encode_path(p: &str) -> String {
    p.split('/').map(|seg| seg.bytes().map(|b| if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>()).collect::<Vec<_>>().join("/")
}

/// AWS Signature V4 `Authorization` header. `headers` are (lower-case name, value) and must include `host`,
/// `x-amz-content-sha256` and `x-amz-date`; `amz_date` is `YYYYMMDDTHHMMSSZ`.
#[allow(clippy::too_many_arguments)]
pub fn sign_v4(method: &str, path: &str, query: &str, headers: &[(String, String)], payload_hash: &str, access_key: &str, secret_key: &str, region: &str, service: &str, amz_date: &str) -> String {
    let mut hs: Vec<(String, String)> = headers.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_string())).collect();
    hs.sort();
    let canonical_headers: String = hs.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed: String = hs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(";");
    let canonical = format!("{method}\n{}\n{query}\n{canonical_headers}\n{signed}\n{payload_hash}", uri_encode_path(path));
    let date = &amz_date[..8];
    let scope = format!("{date}/{region}/{service}/aws4_request");
    let to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}", sha_hex(canonical.as_bytes()));
    let k_date = hmac(format!("AWS4{secret_key}").as_bytes(), date.as_bytes());
    let k_region = hmac(&k_date, region.as_bytes());
    let k_service = hmac(&k_region, service.as_bytes());
    let k_signing = hmac(&k_service, b"aws4_request");
    let sig = hex::encode(hmac(&k_signing, to_sign.as_bytes()));
    format!("AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed}, Signature={sig}")
}

impl S3 {
    fn target(&self, key: &str) -> anyhow::Result<(String, String, String)> {
        // path-style: {endpoint}/{bucket}/{key} (works on R2, B2 and AWS)
        let host_start = self.endpoint.find("://").map(|i| i + 3).ok_or_else(|| anyhow::anyhow!("CIRCLE_S3_ENDPOINT needs a scheme"))?;
        let host = self.endpoint[host_start..].split('/').next().unwrap_or("").to_string();
        let path = format!("/{}/{key}", self.bucket);
        Ok((format!("{}{}", self.endpoint.trim_end_matches('/'), uri_encode_path(&path)), host, path))
    }

    async fn send(&self, method: reqwest::Method, key: &str, body: Option<Vec<u8>>, content_type: Option<&str>) -> anyhow::Result<reqwest::Response> {
        let (url, host, path) = self.target(key)?;
        let payload_hash = sha_hex(body.as_deref().unwrap_or(b""));
        let amz_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let headers = vec![("host".to_string(), host), ("x-amz-content-sha256".to_string(), payload_hash.clone()), ("x-amz-date".to_string(), amz_date.clone())];
        let auth = sign_v4(method.as_str(), &path, "", &headers, &payload_hash, &self.access_key, &self.secret_key, &self.region, "s3", &amz_date);
        let mut rb = self.http.request(method, url).header("authorization", auth).header("x-amz-content-sha256", payload_hash).header("x-amz-date", amz_date).timeout(std::time::Duration::from_secs(120));
        if let Some(ct) = content_type {
            rb = rb.header("content-type", ct).header("cache-control", "public, max-age=31536000, immutable");
        }
        if let Some(b) = body {
            rb = rb.body(b);
        }
        Ok(rb.send().await?)
    }

    pub async fn put(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> anyhow::Result<()> {
        let r = self.send(reqwest::Method::PUT, key, Some(bytes), Some(content_type)).await?;
        if !r.status().is_success() {
            anyhow::bail!("S3 PUT {key} returned {}: {}", r.status(), r.text().await.unwrap_or_default().chars().take(300).collect::<String>());
        }
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let r = self.send(reqwest::Method::DELETE, key, None, None).await?;
        if !r.status().is_success() && r.status() != reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!("S3 DELETE {key} returned {}", r.status());
        }
        Ok(())
    }

    pub async fn get(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        let r = self.send(reqwest::Method::GET, key, None, None).await?;
        if !r.status().is_success() {
            anyhow::bail!("S3 GET {key} returned {}", r.status());
        }
        Ok(r.bytes().await?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example of the AWS documentation ("GET Object", examplebucket / test.txt with a Range header).
    #[test]
    fn signs_like_the_aws_example() {
        let headers = vec![
            ("host".to_string(), "examplebucket.s3.amazonaws.com".to_string()),
            ("range".to_string(), "bytes=0-9".to_string()),
            ("x-amz-content-sha256".to_string(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()),
            ("x-amz-date".to_string(), "20130524T000000Z".to_string()),
        ];
        let auth = sign_v4(
            "GET",
            "/test.txt",
            "",
            &headers,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "us-east-1",
            "s3",
            "20130524T000000Z",
        );
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn encodes_paths_and_guards_keys() {
        assert_eq!(uri_encode_path("/b/m/2026/a b+c.webp"), "/b/m/2026/a%20b%2Bc.webp");
        assert!(safe_key("m/2026/abc/large.webp").is_ok());
        for bad in ["../etc/passwd", "/abs", "m/../../x", "m\\x", "m/<x>", ""] {
            assert!(safe_key(bad).is_err(), "{bad}");
        }
        assert_eq!(content_type("m/x/hls/master.m3u8"), "application/vnd.apple.mpegurl");
        assert_eq!(content_type("m/x/poster.webp"), "image/webp");
    }

    #[tokio::test]
    async fn local_put_get_delete() {
        let dir = std::env::temp_dir().join(format!("circle-storage-{}", crate::util::token(6)));
        let s = Storage::Local { dir: dir.clone(), url: "/circle/media".into() };
        s.put("m/2026/k1/large.webp", b"abc".to_vec()).await.unwrap();
        assert_eq!(s.get("m/2026/k1/large.webp").await.unwrap(), b"abc");
        assert_eq!(s.url("m/2026/k1/large.webp"), "/circle/media/m/2026/k1/large.webp");
        s.delete("m/2026/k1/large.webp").await.unwrap();
        assert!(s.get("m/2026/k1/large.webp").await.is_err());
        assert!(!dir.join("m/2026/k1").exists(), "empty directories are removed");
        s.delete("m/2026/k1/large.webp").await.unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
