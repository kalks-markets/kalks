//! Claude (server key `ANTHROPIC_API_KEY`): moderation, translation, per-symbol community sentiment, the post
//! helper and the daily feed digest.
//!
//! Every call is one `POST /v1/messages` with a JSON-schema constrained answer (`output_config.format`), the
//! stable system prompt cached (`cache_control`), effort `low` for classification and short texts, and the
//! server-side refusal fallback (`fallbacks: "default"`), as in the support bot and the news brief. Model:
//! `CIRCLE_AI_MODEL` (default `claude-opus-5-5`), moderation `CIRCLE_MODERATION_MODEL` (same by default).
//! Results are cached in `ai_cache`; client-facing calls are rate-limited per member.

use crate::state::AppState;
use crate::util::sha256_hex;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Debug)]
pub enum AiError {
    NotConfigured,
    Refused,
    Api(String),
    Invalid(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::NotConfigured => write!(f, "AI is not configured"),
            AiError::Refused => write!(f, "the model declined"),
            AiError::Api(e) => write!(f, "Claude API: {e}"),
            AiError::Invalid(e) => write!(f, "invalid answer: {e}"),
        }
    }
}

pub struct Ask<'a> {
    pub model: &'a str,
    pub system: &'a str,
    pub content: Vec<Value>,
    pub schema: Value,
    pub effort: &'a str,
    pub max_tokens: u32,
}

/// The request body (pure, unit-tested). Haiku-class models take neither `effort` nor the fallback.
pub fn request_body(a: &Ask) -> Value {
    let haiku = a.model.contains("haiku");
    let mut output = json!({"format": {"type": "json_schema", "schema": a.schema}});
    if !haiku {
        output["effort"] = json!(a.effort);
    }
    let mut b = json!({
        "model": a.model,
        "max_tokens": a.max_tokens,
        "system": [{"type": "text", "text": a.system, "cache_control": {"type": "ephemeral"}}],
        "messages": [{"role": "user", "content": a.content}],
        "output_config": output,
    });
    if !haiku {
        b["fallbacks"] = json!("default");
    }
    b
}

