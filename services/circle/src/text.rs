//! Text helpers: #hashtags, $cashtags, @mentions, links, handles, language guess, hidden words, the risk line.

use std::collections::BTreeSet;

pub const RISK_LINE: &str = "Not investment advice. Trading involves risk.";

/// The 22 languages of the apps.
pub const LANGS: &[&str] = &["en", "ar", "bn", "de", "es", "fa", "fr", "hi", "id", "it", "ja", "ko", "ms", "pt", "ru", "sw", "ta", "th", "tr", "ur", "vi", "zh"];

pub fn lang_name(code: &str) -> &'static str {
    match code {
        "ar" => "Arabic",
        "bn" => "Bengali",
        "de" => "German",
        "es" => "Spanish",
        "fa" => "Persian",
        "fr" => "French",
        "hi" => "Hindi",
        "id" => "Indonesian",
        "it" => "Italian",
        "ja" => "Japanese",
        "ko" => "Korean",
        "ms" => "Malay",
        "pt" => "Portuguese",
        "ru" => "Russian",
        "sw" => "Swahili",
        "ta" => "Tamil",
        "th" => "Thai",
        "tr" => "Turkish",
        "ur" => "Urdu",
        "vi" => "Vietnamese",
        "zh" => "Chinese (Simplified)",
        _ => "English",
    }
}

pub fn valid_lang(l: &str) -> Option<&'static str> {
    let l = l.trim().to_ascii_lowercase();
    let l = l.split(['-', '_']).next().unwrap_or("");
    LANGS.iter().find(|x| **x == l).copied()
}

#[derive(Debug, Default, PartialEq)]
pub struct Parsed {
    pub hashtags: Vec<String>,
    pub cashtags: Vec<String>,
    pub mentions: Vec<String>,
    /// Lower-case hosts of the links found in the text.
    pub links: Vec<String>,
}

fn tag_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn handle_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.'
}

/// Extracts hashtags (lower case), cashtags (upper case), mentions (handles) and link hosts.
pub fn parse(text: &str) -> Parsed {
    let chars: Vec<char> = text.chars().collect();
    let mut hashtags = BTreeSet::new();
    let mut cashtags = BTreeSet::new();
    let mut mentions = BTreeSet::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let boundary = i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_' || chars[i - 1] == '/' || chars[i - 1] == '&');
        if boundary && (c == '#' || c == '$' || c == '@') {
            let mut j = i + 1;
            match c {
                '#' => {
                    while j < chars.len() && tag_char(chars[j]) && j - i <= 50 {
                        j += 1;
                    }
                    let tag: String = chars[i + 1..j].iter().collect::<String>().to_lowercase();
                    if !tag.is_empty() && !tag.chars().all(|x| x.is_ascii_digit()) && hashtags.len() < 30 {
                        hashtags.insert(tag);
                    }
                }
                '$' => {
                    while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '.') && j - i <= 16 {
                        j += 1;
                    }
                    let mut sym: String = chars[i + 1..j].iter().collect::<String>().to_uppercase();
                    while sym.ends_with('.') {
                        sym.pop();
                        j -= 1;
                    }
                    if crate::upstream::is_symbol(&sym) && cashtags.len() < 10 {
                        cashtags.insert(sym);
                    }
                }
                _ => {
                    while j < chars.len() && handle_char(chars[j].to_ascii_lowercase()) && chars[j].is_ascii() && j - i <= 24 {
                        j += 1;
                    }
                    let mut h: String = chars[i + 1..j].iter().collect::<String>().to_lowercase();
                    while h.ends_with('.') {
                        h.pop();
                    }
                    if (3..=24).contains(&h.len()) && mentions.len() < 20 {
                        mentions.insert(h);
                    }
                }
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    Parsed { hashtags: hashtags.into_iter().collect(), cashtags: cashtags.into_iter().collect(), mentions: mentions.into_iter().collect(), links: link_hosts(text) }
}

/// Hosts of the links in a text: `http(s)://…`, `www.…` and bare domains (`example.com/x`, `t.me/abc`).
pub fn link_hosts(text: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    for raw in text.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '<' | '>' | '"' | '\'' | '[' | ']' | '{' | '}' | ',')) {
        let t = raw.trim_matches(|c: char| matches!(c, '.' | '!' | '?' | ';' | ':' | '*' | '_'));
        if t.is_empty() || t.starts_with('#') || t.starts_with('$') || t.starts_with('@') {
            continue;
        }
        let lower = t.to_lowercase();
        let (explicit, rest) = if let Some(r) = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")) {
            (true, r.to_string())
        } else if let Some(r) = lower.strip_prefix("www.") {
            (true, format!("www.{r}"))
        } else {
            (false, lower.clone())
        };
        // e-mail addresses are not links
        if !explicit && rest.split(['/', '?', '#']).next().unwrap_or("").contains('@') {
            continue;
        }
        let host = rest.split(['/', '?', '#']).next().unwrap_or("").split('@').next_back().unwrap_or("").split(':').next().unwrap_or("").trim_end_matches('.').to_string();
        if host.is_empty() {
            continue;
        }
        let labels: Vec<&str> = host.split('.').collect();
        let tld = labels.last().copied().unwrap_or("");
        let looks_like_domain = labels.len() >= 2
            && labels.iter().all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            && (2..=24).contains(&tld.len())
            && tld.chars().all(|c| c.is_ascii_alphabetic())
            && labels[..labels.len() - 1].iter().any(|l| l.chars().any(|c| c.is_ascii_alphabetic()));
        // a bare "word.word" counts only with a path or a well-known TLD, so "e.g." or "XAUUSD.m" are not links
        let common = ["com", "net", "org", "io", "me", "ly", "co", "gg", "app", "info", "biz", "xyz", "link", "site", "online", "top", "vip", "club", "pro", "in", "uk", "ru", "cn", "ai"];
        if looks_like_domain && (explicit || rest.contains('/') || common.contains(&tld)) {
            out.insert(host.trim_start_matches("www.").to_string());
        }
    }
    out.into_iter().collect()
}

