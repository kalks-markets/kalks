//! Safety (Q33–Q35, Q37–Q38): every upload is checked before anyone else sees it.
//!
//! 1. Synchronous rules when content is created: external links outside the allow-list and `block` keyword rules
//!    are refused with 422 (`link_not_allowed`, `content_blocked`); `review` keywords send the item to the staff
//!    queue.
//! 2. Claude classification in the workers: text, images (vision), video (sampled frames; there is no
//!    speech-to-text on the server, so a video's audio is not checked) and PDF files. Decisions: allow → visible;
//!    review → staff queue, hidden meanwhile; block → rejected, the author is told why.
//!    Detected: nudity / sexual content, violence, hate / harassment, scams, guaranteed-profit promises, P&L
//!    screenshots from other platforms (profit claims only through verified trade cards), off-platform
//!    solicitation, spam, personal data, impersonation.
//! 3. Without an AI key: text passes on the rules; images and video follow `CIRCLE_MODERATION_FALLBACK`
//!    (`review` = staff queue, `publish`).

use crate::ai::{self, AiError, Ask};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Review,
    Block,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Review => "review",
            Decision::Block => "block",
        }
    }
    fn parse(s: &str) -> Decision {
        match s {
            "allow" => Decision::Allow,
            "block" => Decision::Block,
            _ => Decision::Review,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Verdict {
    pub decision: Decision,
    pub categories: Vec<String>,
    pub reason: String,
    /// rules | ai | fallback
    pub source: &'static str,
    pub model: Option<String>,
}

impl Verdict {
    pub fn allow(source: &'static str) -> Self {
        Verdict { decision: Decision::Allow, categories: vec![], reason: String::new(), source, model: None }
    }
    pub fn json(&self) -> Value {
        json!({"decision": self.decision.as_str(), "categories": self.categories, "reason": self.reason, "source": self.source, "model": self.model, "at": chrono::Utc::now()})
    }
    /// The stricter of two verdicts (block > review > allow), keeping every category.
    pub fn merge(self, other: Verdict) -> Verdict {
        let rank = |d: Decision| match d {
            Decision::Allow => 0,
            Decision::Review => 1,
            Decision::Block => 2,
        };
        let mut cats = self.categories.clone();
        for c in &other.categories {
            if !cats.contains(c) {
                cats.push(c.clone());
            }
        }
        let (mut top, low) = if rank(other.decision) > rank(self.decision) { (other, self) } else { (self, other) };
        if top.reason.is_empty() {
            top.reason = low.reason;
        }
        top.categories = cats;
        top
    }
}

pub const CATEGORIES: &[&str] = &[
    "nudity", "sexual", "violence", "hate", "harassment", "scam", "guaranteed_profit", "pnl_screenshot", "off_platform", "spam", "personal_data", "impersonation", "self_harm", "illegal", "link", "keyword",
];

/// User-facing wording of the main category.
pub fn reason_text(cat: &str) -> &'static str {
    match cat {
        "nudity" | "sexual" => "Nudity and sexual content aren't allowed.",
        "violence" => "Violent content isn't allowed.",
        "hate" | "harassment" => "Hate, harassment and personal attacks aren't allowed.",
        "scam" => "This looks like a scam or a request for money or account access.",
        "guaranteed_profit" => "Promises of guaranteed or risk-free profit aren't allowed.",
        "pnl_screenshot" => "Profit claims are shown only with verified Kalks trade cards. Screenshots of P&L from other platforms aren't allowed.",
        "off_platform" => "Inviting members to other apps for signals or account management isn't allowed.",
        "spam" => "This looks like spam or advertising.",
        "personal_data" => "Sharing someone's personal data isn't allowed.",
        "impersonation" => "Impersonating Kalks or other members isn't allowed.",
        "self_harm" => "This content can't be shown.",
        "illegal" => "Illegal content isn't allowed.",
        "link" => "External links are limited to Kalks pages, YouTube and TradingView.",
        _ => "This content breaks the community rules.",
    }
}

// ---------------------------------------------------------------- rules

#[derive(Clone, Default)]
pub struct Rules {
    /// (pattern, action block | review)
    pub keywords: Vec<(String, String)>,
    pub allow: Vec<String>,
}

pub async fn rules(st: &AppState) -> ApiResult<Rules> {
    if let Some(v) = st.cache.get("rules", Duration::from_secs(30))
        && let Some(r) = rules_from(&v)
    {
        return Ok(r);
    }
    let rows: Vec<(String, String, String)> = sqlx::query_as("SELECT kind, pattern, action FROM rules WHERE active").fetch_all(&st.pool).await?;
    let v = json!(rows.iter().map(|(k, p, a)| json!([k, p, a])).collect::<Vec<_>>());
    st.cache.put("rules", v.clone());
    Ok(rules_from(&v).unwrap_or_default())
}

fn rules_from(v: &Value) -> Option<Rules> {
    let mut r = Rules::default();
    for row in v.as_array()? {
        let (k, p, a) = (row[0].as_str()?, row[1].as_str()?, row[2].as_str()?);
        match k {
            "keyword" => r.keywords.push((p.to_string(), a.to_string())),
            "link_allow" => r.allow.push(p.to_string()),
            _ => {}
        }
    }
    Some(r)
}

pub fn invalidate_rules(st: &AppState) {
    st.cache.put("rules", Value::Null);
}

/// Rule check of a text (pure): Err(422) for a forbidden link or a blocking keyword, Some(review) for a review
/// keyword.
pub fn check_text(text: &str, r: &Rules) -> Result<Option<Verdict>, ApiError> {
    for host in crate::text::link_hosts(text) {
        if !crate::text::host_allowed(&host, &r.allow) {
            return Err(ApiError::Rejected { code: "link_not_allowed", message: format!("Links to {host} aren't allowed. {}", reason_text("link")) });
        }
    }
    let mut review: Option<Verdict> = None;
    for (p, action) in &r.keywords {
        if crate::text::matches_keyword(text, p) {
            if action == "block" {
                return Err(ApiError::Rejected { code: "content_blocked", message: format!("\"{p}\" isn't allowed on Kalks Circle. Please read the community rules.") });
            }
            review.get_or_insert(Verdict { decision: Decision::Review, categories: vec!["keyword".into()], reason: format!("Keyword rule: {p}"), source: "rules", model: None });
        }
    }
    Ok(review)
}

pub async fn precheck(st: &AppState, text: &str) -> ApiResult<Option<Verdict>> {
    let r = rules(st).await?;
    check_text(text, &r)
}

// ---------------------------------------------------------------- Claude

const SYSTEM: &str = "You are the content safety classifier of Kalks Circle, a social network for retail traders run by a regulated-style online broker. Members post trading ideas, charts, verified trade cards, memes and questions. Classify the content you are given (text, images, video frames or a document) for public display.

block when the content contains any of:
- nudity, sexual content or sexualised imagery (category nudity / sexual)
- graphic violence, gore, weapons used against people (violence)
- hate speech, slurs, threats, harassment or personal attacks (hate / harassment)
- scams: asking for money, crypto, passwords, OTP codes or account access; fake investment schemes; offers to manage accounts or pass prop challenges for a fee; impersonating Kalks staff or support (scam / impersonation)
- promises of guaranteed, certain or risk-free profit or fixed returns (guaranteed_profit)
- screenshots showing profit, P&L, account balance or equity from another trading platform, broker, exchange or app (MT4, MT5, cTrader, Binance, other brokers' apps, bank statements). On Kalks Circle profit claims are only allowed through Kalks verified trade cards (pnl_screenshot)
- personal data of other people: phone numbers, addresses, ID documents (personal_data)
- self-harm, illegal goods or services (self_harm / illegal)

review (a person decides) when:
- an unverifiable performance claim (win rates, monthly returns) without a verified trade card, or a screenshot of the Kalks app that shows P&L (pnl_screenshot)
- invitations to Telegram, WhatsApp or other groups for signals or paid services (off_platform)
- repetitive advertising or self-promotion (spam)
- you are unsure

allow everything else: market opinions (bullish or bearish), charts and technical analysis (TradingView, Kalks Trader), news, education, questions, jokes and memes, frustration about losses, normal disagreement.

Answer with the decision, the categories that apply (empty for allow) and a short neutral reason (one sentence, for the moderators).";

fn schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false, "required": ["decision", "categories", "reason"],
        "properties": {
            "decision": {"type": "string", "enum": ["allow", "review", "block"]},
            "categories": {"type": "array", "items": {"type": "string", "enum": CATEGORIES}},
            "reason": {"type": "string"}
        }
    })
}