/// One schema-constrained call. Returns (answer, model that answered).
pub async fn call(st: &AppState, a: Ask<'_>) -> Result<(Value, String), AiError> {
    if !st.ai() {
        return Err(AiError::NotConfigured);
    }
    let body = request_body(&a);
    let mut rb = st
        .http
        .post(format!("{}/v1/messages", st.cfg.anthropic_url))
        .header("x-api-key", &st.cfg.anthropic_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .timeout(Duration::from_secs(90))
        .json(&body);
    if !a.model.contains("haiku") {
        rb = rb.header("anthropic-beta", "server-side-fallback-2026-07-01");
    }
    let r = rb.send().await.map_err(|e| AiError::Api(e.to_string()))?;
    let status = r.status();
    let v: Value = r.json().await.map_err(|e| AiError::Api(e.to_string()))?;
    if !status.is_success() {
        return Err(AiError::Api(format!("{status}: {}", v.pointer("/error/message").and_then(Value::as_str).unwrap_or("error"))));
    }
    parse_response(&v, a.model)
}

/// Reads the JSON answer out of a Messages API response.
pub fn parse_response(v: &Value, fallback_model: &str) -> Result<(Value, String), AiError> {
    match v["stop_reason"].as_str() {
        Some("refusal") => return Err(AiError::Refused),
        Some("max_tokens") => return Err(AiError::Invalid("answer cut off".into())),
        _ => {}
    }
    let text: String = v["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
    let parsed: Value = serde_json::from_str(text.trim()).map_err(|_| AiError::Invalid("not JSON".into()))?;
    Ok((parsed, v["model"].as_str().unwrap_or(fallback_model).to_string()))
}

// ---------------------------------------------------------------- cache

pub async fn cached(st: &AppState, key: &str) -> Option<Value> {
    sqlx::query_scalar::<_, sqlx::types::Json<Value>>("SELECT result FROM ai_cache WHERE key = $1 AND (expires_at IS NULL OR expires_at > now())")
        .bind(key)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten()
        .map(|j| j.0)
}

pub async fn store(st: &AppState, key: &str, kind: &str, result: &Value, model: &str, ttl: Option<chrono::Duration>) {
    let _ = sqlx::query(
        "INSERT INTO ai_cache (key, kind, result, model, expires_at) VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (key) DO UPDATE SET result = EXCLUDED.result, model = EXCLUDED.model, created_at = now(), expires_at = EXCLUDED.expires_at",
    )
    .bind(key)
    .bind(kind)
    .bind(sqlx::types::Json(result))
    .bind(model)
    .bind(ttl.map(|t| chrono::Utc::now() + t))
    .execute(&st.pool)
    .await;
}

// ---------------------------------------------------------------- translate

const TRANSLATE_SYSTEM: &str = "You translate short social-media texts written by retail traders on Kalks Circle.
Rules: translate faithfully and naturally into the requested language; keep $cashtags (e.g. $XAUUSD), #hashtags, @mentions, numbers, prices, URLs and emoji exactly as written; keep trading jargon understandable (SL, TP, pips, lots may stay as they are); never add, remove or soften content; never add commentary. If the text is already in the requested language, return it unchanged.";

/// Translation of `text` into `lang` (cached by text + language).
pub async fn translate(st: &AppState, text: &str, lang: &str) -> Result<String, AiError> {
    let key = sha256_hex(format!("translate:{lang}:{text}"));
    if let Some(v) = cached(st, &key).await
        && let Some(t) = v["text"].as_str()
    {
        return Ok(t.to_string());
    }
    let schema = json!({"type": "object", "additionalProperties": false, "required": ["text"], "properties": {"text": {"type": "string"}}});
    let prompt = format!("Translate into {} ({lang}).\n<text>\n{text}\n</text>", crate::text::lang_name(lang));
    let (v, model) = call(st, Ask { model: &st.cfg.ai_model, system: TRANSLATE_SYSTEM, content: vec![json!({"type": "text", "text": prompt})], schema, effort: "low", max_tokens: 4096 }).await?;
    let t = v["text"].as_str().ok_or_else(|| AiError::Invalid("no text".into()))?.to_string();
    store(st, &key, "translate", &json!({"text": t}), &model, None).await;
    Ok(t)
}

// ---------------------------------------------------------------- sentiment

const SENTIMENT_SYSTEM: &str = "You summarise what a trading community is saying about one instrument, for other members.
Use only the posts and vote counts given. Be neutral and factual: describe the crowd's view and the reasons given, never recommend a trade, never predict the price, never present opinions as facts. Plain English, short sentences, no hype, no emoji.";

pub async fn sentiment(st: &AppState, symbol: &str, posts: &[String], bulls: i64, bears: i64) -> Result<Value, AiError> {
    let key = sha256_hex(format!("sentiment:{symbol}:{}", chrono::Utc::now().format("%Y%m%d%H")));
    if let Some(v) = cached(st, &key).await {
        return Ok(v);
    }
    let schema = json!({
        "type": "object", "additionalProperties": false, "required": ["mood", "summary", "points"],
        "properties": {
            "mood": {"type": "string", "enum": ["bullish", "bearish", "mixed", "neutral"]},
            "summary": {"type": "string", "description": "One or two sentences, at most 280 characters"},
            "points": {"type": "array", "items": {"type": "string"}, "description": "2 to 4 short reasons members give"}
        }
    });
    let mut lines = vec![format!("Instrument: {symbol}"), format!("Bull votes: {bulls}, bear votes: {bears}"), "Recent posts (newest first):".into()];
    for p in posts.iter().take(60) {
        lines.push(format!("- {}", crate::util::preview(p, 400)));
    }
    let (mut v, model) = call(st, Ask { model: &st.cfg.ai_model, system: SENTIMENT_SYSTEM, content: vec![json!({"type": "text", "text": lines.join("\n")})], schema, effort: "low", max_tokens: 4096 }).await?;
    v["summary"] = json!(crate::util::preview(v["summary"].as_str().unwrap_or(""), 300));
    v["points"] = json!(v["points"].as_array().cloned().unwrap_or_default().into_iter().take(4).collect::<Vec<_>>());
    store(st, &key, "sentiment", &v, &model, Some(chrono::Duration::minutes(30))).await;
    Ok(v)
}

// ---------------------------------------------------------------- post helper

const CAPTION_SYSTEM: &str = "You help a trader write a short, honest caption for a post on Kalks Circle about their trade or chart.
Write in the requested language, first person, at most 3 short sentences, plain text with optional $cashtag and up to 3 #hashtags.
Describe what the trader did and why (from their notes and the figures given); never promise or predict profits, never call anything guaranteed or risk free, never give advice to others. Also give a one-paragraph neutral explanation of the setup for beginners.";

pub async fn caption(st: &AppState, context: &Value, notes: &str, lang: &str) -> Result<Value, AiError> {
    let key = sha256_hex(format!("caption:{lang}:{context}:{notes}"));
    if let Some(v) = cached(st, &key).await {
        return Ok(v);
    }
    let schema = json!({
        "type": "object", "additionalProperties": false, "required": ["caption", "explanation", "hashtags"],
        "properties": {"caption": {"type": "string"}, "explanation": {"type": "string"}, "hashtags": {"type": "array", "items": {"type": "string"}}}
    });
    let prompt = format!("Language: {} ({lang})\n<figures>\n{context}\n</figures>\n<notes>\n{}\n</notes>", crate::text::lang_name(lang), crate::util::clean(notes, 1000));
    let (v, model) = call(st, Ask { model: &st.cfg.ai_model, system: CAPTION_SYSTEM, content: vec![json!({"type": "text", "text": prompt})], schema, effort: "low", max_tokens: 4096 }).await?;
    store(st, &key, "caption", &v, &model, Some(chrono::Duration::days(1))).await;
    Ok(v)
}

// ---------------------------------------------------------------- daily digest

const DIGEST_SYSTEM: &str = "You write a member's daily digest of their Kalks Circle feed: what the traders they follow and the community talked about in the last 24 hours.
Use only the posts given. Neutral, factual, no advice, no predictions, no hype. Each line is one short sentence that refers to one post by its id.";

pub async fn digest(st: &AppState, user: i64, lang: &str, posts: &[(i64, String, String)], trending: &[String]) -> Result<Value, AiError> {
    let key = sha256_hex(format!("digest:{user}:{lang}:{}", chrono::Utc::now().format("%Y%m%d")));
    if let Some(v) = cached(st, &key).await {
        return Ok(v);
    }
    let schema = json!({
        "type": "object", "additionalProperties": false, "required": ["headline", "items"],
        "properties": {
            "headline": {"type": "string"},
            "items": {"type": "array", "items": {"type": "object", "additionalProperties": false, "required": ["postId", "line"], "properties": {"postId": {"type": "integer"}, "line": {"type": "string"}}}}
        }
    });
    let mut lines = vec![format!("Write in {} ({lang}). 3 to 6 items.", crate::text::lang_name(lang)), format!("Trending: {}", trending.join(", ")), "Posts:".into()];
    for (id, who, text) in posts.iter().take(40) {
        lines.push(format!("[{id}] {who}: {}", crate::util::preview(text, 300)));
    }
    let (mut v, model) = call(st, Ask { model: &st.cfg.ai_model, system: DIGEST_SYSTEM, content: vec![json!({"type": "text", "text": lines.join("\n")})], schema, effort: "low", max_tokens: 4096 }).await?;
    // only ids that were given
    let ids: std::collections::HashSet<i64> = posts.iter().map(|(i, _, _)| *i).collect();
    v["items"] = json!(v["items"].as_array().cloned().unwrap_or_default().into_iter().filter(|i| i["postId"].as_i64().is_some_and(|x| ids.contains(&x))).take(6).collect::<Vec<_>>());
    store(st, &key, "digest", &v, &model, Some(chrono::Duration::hours(24))).await;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_requests_and_reads_answers() {
        let a = Ask { model: "claude-opus-5-5", system: "s", content: vec![json!({"type": "text", "text": "hi"})], schema: json!({"type": "object"}), effort: "low", max_tokens: 100 };
        let b = request_body(&a);
        assert_eq!(b["output_config"]["effort"], "low");
        assert_eq!(b["output_config"]["format"]["type"], "json_schema");
        assert_eq!(b["fallbacks"], "default");
        assert_eq!(b["system"][0]["cache_control"]["type"], "ephemeral");
        let h = request_body(&Ask { model: "claude-haiku-4-5", ..a });
        assert!(h["output_config"].get("effort").is_none() && h.get("fallbacks").is_none());
        let ok = json!({"model": "claude-opus-5-5", "stop_reason": "end_turn", "content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": "{\"text\":\"hola\"}"}]});
        assert_eq!(parse_response(&ok, "x").unwrap().0["text"], "hola");
        assert!(matches!(parse_response(&json!({"stop_reason": "refusal", "content": []}), "x"), Err(AiError::Refused)));
        assert!(matches!(parse_response(&json!({"stop_reason": "end_turn", "content": [{"type": "text", "text": "nope"}]}), "x"), Err(AiError::Invalid(_))));
    }
}
