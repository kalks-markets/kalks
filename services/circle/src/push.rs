//! Push notifications through Firebase Cloud Messaging (HTTP v1 API). A no-op until `FCM_SERVICE_ACCOUNT_FILE`
//! points at the Firebase service-account JSON (the founder provides the Firebase project).
//!
//! Auth: a self-signed RS256 JWT (service account e-mail, scope firebase.messaging) is exchanged at the
//! account's `token_uri` for an OAuth access token, cached ~50 minutes. Device tokens come from the apps
//! (`POST /v1/circle/me/devices`); tokens FCM reports as unregistered are disabled.

use crate::config::Config;
use crate::state::AppState;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sqlx::Row;
use std::time::Duration;

pub fn configured(cfg: &Config) -> bool {
    !cfg.fcm_service_account.is_empty()
}

#[derive(Clone, Debug)]
pub struct ServiceAccount {
    pub client_email: String,
    pub private_key: String,
    pub token_uri: String,
    pub project_id: String,
}

pub fn parse_service_account(raw: &str, project_override: &str) -> anyhow::Result<ServiceAccount> {
    let v: Value = serde_json::from_str(raw)?;
    let s = |k: &str| v[k].as_str().map(str::to_string).filter(|x| !x.is_empty());
    Ok(ServiceAccount {
        client_email: s("client_email").ok_or_else(|| anyhow::anyhow!("service account: client_email missing"))?,
        private_key: s("private_key").ok_or_else(|| anyhow::anyhow!("service account: private_key missing"))?,
        token_uri: s("token_uri").unwrap_or_else(|| "https://oauth2.googleapis.com/token".into()),
        project_id: if project_override.is_empty() { s("project_id").ok_or_else(|| anyhow::anyhow!("service account: project_id missing"))? } else { project_override.to_string() },
    })
}

/// PKCS#8 DER from a PEM `-----BEGIN PRIVATE KEY-----` block.
fn pem_der(pem: &str) -> anyhow::Result<Vec<u8>> {
    let b64: String = pem.lines().filter(|l| !l.starts_with("-----")).map(str::trim).collect();
    Ok(STANDARD.decode(b64)?)
}

/// The signed JWT assertion for the token exchange.
pub fn assertion(sa: &ServiceAccount, now: i64) -> anyhow::Result<String> {
    let header = URL_SAFE_NO_PAD.encode(json!({"alg": "RS256", "typ": "JWT"}).to_string());
    let claims = URL_SAFE_NO_PAD.encode(
        json!({"iss": sa.client_email, "scope": "https://www.googleapis.com/auth/firebase.messaging", "aud": sa.token_uri, "iat": now, "exp": now + 3600}).to_string(),
    );
    let input = format!("{header}.{claims}");
    let key = ring::signature::RsaKeyPair::from_pkcs8(&pem_der(&sa.private_key)?).map_err(|e| anyhow::anyhow!("service account key: {e}"))?;
    let mut sig = vec![0u8; key.public().modulus_len()];
    key.sign(&ring::signature::RSA_PKCS1_SHA256, &ring::rand::SystemRandom::new(), input.as_bytes(), &mut sig).map_err(|e| anyhow::anyhow!("sign: {e}"))?;
    Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(sig)))
}

async fn account(st: &AppState) -> anyhow::Result<ServiceAccount> {
    let raw = tokio::fs::read_to_string(&st.cfg.fcm_service_account).await?;
    parse_service_account(&raw, &st.cfg.fcm_project_id)
}

async fn access_token(st: &AppState, sa: &ServiceAccount) -> anyhow::Result<String> {
    if let Some(t) = st.cache.get("fcm:token", Duration::from_secs(50 * 60)).and_then(|v| v.as_str().map(str::to_string)) {
        return Ok(t);
    }
    let jwt = assertion(sa, chrono::Utc::now().timestamp())?;
    let r = st
        .http
        .post(&sa.token_uri)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(format!("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={jwt}"))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    if !r.status().is_success() {
        anyhow::bail!("FCM token exchange returned {}", r.status());
    }
    let v: Value = r.json().await?;
    let t = v["access_token"].as_str().ok_or_else(|| anyhow::anyhow!("FCM token exchange: no access_token"))?.to_string();
    st.cache.put("fcm:token", json!(t));
    Ok(t)
}

/// The FCM v1 message for one device.
pub fn message(token: &str, title: &str, body: &str, data: &Value) -> Value {
    let data: serde_json::Map<String, Value> = data.as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), json!(v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))).collect()).unwrap_or_default();
    json!({"message": {
        "token": token,
        "notification": {"title": crate::util::preview(title, 120), "body": crate::util::preview(body, 240)},
        "data": data,
        "android": {"priority": "high", "notification": {"channel_id": "circle", "click_action": "FLUTTER_NOTIFICATION_CLICK"}},
        "apns": {"payload": {"aps": {"sound": "default"}}},
    }})
}

/// Pushes to every active device of `user` (best effort; errors are logged). Returns the devices reached.
pub async fn send(st: &AppState, user: i64, title: &str, body: &str, data: &Value) -> usize {
    if !configured(&st.cfg) {
        return 0;
    }
    match send_inner(st, user, title, body, data).await {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(error = %e, user, "push failed");
            0
        }
    }
}

async fn send_inner(st: &AppState, user: i64, title: &str, body: &str, data: &Value) -> anyhow::Result<usize> {
    let tokens: Vec<String> = sqlx::query("SELECT token FROM devices WHERE user_id = $1 AND NOT disabled ORDER BY last_seen_at DESC LIMIT 10")
        .bind(user)
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|r| r.get("token"))
        .collect();
    if tokens.is_empty() {
        return Ok(0);
    }
    let sa = account(st).await?;
    let bearer = access_token(st, &sa).await?;
    let mut ok = 0;
    for t in tokens {
        let r = st
            .http
            .post(format!("{}/v1/projects/{}/messages:send", st.cfg.fcm_api_url, sa.project_id))
            .bearer_auth(&bearer)
            .json(&message(&t, title, body, data))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        let status = r.status();
        if status.is_success() {
            ok += 1;
            continue;
        }
        let v: Value = r.json().await.unwrap_or(Value::Null);
        let unregistered = status.as_u16() == 404 || v.to_string().contains("UNREGISTERED") || (status.as_u16() == 400 && v.to_string().contains("registration token"));
        if unregistered {
            sqlx::query("UPDATE devices SET disabled = true WHERE token = $1").bind(&t).execute(&st.pool).await?;
        } else if status.as_u16() == 401 {
            st.cache.put("fcm:token", Value::Null);
        } else {
            tracing::warn!(%status, "FCM send failed");
        }
    }
    Ok(ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_messages() {
        let m = message("tok", "Ana liked your post", "", &json!({"kind": "like", "postId": 5}));
        assert_eq!(m["message"]["token"], "tok");
        assert_eq!(m["message"]["data"]["postId"], "5", "FCM data values are strings");
        assert_eq!(m["message"]["android"]["notification"]["channel_id"], "circle");
        let sa = parse_service_account(r#"{"client_email":"a@b.iam.gserviceaccount.com","private_key":"x","project_id":"kalks"}"#, "").unwrap();
        assert_eq!((sa.project_id.as_str(), sa.token_uri.as_str()), ("kalks", "https://oauth2.googleapis.com/token"));
        assert!(parse_service_account("{}", "").is_err());
    }
}