fn verdict_of(v: &Value, model: String) -> Verdict {
    let categories: Vec<String> = v["categories"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|c| CATEGORIES.contains(c)).map(str::to_string).collect();
    let mut decision = Decision::parse(v["decision"].as_str().unwrap_or("review"));
    // a P&L screenshot is never allowed through, even if the model said allow
    if decision == Decision::Allow && !categories.is_empty() {
        decision = Decision::Review;
    }
    Verdict { decision, categories, reason: crate::util::preview(v["reason"].as_str().unwrap_or(""), 300), source: "ai", model: Some(model) }
}

async fn classify(st: &AppState, content: Vec<Value>) -> Result<Verdict, AiError> {
    match ai::call(st, Ask { model: &st.cfg.moderation_model, system: SYSTEM, content, schema: schema(), effort: "low", max_tokens: 2048 }).await {
        Ok((v, model)) => Ok(verdict_of(&v, model)),
        // a declined classification goes to a person
        Err(AiError::Refused) => Ok(Verdict { decision: Decision::Review, categories: vec![], reason: "The AI check declined to classify this item.".into(), source: "ai", model: None }),
        Err(e) => Err(e),
    }
}

/// Text (posts, comments, stories, chat). `kind` tells the model what it is reading.
pub async fn classify_text(st: &AppState, kind: &str, text: &str) -> Result<Verdict, AiError> {
    if text.trim().is_empty() {
        return Ok(Verdict::allow("rules"));
    }
    let prompt = format!("Kind: {kind}\n<content>\n{}\n</content>", crate::util::clean(text, 8000));
    classify(st, vec![json!({"type": "text", "text": prompt})]).await
}

