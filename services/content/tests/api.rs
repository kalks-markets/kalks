//! Integration test against PostgreSQL: drives the HTTP API end to end — media upload (sniffing, SVG refusal,
//! dedupe, WebP variants, dark file, alt text, public serving), drafts → publish → versions → rollback → unpublish,
//! version pruning, English fallback, preview tokens, ETag / 304, permissions, the internal token, audit and
//! tenant isolation.
//!
//! Needs the local Postgres (127.0.0.1:5433). Uses a throw-away database `kalks_content_test_<pid>`; skipped with a
//! message when Postgres is not reachable. Override with CONTENT_TEST_DATABASE_URL (a server URL).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::io::Cursor;
use std::str::FromStr;
use std::sync::Arc;
use tower::ServiceExt;

use content::api::{AppState, router};
use content::config::Config;
use content::{storage, store};

async fn test_db() -> Option<String> {
    let server = std::env::var("CONTENT_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let opts = PgConnectOptions::from_str(&server).ok()?;
    let db = format!("kalks_content_test_{}", std::process::id());
    let mut conn = match opts.clone().database("postgres").connect().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skipping: Postgres not reachable ({e})");
            return None;
        }
    };
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\""))).execute(&mut conn).await;
    let base = server.rsplit_once('/').map(|x| x.0).unwrap_or(&server).to_string();
    Some(format!("{base}/{db}"))
}

async fn drop_db(url: &str) {
    let opts = PgConnectOptions::from_str(url).unwrap();
    let db = opts.get_database().unwrap().to_string();
    if let Ok(mut c) = opts.database("postgres").connect().await {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut c).await;
    }
}

const ALL: &str = "content.read,content.write,content.publish";

struct Res {
    status: StatusCode,
    json: Value,
    bytes: Vec<u8>,
    headers: axum::http::HeaderMap,
}

struct Client {
    app: axum::Router,
}

impl Client {
    async fn raw(&self, method: &str, path: &str, headers: &[(&str, &str)], body: Vec<u8>) -> Res {
        let mut rb = Request::builder().method(method).uri(path);
        for (k, v) in headers {
            rb = rb.header(*k, *v);
        }
        let res = self.app.clone().oneshot(rb.body(Body::from(body)).unwrap()).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Res { status, json, bytes, headers }
    }

    /// Staff call for a tenant with every content permission.
    async fn staff(&self, tenant: &str, method: &str, path: &str, body: Option<Value>) -> Res {
        self.staff_as(tenant, ALL, method, path, body).await
    }

    async fn staff_as(&self, tenant: &str, perms: &str, method: &str, path: &str, body: Option<Value>) -> Res {
        let h = [("x-kalks-internal", "test-token"), ("x-kalks-tenant", tenant), ("x-kalks-staff", "editor@kalks.test"), ("x-kalks-staff-perms", perms), ("content-type", "application/json")];
        self.raw(method, path, &h, body.map(|b| serde_json::to_vec(&b).unwrap()).unwrap_or_default()).await
    }

    async fn upload(&self, tenant: &str, path: &str, name: &str, bytes: Vec<u8>) -> Res {
        let h = [("x-kalks-internal", "test-token"), ("x-kalks-tenant", tenant), ("x-kalks-staff", "editor@kalks.test"), ("x-kalks-staff-perms", ALL), ("x-file-name", name)];
        self.raw("POST", path, &h, bytes).await
    }

    async fn public(&self, path: &str) -> Res {
        self.raw("GET", path, &[], vec![]).await
    }
}