/// Whether `host` is on the allow-list (`pattern` matches the host or any parent domain).
pub fn host_allowed(host: &str, allow: &[String]) -> bool {
    let h = host.to_ascii_lowercase();
    allow.iter().any(|p| {
        let p = p.trim().trim_start_matches("*.").to_ascii_lowercase();
        !p.is_empty() && (h == p || h.ends_with(&format!(".{p}")))
    })
}

pub const RESERVED_HANDLES: &[&str] = &[
    "admin", "administrator", "kalks", "kalkstrade", "kalksofficial", "official", "support", "help", "staff", "team", "mentor", "moderator", "mod", "circle", "system",
    "root", "security", "compliance", "billing", "news", "academy", "settings", "explore", "me", "api", "null", "undefined",
];

/// Normalises and validates a handle: 3–24 of `a-z 0-9 _ .`, no leading / trailing / double dots, not reserved.
pub fn handle(raw: &str) -> Result<String, &'static str> {
    let h = raw.trim().trim_start_matches('@').to_lowercase();
    if h.len() < 3 || h.len() > 24 {
        return Err("Handles are 3 to 24 characters.");
    }
    if !h.chars().all(handle_char) {
        return Err("Use letters, numbers, underscores and dots only.");
    }
    if h.starts_with('.') || h.ends_with('.') || h.contains("..") {
        return Err("A handle can't start or end with a dot or have two dots in a row.");
    }
    if RESERVED_HANDLES.contains(&h.as_str()) || h.starts_with("kalks") {
        return Err("This handle is reserved.");
    }
    Ok(h)
}

/// A handle suggestion from a display name (ASCII letters / digits only; may need a suffix to be unique).
pub fn handle_base(name: &str) -> String {
    let mut base: String = name.to_lowercase().chars().filter_map(|c| if c.is_ascii_alphanumeric() { Some(c) } else if c == ' ' || c == '.' || c == '_' { Some('_') } else { None }).collect();
    while base.contains("__") {
        base = base.replace("__", "_");
    }
    let mut base = base.trim_matches('_').to_string();
    base.truncate(16);
    if base.len() < 3 || handle(&base).is_err() {
        base = "trader".into();
    }
    base
}