/// Images (photos, chart snapshots, video frames), with the accompanying text if any.
pub async fn classify_images(st: &AppState, kind: &str, images: &[(String, Vec<u8>)], text: &str) -> Result<Verdict, AiError> {
    let mut content: Vec<Value> = images
        .iter()
        .take(10)
        .map(|(mime, bytes)| json!({"type": "image", "source": {"type": "base64", "media_type": mime, "data": STANDARD.encode(bytes)}}))
        .collect();
    content.push(json!({"type": "text", "text": format!("Kind: {kind} ({} image(s)).\n<caption>\n{}\n</caption>", images.len(), crate::util::clean(text, 2000))}));
    classify(st, content).await
}

/// A PDF shared in a chat.
pub async fn classify_pdf(st: &AppState, bytes: &[u8], name: &str) -> Result<Verdict, AiError> {
    let content = vec![
        json!({"type": "document", "source": {"type": "base64", "media_type": "application/pdf", "data": STANDARD.encode(bytes)}}),
        json!({"type": "text", "text": format!("Kind: file shared in a chat, name {name:?}")}),
    ];
    classify(st, content).await
}

/// Puts an item on the staff queue (once while open).
#[allow(clippy::too_many_arguments)]
pub async fn queue(st: &AppState, kind: &str, id: i64, owner: i64, tenant: &str, source: &str, v: &Verdict, excerpt: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO mod_queue (target_kind, target_id, owner, tenant, source, categories, verdict, excerpt) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT (target_kind, target_id) WHERE status = 'open' DO UPDATE SET categories = EXCLUDED.categories, verdict = EXCLUDED.verdict, source = EXCLUDED.source",
    )
    .bind(kind)
    .bind(id)
    .bind(owner)
    .bind(tenant)
    .bind(source)
    .bind(&v.categories)
    .bind(sqlx::types::Json(v.json()))
    .bind(crate::util::preview(excerpt, 500))
    .execute(&st.pool)
    .await?;
    st.hub.send(crate::state::Target::Staff, json!({"type": "queue", "kind": kind, "id": id}));
    Ok(())
}

/// Main category for the user-facing reason.
pub fn main_reason(v: &Verdict) -> String {
    v.categories.first().map(|c| reason_text(c).to_string()).unwrap_or_else(|| "This content breaks the community rules.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules {
            keywords: vec![("guaranteed profit".into(), "block".into()), ("whatsapp me".into(), "review".into())],
            allow: vec!["kalkstrade.com".into(), "youtube.com".into(), "youtu.be".into(), "tradingview.com".into()],
        }
    }

    #[test]
    fn link_allow_list_and_keywords() {
        assert!(check_text("chart: https://www.tradingview.com/x/abc and youtu.be/xyz", &rules()).unwrap().is_none());
        assert!(matches!(check_text("join t.me/signals now", &rules()), Err(ApiError::Rejected { code: "link_not_allowed", .. })));
        assert!(matches!(check_text("visit https://evil.example/kalkstrade.com", &rules()), Err(ApiError::Rejected { code: "link_not_allowed", .. })));
        assert!(matches!(check_text("This is a GUARANTEED profit setup", &rules()), Err(ApiError::Rejected { code: "content_blocked", .. })));
        let v = check_text("Whatsapp me for more", &rules()).unwrap().unwrap();
        assert_eq!(v.decision, Decision::Review);
        assert!(check_text("Gold looks strong above 2400 #gold $XAUUSD", &rules()).unwrap().is_none());
    }

    #[test]
    fn verdicts() {
        let v = verdict_of(&json!({"decision": "block", "categories": ["pnl_screenshot", "made_up"], "reason": "MT5 balance"}), "m".into());
        assert_eq!((v.decision, v.categories.clone()), (Decision::Block, vec!["pnl_screenshot".to_string()]));
        let allow_with_cat = verdict_of(&json!({"decision": "allow", "categories": ["spam"], "reason": ""}), "m".into());
        assert_eq!(allow_with_cat.decision, Decision::Review);
        let merged = Verdict::allow("ai").merge(v.clone());
        assert_eq!(merged.decision, Decision::Block);
        assert_eq!(main_reason(&merged), reason_text("pnl_screenshot"));
        let review = Verdict { decision: Decision::Review, categories: vec!["spam".into()], reason: "r".into(), source: "ai", model: None };
        let m = v.merge(review);
        assert_eq!(m.decision, Decision::Block);
        assert_eq!(m.categories, vec!["pnl_screenshot".to_string(), "spam".to_string()]);
    }
}