fn png(w: u32, h: u32, shade: u8) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([shade, (x % 256) as u8, (y % 256) as u8]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// The path of a public URL (the test config's public base is https://api.test).
fn path_of(url: &Value) -> String {
    url.as_str().unwrap().strip_prefix("https://api.test").unwrap().to_string()
}

#[tokio::test]
async fn content_service_end_to_end() {
    let Some(url) = test_db().await else { return };
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::for_tests(&url, dir.path().to_str().unwrap());
    let pool = store::connect(&url).await.unwrap();
    let st = AppState { pool: pool.clone(), storage: storage::from_config(&cfg).unwrap(), cfg: Arc::new(cfg) };
    let c = Client { app: router(st) };

    /* ---------------- health, internal token, permissions ---------------- */
    let r = c.public("/health").await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["db"], true);
    let r = c.raw("GET", "/v1/admin/content/tree", &[("x-kalks-staff", "x@y")], vec![]).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "admin routes need the internal token");
    let r = c.raw("GET", "/v1/admin/content/tree", &[("x-kalks-internal", "test-token")], vec![]).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "and a staff member");
    let r = c.staff_as("kalks", "content.read", "PUT", "/v1/admin/content/entry", Some(json!({"surface": "website", "page": "home", "slot": "hero.title", "kind": "text", "locale": "en", "value": {"text": "x"}}))).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "read-only staff can't write");

    /* ---------------- media: sniffing, SVG checks, variants ---------------- */
    let r = c.upload("kalks", "/v1/admin/media", "doc.pdf", b"%PDF-1.7 nope".to_vec()).await;
    assert_eq!(r.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let r = c.upload("kalks", "/v1/admin/media", "evil.svg", br#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"/>"#.to_vec()).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(r.json["error"]["message"].as_str().unwrap().contains("event handler"));
    let r = c.upload("kalks", "/v1/admin/media", "logo.svg", br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="#f60"/></svg>"##.to_vec()).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let svg = r.json["asset"].clone();
    assert_eq!(svg["light"]["mime"], "image/svg+xml");
    let served = c.public(&path_of(&svg["light"]["url"])).await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.headers["content-type"], "image/svg+xml");
    assert!(served.headers["content-security-policy"].to_str().unwrap().contains("sandbox"));
    // asking an SVG for a raster variant serves the original
    let r = c.public(&path_of(&svg["light"]["url"]).replace("/orig", "/md")).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.headers["content-type"], "image/svg+xml");

    let hero = png(1200, 600, 200);
    let r = c.upload("kalks", "/v1/admin/media", "Hero%20light.png", hero.clone()).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let asset = r.json["asset"].clone();
    let id = asset["id"].as_str().unwrap().to_string();
    assert_eq!(asset["name"], "Hero light");
    assert_eq!(asset["light"]["width"], 1200);
    assert!(asset["light"]["variants"]["sm"]["url"].is_string() && asset["light"]["variants"]["md"]["url"].is_string());
    assert!(asset["light"]["variants"].get("lg").is_none(), "no upscaling");
    assert!(asset["light"]["srcset"].as_str().unwrap().contains(" 480w") && asset["light"]["srcset"].as_str().unwrap().contains(" 1200w"));
    // the same picture again is the same library entry
    let r = c.upload("kalks", "/v1/admin/media", "again.png", hero.clone()).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["existing"], true);
    assert_eq!(r.json["asset"]["id"], id.as_str());
    // public variant: WebP, immutable caching
    let sm = c.public(&path_of(&asset["light"]["variants"]["sm"]["url"])).await;
    assert_eq!(sm.status, StatusCode::OK);
    assert_eq!(sm.headers["content-type"], "image/webp");
    assert!(sm.headers["cache-control"].to_str().unwrap().contains("immutable"));
    assert_eq!(image::load_from_memory(&sm.bytes).unwrap().width(), 480);
    // lg doesn't exist: the next larger rendition (the original) is served
    let lg = c.public(&path_of(&asset["light"]["url"]).replace("/orig", "/lg")).await;
    assert_eq!(lg.headers["content-type"], "image/png");
    assert_eq!(c.public("/v1/public/media/0123/orig").await.status, StatusCode::NOT_FOUND);
    assert_eq!(c.public(&path_of(&asset["light"]["url"]).replace("/orig", "/xl")).await.status, StatusCode::NOT_FOUND);

    // dark file + alt text
    let r = c.upload("kalks", &format!("/v1/admin/media/{id}/file?theme=dark"), "hero-dark.png", png(800, 400, 20)).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["asset"]["dark"]["width"], 800);
    let r = c.staff("kalks", "PUT", &format!("/v1/admin/media/{id}"), Some(json!({"alt": {"en": "Trader at a desk", "ar": "متداول"}}))).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["asset"]["alt"]["ar"], "متداول");
    assert_eq!(c.staff("kalks", "PUT", &format!("/v1/admin/media/{id}"), Some(json!({"alt": {"xx": "?"}}))).await.status, StatusCode::UNPROCESSABLE_ENTITY);
    let r = c.staff("kalks", "GET", "/v1/admin/media?q=hero", None).await;
    assert_eq!(r.json["total"], 1);

    /* ---------------- drafts, publish, fallback ---------------- */
    let put = |slot: &str, kind: &str, locale: &str, value: Value| json!({"surface": "website", "page": "home", "slot": slot, "kind": kind, "locale": locale, "value": value, "label": "Hero"});
    for (slot, kind, value) in [
        ("hero.title", "text", json!({"text": "Trade the world"})),
        ("hero.image", "image", json!({"media": id})),
        ("hero.cta", "link", json!({"label": "Open account", "href": "/register"})),
    ] {
        let r = c.staff("kalks", "PUT", "/v1/admin/content/entry", Some(put(slot, kind, "en", value))).await;
        assert_eq!(r.status, StatusCode::OK, "{slot}: {:?}", r.json);
    }
    assert_eq!(c.staff("kalks", "PUT", "/v1/admin/content/entry", Some(put("hero.title", "image", "en", json!({"media": id})))).await.status, StatusCode::CONFLICT, "kind is fixed per slot");
    assert_eq!(c.staff("kalks", "PUT", "/v1/admin/content/entry", Some(put("hero.cta", "link", "en", json!({"label": "x", "href": "javascript:alert(1)"})))).await.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(c.staff("kalks", "PUT", "/v1/admin/content/entry", Some(put("hero.image", "image", "en", json!({"media": "m000000000000000000"})))).await.status, StatusCode::UNPROCESSABLE_ENTITY, "unknown image");

    // nothing is public before publishing
    let r = c.public("/v1/public/content?surface=website&page=home&locale=en").await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["pages"]["home"], json!({}));
    // publishing needs content.publish
    assert_eq!(c.staff_as("kalks", "content.read,content.write", "POST", "/v1/admin/content/publish", Some(json!({"surface": "website", "page": "home"}))).await.status, StatusCode::FORBIDDEN);
    let r = c.staff("kalks", "POST", "/v1/admin/content/publish", Some(json!({"surface": "website", "page": "home"}))).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["published"].as_array().unwrap().len(), 3);

    let r = c.public("/v1/public/content?surface=website&page=home&locale=en").await;
    let home = &r.json["pages"]["home"];
    assert_eq!(home["hero.title"]["value"]["text"], "Trade the world");
    assert_eq!(home["hero.cta"]["value"]["href"], "/register");
    let img = &home["hero.image"]["value"];
    assert_eq!(img["alt"], "Trader at a desk");
    assert!(img["url"].as_str().unwrap().starts_with("https://api.test/v1/public/media/"));
    assert!(img["dark"]["url"].is_string() && img["dark"]["srcset"].as_str().unwrap().contains("480w"), "dark variant resolved");
    assert!(r.headers["cache-control"].to_str().unwrap().contains("max-age=60"));
    let etag = r.headers["etag"].to_str().unwrap().to_string();
    let r304 = c.raw("GET", "/v1/public/content?surface=website&page=home&locale=en", &[("if-none-match", &etag)], vec![]).await;
    assert_eq!(r304.status, StatusCode::NOT_MODIFIED);

    // Arabic: title translated, everything else falls back to English (alt text in Arabic)
    c.staff("kalks", "PUT", "/v1/admin/content/entry", Some(put("hero.title", "text", "ar", json!({"text": "تداول العالم"})))).await;
    let r = c.public("/v1/public/content?surface=website&page=home&locale=ar").await;
    assert_eq!(r.json["pages"]["home"]["hero.title"]["locale"], "en", "an unpublished translation isn't shown");
    c.staff("kalks", "POST", "/v1/admin/content/publish", Some(json!({"surface": "website", "page": "home", "slot": "hero.title", "locale": "ar"}))).await;
    let r = c.public("/v1/public/content?surface=website&page=home&locale=ar-SA").await;
    let home = &r.json["pages"]["home"];
    assert_eq!(r.json["locale"], "ar");
    assert_eq!(home["hero.title"]["value"]["text"], "تداول العالم");
    assert_eq!(home["hero.title"]["locale"], "ar");
    assert_eq!(home["hero.cta"]["locale"], "en");
    assert_eq!(home["hero.image"]["value"]["alt"], "متداول");
    // an unknown locale reads English
    assert_eq!(c.public("/v1/public/content?surface=website&page=home&locale=xx").await.json["locale"], "en");

    /* ---------------- preview ---------------- */
    c.staff("kalks", "PUT", "/v1/admin/content/entry", Some(put("hero.title", "text", "en", json!({"text": "Draft headline"})))).await;
    assert_eq!(c.public("/v1/public/content?surface=website&page=home").await.json["pages"]["home"]["hero.title"]["value"]["text"], "Trade the world");
    let tok = c.staff("kalks", "POST", "/v1/admin/content/preview-token", Some(json!({"surface": "website"}))).await.json["token"].as_str().unwrap().to_string();
    let r = c.public(&format!("/v1/public/content?surface=website&page=home&preview={tok}")).await;
    assert_eq!(r.json["preview"], true);
    assert_eq!(r.json["pages"]["home"]["hero.title"]["value"]["text"], "Draft headline");
    assert!(r.headers["cache-control"].to_str().unwrap().contains("no-store"));
    // the token is scoped to its tenant and surface
    assert_eq!(c.public(&format!("/v1/public/content?surface=app&page=home&preview={tok}")).await.status, StatusCode::FORBIDDEN);
    assert_eq!(c.public(&format!("/v1/public/content?surface=website&tenant=other&preview={tok}")).await.status, StatusCode::FORBIDDEN);
    assert_eq!(c.public("/v1/public/content?surface=website&preview=forged.token").await.status, StatusCode::FORBIDDEN);

    /* ---------------- versions, rollback, unpublish ---------------- */
    c.staff("kalks", "POST", "/v1/admin/content/publish", Some(json!({"surface": "website", "page": "home", "slot": "hero.title", "locale": "en"}))).await;
    let v = c.staff("kalks", "GET", "/v1/admin/content/versions?surface=website&page=home&slot=hero.title&locale=en", None).await.json;
    assert_eq!(v["current"], 2);
    assert_eq!(v["versions"][0]["value"]["text"], "Draft headline");
    // rollback to v1 creates v3 with v1's value
    let r = c.staff("kalks", "POST", "/v1/admin/content/rollback", Some(json!({"surface": "website", "page": "home", "slot": "hero.title", "locale": "en", "version": 1}))).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["version"], 3);
    assert_eq!(c.public("/v1/public/content?surface=website&page=home").await.json["pages"]["home"]["hero.title"]["value"]["text"], "Trade the world");
    let v = c.staff("kalks", "GET", "/v1/admin/content/versions?surface=website&page=home&slot=hero.title&locale=en", None).await.json;
    assert_eq!(v["versions"][0]["action"], "rollback");
    assert_eq!(v["versions"][0]["from_version"], 1);
    assert_eq!(c.staff("kalks", "POST", "/v1/admin/content/rollback", Some(json!({"surface": "website", "page": "home", "slot": "hero.title", "locale": "en", "version": 99}))).await.status, StatusCode::NOT_FOUND);
    // unpublish: the slot is gone for readers (the app default shows), and can be rolled back
    let r = c.staff("kalks", "POST", "/v1/admin/content/unpublish", Some(json!({"surface": "website", "page": "home", "slot": "hero.cta", "locale": "en"}))).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(c.public("/v1/public/content?surface=website&page=home").await.json["pages"]["home"].get("hero.cta").is_none());
    c.staff("kalks", "POST", "/v1/admin/content/rollback", Some(json!({"surface": "website", "page": "home", "slot": "hero.cta", "locale": "en", "version": 1}))).await;
    assert_eq!(c.public("/v1/public/content?surface=website&page=home").await.json["pages"]["home"]["hero.cta"]["value"]["label"], "Open account");

    // only the last 50 versions are kept
    for i in 0..55 {
        c.staff("kalks", "PUT", "/v1/admin/content/entry", Some(json!({"surface": "website", "page": "faq", "slot": "intro", "kind": "rich_text", "locale": "en", "value": {"markdown": format!("Version {i}")}}))).await;
        c.staff("kalks", "POST", "/v1/admin/content/publish", Some(json!({"surface": "website", "page": "faq"}))).await;
    }
    let v = c.staff("kalks", "GET", "/v1/admin/content/versions?surface=website&page=faq&slot=intro&locale=en", None).await.json;
    assert_eq!(v["current"], 55);
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM content_versions WHERE tenant = 'kalks' AND page = 'faq'").fetch_one(&pool).await.unwrap();
    assert_eq!(kept, 50);
    assert_eq!(c.staff("kalks", "POST", "/v1/admin/content/rollback", Some(json!({"surface": "website", "page": "faq", "slot": "intro", "locale": "en", "version": 3}))).await.status, StatusCode::NOT_FOUND, "pruned");
    // a draft equal to the published value is no draft
    c.staff("kalks", "PUT", "/v1/admin/content/entry", Some(json!({"surface": "website", "page": "faq", "slot": "intro", "kind": "rich_text", "locale": "en", "value": {"markdown": "Version 54"}}))).await;
    let s = c.staff("kalks", "GET", "/v1/admin/content/slot?surface=website&page=faq&slot=intro", None).await.json;
    assert!(s["entries"]["en"]["draft"].is_null());

    // several pages in one read; all pages when none is given
    let r = c.public("/v1/public/content?surface=website&page=home,faq").await;
    assert!(r.json["pages"]["faq"]["intro"].is_object() && r.json["pages"]["home"]["hero.title"].is_object());
    assert!(c.public("/v1/public/content?surface=website").await.json["pages"]["faq"].is_object());
    assert_eq!(c.public("/v1/public/content?surface=nope").await.status, StatusCode::UNPROCESSABLE_ENTITY);

    /* ---------------- media usage and delete ---------------- */
    let r = c.staff("kalks", "GET", &format!("/v1/admin/media/{id}"), None).await;
    assert_eq!(r.json["usage"][0]["slot"], "hero.image");
    assert_eq!(r.json["usage"][0]["published"], true);
    let r = c.staff("kalks", "DELETE", &format!("/v1/admin/media/{id}"), None).await;
    assert_eq!(r.status, StatusCode::CONFLICT, "in use");
    // replacing a live image needs content.publish
    assert_eq!(c.staff_as("kalks", "content.read,content.write", "DELETE", &format!("/v1/admin/media/{id}/file?theme=dark"), None).await.status, StatusCode::FORBIDDEN);
    let svg_id = svg["id"].as_str().unwrap();
    assert_eq!(c.staff("kalks", "DELETE", &format!("/v1/admin/media/{svg_id}"), None).await.status, StatusCode::OK);
    assert_eq!(c.staff("kalks", "GET", &format!("/v1/admin/media/{svg_id}"), None).await.status, StatusCode::NOT_FOUND);

    // slot delete
    assert_eq!(c.staff("kalks", "DELETE", "/v1/admin/content/slot?surface=website&page=faq&slot=intro", None).await.status, StatusCode::OK);
    assert!(c.public("/v1/public/content?surface=website&page=faq").await.json["pages"]["faq"].get("intro").is_none());

    /* ---------------- import ---------------- */
    let r = c
        .staff(
            "kalks",
            "POST",
            "/v1/admin/content/import",
            Some(json!({"surface": "app", "publish": true, "entries": [
                {"page": "dashboard", "slot": "hero.title", "kind": "text", "locale": "en", "value": {"text": "Welcome back"}},
                {"page": "dashboard", "slot": "hero.title", "kind": "text", "locale": "de", "value": {"text": "Willkommen zurück"}},
            ]})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["published"].as_array().unwrap().len(), 2);
    // the same import again changes nothing
    let r = c.staff("kalks", "POST", "/v1/admin/content/import", Some(json!({"surface": "app", "publish": true, "entries": [{"page": "dashboard", "slot": "hero.title", "kind": "text", "locale": "en", "value": {"text": "Welcome back"}}]}))).await;
    assert_eq!(r.json["published"], json!([]));

    /* ---------------- tenant isolation ---------------- */
    let r = c.staff("acme", "GET", "/v1/admin/media", None).await;
    assert_eq!(r.json["total"], 0);
    assert_eq!(c.staff("acme", "GET", &format!("/v1/admin/media/{id}"), None).await.status, StatusCode::NOT_FOUND);
    let r = c.staff("acme", "PUT", "/v1/admin/content/entry", Some(json!({"surface": "website", "page": "home", "slot": "hero.image", "kind": "image", "locale": "en", "value": {"media": id}}))).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "another broker's image can't be used");
    assert_eq!(c.public("/v1/public/content?surface=website&page=home&tenant=acme").await.json["pages"]["home"], json!({}));
    let r = c.raw("GET", "/v1/public/content?surface=website&page=home", &[("x-kalks-tenant", "acme")], vec![]).await;
    assert_eq!(r.json["tenant"], "acme");
    assert_eq!(r.json["pages"]["home"], json!({}));
    assert_eq!(c.staff("acme", "GET", "/v1/admin/content/tree", None).await.json["pages"], json!([]));
    // the same bytes uploaded by another broker are a separate file
    let r = c.upload("acme", "/v1/admin/media", "hero.png", hero.clone()).await;
    assert_eq!(r.status, StatusCode::CREATED);
    assert_ne!(r.json["asset"]["light"]["id"], asset["light"]["id"]);

    /* ---------------- audit ---------------- */
    let a = c.staff("kalks", "GET", "/v1/admin/content/audit", None).await.json;
    let actions: Vec<&str> = a["audit"].as_array().unwrap().iter().map(|x| x["action"].as_str().unwrap()).collect();
    for want in ["content.publish", "content.rollback", "content.unpublish", "content.delete", "content.import", "media.replace", "media.delete"] {
        assert!(actions.contains(&want), "audit has {want}: {actions:?}");
    }
    assert_eq!(c.staff("acme", "GET", "/v1/admin/content/audit", None).await.json["audit"], json!([]));

    pool.close().await;
    drop_db(&url).await;
}