/// Script-based language guess for non-Latin text; Latin text keeps the author's language.
pub fn guess_lang(text: &str, fallback: &str) -> String {
    let mut counts: std::collections::HashMap<&str, usize> = Default::default();
    let mut letters = 0usize;
    for c in text.chars().filter(|c| c.is_alphabetic()) {
        letters += 1;
        let u = c as u32;
        let k = match u {
            0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => "arabic",
            0x0980..=0x09FF => "bn",
            0x0900..=0x097F => "hi",
            0x0B80..=0x0BFF => "ta",
            0x0E00..=0x0E7F => "th",
            0x0400..=0x04FF => "ru",
            0x3040..=0x30FF => "ja",
            0xAC00..=0xD7AF | 0x1100..=0x11FF => "ko",
            0x4E00..=0x9FFF => "han",
            _ => "latin",
        };
        *counts.entry(k).or_default() += 1;
    }
    if letters == 0 {
        return fallback.to_string();
    }
    let (top, n) = counts.iter().max_by_key(|(_, n)| **n).map(|(k, n)| (*k, *n)).unwrap_or(("latin", 0));
    if n * 2 < letters || top == "latin" {
        return fallback.to_string();
    }
    match top {
        // Arabic script: Persian / Urdu authors keep their language, everyone else is Arabic
        "arabic" => if ["fa", "ur"].contains(&fallback) { fallback.to_string() } else { "ar".into() },
        // Han only: Japanese authors keep ja (kanji), otherwise Chinese
        "han" => if fallback == "ja" { "ja".into() } else { "zh".into() },
        other => other.to_string(),
    }
}

/// Whether `text` contains one of the hidden words (case-insensitive, whole words or phrases).
pub fn has_hidden_word(text: &str, words: &[String]) -> bool {
    if words.is_empty() {
        return false;
    }
    let norm: String = text.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let padded = format!(" {} ", norm.split_whitespace().collect::<Vec<_>>().join(" "));
    words.iter().any(|w| {
        let w: String = w.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
        let w = w.split_whitespace().collect::<Vec<_>>().join(" ");
        !w.is_empty() && padded.contains(&format!(" {w} "))
    })
}

/// Keyword rule match: case-insensitive, punctuation-insensitive phrase match.
pub fn matches_keyword(text: &str, pattern: &str) -> bool {
    has_hidden_word(text, &[pattern.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tags_mentions_and_links() {
        let p = parse("Long $XAUUSD and $eurusd. #Gold #breakout #2024 @Ana_Trader @x hi@mail.com $100 see https://www.TradingView.com/x and t.me/scam");
        assert_eq!(p.cashtags, vec!["EURUSD", "XAUUSD"]);
        assert_eq!(p.hashtags, vec!["breakout", "gold"]);
        assert_eq!(p.mentions, vec!["ana_trader"]);
        assert_eq!(p.links, vec!["t.me", "tradingview.com"]);
        assert!(parse("e.g. price 1.2345 and XAUUSD.m, U.S. data").links.is_empty());
        assert_eq!(parse("visit kalkstrade.com today").links, vec!["kalkstrade.com"]);
        assert_eq!(parse("#ünïcode #日本").hashtags, vec!["ünïcode", "日本"]);
    }

    #[test]
    fn allow_list_matches_parents() {
        let allow = vec!["kalkstrade.com".to_string(), "youtube.com".to_string(), "youtu.be".to_string()];
        assert!(host_allowed("app.kalkstrade.com", &allow));
        assert!(host_allowed("youtube.com", &allow) && host_allowed("m.youtube.com", &allow));
        assert!(!host_allowed("kalkstrade.com.evil.io", &allow));
        assert!(!host_allowed("notyoutube.com", &allow));
    }

    #[test]
    fn handles() {
        assert_eq!(handle("@Ana_Trader"), Ok("ana_trader".into()));
        assert!(handle("ab").is_err() && handle("a..b").is_err() && handle(".abc").is_err() && handle("admin").is_err() && handle("kalks_team").is_err());
        assert!(handle("ana-trader").is_err());
        assert_eq!(handle_base("José María Pérez"), "jos_mara_prez");
        assert_eq!(handle_base("李"), "trader");
    }

    #[test]
    fn guesses_languages_and_hides_words() {
        assert_eq!(guess_lang("Золото растёт", "en"), "ru");
        assert_eq!(guess_lang("الذهب يرتفع", "en"), "ar");
        assert_eq!(guess_lang("الذهب يرتفع", "ur"), "ur");
        assert_eq!(guess_lang("Gold is up", "de"), "de");
        assert_eq!(guess_lang("黄金上涨", "en"), "zh");
        assert_eq!(guess_lang("1234", "es"), "es");
        let w = vec!["pump".to_string(), "free money".to_string()];
        assert!(has_hidden_word("Total PUMP incoming", &w));
        assert!(has_hidden_word("get free-money now", &w));
        assert!(!has_hidden_word("pumpkin season", &w));
        assert!(matches_keyword("This is a Guaranteed  profit!", "guaranteed profit"));
        assert_eq!(valid_lang("pt-BR"), Some("pt"));
        assert_eq!(valid_lang("xx"), None);
    }
}
