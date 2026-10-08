//! Integration test against PostgreSQL: seeds a small fixture course, then drives the HTTP API end to end —
//! catalogue, chapter view (no answers leaked), reading progress, quizzes, exam lock / pass / certificate,
//! public verification, the certificate image, idempotent re-seeding and the Back Office CMS (tenant override,
//! unpublish, reset, create, reorder, stats).
//!
//! Needs the local Postgres (127.0.0.1:5433). Uses a throw-away database `kalks_academy_test_<pid>`; skipped
//! with a message when Postgres is not reachable. Override with ACADEMY_TEST_DATABASE_URL (a server URL).
//! Also lints the real content/academy tree (the same check as `academy-lint`) and checks the shape of the
//! phase 9 elective (Kalks FX Options: one `options` section, examined chapter by chapter). The fixture has a
//! core phase and an options-only product phase, which is studied, examined and certified independently.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tower::ServiceExt;

use academy::api::{AppState, router};
use academy::config::Config;
use academy::content;
use academy::store;

fn words(n: usize) -> String {
    (0..n).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ")
}

fn chapter(slug: &str, order: i32, answer: usize) -> String {
    format!(
        "---\nslug: \"{slug}\"\ntitle: \"Chapter {slug}\"\nsummary: \"Summary.\"\norder: {order}\ntakeaways:\n  - \"One.\"\n  - \"Two.\"\n  - \"Three.\"\nquiz:\n  - question: \"Q1?\"\n    options: [\"a\", \"b\", \"c\", \"d\"]\n    answer: {answer}\n    explanation: \"Because.\"\n  - question: \"Q2?\"\n    options: [\"a\", \"b\", \"c\"]\n    answer: 0\n    explanation: \"Because.\"\n  - question: \"Q3?\"\n    options: [\"a\", \"b\", \"c\"]\n    answer: 2\n    explanation: \"Because.\"\n---\n\nIntro.\n\n## Part\n\n{}\n",
        words(650)
    )
}

fn fixture(root: &Path) {
    let p = root.join("en/phase-1");
    for t in ["fundamental", "technical"] {
        std::fs::create_dir_all(p.join(t)).unwrap();
        for i in 1..=6 {
            let tag = &t[..1];
            std::fs::write(p.join(t).join(format!("0{i}-c.md")), chapter(&format!("p1-{tag}-c{i}"), i, (i as usize) % 4)).unwrap();
        }
    }
    std::fs::write(
        p.join("phase.yaml"),
        "slug: \"phase-1\"\norder: 1\ntitle: \"Markets\"\nlevel: \"Beginner\"\nsummary: \"S.\"\nsections:\n  - slug: \"p1-fundamental\"\n    track: \"fundamental\"\n    title: \"F\"\n    summary: \"F.\"\n  - slug: \"p1-technical\"\n    track: \"technical\"\n    title: \"T\"\n    summary: \"T.\"\n",
    )
    .unwrap();
    let qs: Vec<String> = (0..10).map(|i| format!("  - question: \"E{i}?\"\n    options: [\"a\", \"b\", \"c\", \"d\"]\n    answer: {}\n    explanation: \"x\"\n", i % 4)).collect();
    std::fs::write(p.join("exam.yaml"), format!("pass_mark: 70\nquestions:\n{}", qs.join(""))).unwrap();
    // product phase: a single options section (no fundamental / technical)
    let o = root.join("en/phase-9");
    std::fs::create_dir_all(o.join("options")).unwrap();
    for i in 1..=6 {
        std::fs::write(o.join("options").join(format!("0{i}-c.md")), chapter(&format!("p9-o-c{i}"), i, (i as usize) % 4)).unwrap();
    }
    std::fs::write(
        o.join("phase.yaml"),
        "slug: \"phase-9\"\norder: 9\ntitle: \"Options\"\nlevel: \"Intermediate\"\nsummary: \"S.\"\nsections:\n  - slug: \"p9-options\"\n    track: \"options\"\n    title: \"O\"\n    summary: \"O.\"\n",
    )
    .unwrap();
    std::fs::write(o.join("exam.yaml"), format!("pass_mark: 70\nquestions:\n{}", qs.join(""))).unwrap();
    let terms: Vec<String> = (0..150).map(|i| format!("  - slug: \"term-{i}\"\n    term: \"Term {i}\"\n    category: \"Markets\"\n    definition: \"Definition {i}.\"\n    related: [\"term-{}\"]\n", (i + 1) % 150)).collect();
    std::fs::write(root.join("en/glossary.yaml"), format!("terms:\n{}", terms.join(""))).unwrap();
}

async fn test_db() -> Option<String> {
    let server = std::env::var("ACADEMY_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let opts = PgConnectOptions::from_str(&server).ok()?;
    let db = format!("kalks_academy_test_{}", std::process::id());
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

struct Client {
    app: axum::Router,
}

impl Client {
    async fn call(&self, method: &str, path: &str, headers: &[(&str, &str)], body: Option<Value>) -> (StatusCode, Value, String) {
        let mut rb = Request::builder().method(method).uri(path).header("x-kalks-internal", "test-token");
        for (k, v) in headers {
            rb = rb.header(*k, *v);
        }
        let req = match body {
            Some(b) => rb.header("content-type", "application/json").body(Body::from(b.to_string())).unwrap(),
            None => rb.body(Body::empty()).unwrap(),
        };
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let ctype = res.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let v = serde_json::from_str(&text).unwrap_or(Value::Null);
        (status, v, if ctype.contains("svg") { text } else { ctype })
    }
}

const U: &[(&str, &str)] = &[("x-kalks-tenant", "kalks"), ("x-kalks-user-id", "42")];

/// Module switches (gateway): while a broker has `academy` off its client routes answer 403 module_disabled before any
/// handler runs (no database is reached); public certificate checks and the CMS keep their routes.
#[tokio::test]
async fn module_switch_refuses_the_client_routes() {
    let pool = sqlx::postgres::PgPoolOptions::new().acquire_timeout(std::time::Duration::from_millis(300)).connect_lazy("postgres://postgres@127.0.0.1:1/unused").unwrap();
    let cfg = Config {
        bind: String::new(),
        database_url: String::new(),
        internal_token: "test-token".into(),
        content_dir: String::new(),
        verify_base_url: String::new(),
        dev_mode: true,
        json_logs: false,
        gateway_database_url: String::new(),
    };
    let c = Client { app: router(AppState { pool, cfg: Arc::new(cfg), gateway: None }) };
    academy::modules::prime("qa-academy-off", "academy", false);
    let off: &[(&str, &str)] = &[("x-kalks-tenant", "qa-academy-off"), ("x-kalks-user-id", "42")];
    for (m, path) in [("GET", "/v1/catalog"), ("GET", "/v1/chapters/p1-f-c1"), ("POST", "/v1/exams/phase-1"), ("GET", "/v1/me/certificates"), ("GET", "/v1/glossary")] {
        let (s, v, _) = c.call(m, path, off, if m == "POST" { Some(json!({})) } else { None }).await;
        assert_eq!((s, v["error"]["code"].as_str()), (StatusCode::FORBIDDEN, Some("module_disabled")), "{path}");
    }
    // no gateway connection = on: the request reaches the handler (and fails there on the missing database)
    let (s, _, _) = c.call("GET", "/v1/catalog", U, None).await;
    assert_ne!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = c.call("GET", "/v1/public/certificates/KA-NOPE", off, None).await;
    assert_ne!(s, StatusCode::FORBIDDEN);
}
const S: &[(&str, &str)] = &[("x-kalks-tenant", "kalks"), ("x-kalks-staff", "editor@kalks.test")];

#[tokio::test]
async fn real_content_passes_lint() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/academy");
    let langs = content::languages(&root);
    assert!(langs.contains(&"en".to_string()), "content/academy/en is missing");
    for lang in langs {
        let b = content::load_lang(&root, &lang);
        let (errors, _) = content::lint(&b);
        assert!(errors.is_empty(), "content lint errors in {lang}:\n{}", errors.join("\n"));
        let c = content::counts(&b);
        assert!(c.phases >= 9 && c.sections >= 17 && c.chapters >= 103 && c.glossary_terms >= 150, "{c:?}");
        // core phases keep exactly one fundamental and one technical section
        for p in b.phases.iter().filter(|p| p.order <= 8) {
            let tracks: Vec<&str> = p.sections.iter().map(|s| s.def.track.as_str()).collect();
            assert_eq!(tracks, ["fundamental", "technical"], "{}", p.dir);
        }
        // phase 9: the Kalks FX Options elective, one options section, every chapter examined
        let p9 = b.phases.iter().find(|p| p.slug == "phase-9").expect("phase-9 is missing");
        assert_eq!((p9.order, p9.sections.len()), (9, 1));
        assert_eq!(p9.sections[0].def.track, "options");
        assert!(content::is_product_phase(p9.sections.iter().map(|s| s.def.track.as_str())));
        let chapters: HashSet<&str> = p9.sections[0].chapters.iter().map(|c| c.slug.as_str()).collect();
        assert!(chapters.len() >= 7, "{chapters:?}");
        assert!(p9.sections[0].chapters.iter().all(|c| c.slug.starts_with("p9-o-") && !c.quiz.is_empty()));
        let exam = p9.exam.as_ref().expect("phase-9 exam");
        assert!(exam.questions.len() >= 15 && exam.pass_mark == 70);
        for q in &exam.questions {
            assert!(q.chapter.as_deref().is_some_and(|c| chapters.contains(c)), "exam question without a phase-9 chapter: {}", q.question);
        }
        for c in &chapters {
            assert!(exam.questions.iter().any(|q| q.chapter.as_deref() == Some(*c)), "{c} has no exam question");
        }
    }
}

#[tokio::test]
async fn academy_end_to_end() {
    let Some(url) = test_db().await else { return };
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let pool = store::connect(&url).await.unwrap();
    let seeded = store::seed_all(&pool, dir.path()).await.unwrap();
    assert_eq!(seeded[0].1.inserted, (1 + 2 + 12 + 1) + (1 + 1 + 6 + 1) + 150);
    // idempotent: second run changes nothing
    let again = store::seed_all(&pool, dir.path()).await.unwrap();
    assert_eq!((again[0].1.inserted, again[0].1.updated), (0, 0));

    let cfg = Config {
        bind: String::new(),
        database_url: url.clone(),
        internal_token: "test-token".into(),
        content_dir: dir.path().display().to_string(),
        verify_base_url: "https://my.example.com".into(),
        dev_mode: true,
        json_logs: false,
        gateway_database_url: String::new(),
    };
    let c = Client { app: router(AppState { pool: pool.clone(), cfg: Arc::new(cfg), gateway: None }) };

    // internal token enforced
    let res = axum::Router::clone(&c.app).oneshot(Request::get("/v1/catalog").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // catalogue
    let (s, cat, _) = c.call("GET", "/v1/catalog", U, None).await;
    assert_eq!(s, StatusCode::OK, "{cat}");
    assert_eq!(cat["phases"][0]["sections"].as_array().unwrap().len(), 2);
    assert_eq!(cat["phases"][0]["elective"], false);
    assert_eq!(cat["phases"][1]["slug"], "phase-9");
    assert_eq!(cat["phases"][1]["elective"], true);
    assert_eq!(cat["phases"][1]["sections"].as_array().unwrap().len(), 1);
    assert_eq!(cat["phases"][1]["sections"][0]["track"], "options");
    assert_eq!(cat["me"]["chapters_total"], 18);
    assert_eq!(cat["phases"][0]["exam"]["unlocked"], false);
    assert_eq!(cat["me"]["continue"]["slug"], "p1-f-c1");

    // chapter view: no answers leak
    let (s, ch, _) = c.call("GET", "/v1/chapters/p1-f-c1", U, None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(ch["chapter"]["quiz"][0].get("answer").is_none());
    assert!(ch["chapter"]["quiz"][0].get("explanation").is_none());
    assert_eq!(ch["next"]["slug"], "p1-f-c2");
    assert!(ch["prev"].is_null());
    assert_eq!(ch["chapter"]["minutes"], 4);

    // reading progress keeps the max
    let (_, p, _) = c.call("POST", "/v1/chapters/p1-f-c1/progress", U, Some(json!({"read_pct": 80}))).await;
    assert_eq!(p["progress"]["read_pct"], 80);
    let (_, p, _) = c.call("POST", "/v1/chapters/p1-f-c1/progress", U, Some(json!({"read_pct": 30}))).await;
    assert_eq!(p["progress"]["read_pct"], 80);

    // instant feedback on a partial quiz does not record an attempt
    let (_, r, _) = c.call("POST", "/v1/chapters/p1-f-c1/quiz", U, Some(json!({"answers": [1, null, null]}))).await;
    assert_eq!(r["results"][0]["correct"], true);
    assert_eq!(r["all_answered"], false);
    assert_eq!(r["completed"], false);
    // 1 of 3 = fail
    let (_, r, _) = c.call("POST", "/v1/chapters/p1-f-c1/quiz", U, Some(json!({"answers": [0, 1, 1]}))).await;
    assert_eq!((r["passed"].clone(), r["completed"].clone()), (json!(false), json!(false)));
    // exam still locked
    let (s, e, _) = c.call("POST", "/v1/exams/phase-1", U, Some(json!({"answers": [0,1,2,3,0,1,2,3,0,1]}))).await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");

    // the options elective does not wait for phase 1: finish its chapters, its exam unlocks, pass it
    let (_, ch, _) = c.call("GET", "/v1/chapters/p9-o-c1", U, None).await;
    assert_eq!((ch["section"]["track"].clone(), ch["phase"]["elective"].clone()), (json!("options"), json!(true)));
    for i in 1..=6 {
        let a = (i % 4) as i64;
        let (s, r, _) = c.call("POST", &format!("/v1/chapters/p9-o-c{i}/quiz"), U, Some(json!({"answers": [a, 0, 2]}))).await;
        assert_eq!((s, r["completed"].clone()), (StatusCode::OK, json!(true)), "{r}");
    }
    let (_, cat, _) = c.call("GET", "/v1/catalog", U, None).await;
    assert_eq!((cat["phases"][1]["exam"]["unlocked"].clone(), cat["phases"][0]["exam"]["unlocked"].clone()), (json!(true), json!(false)));
    let (_, r, _) = c.call("POST", "/v1/exams/phase-9", U, Some(json!({"answers": [0,1,2,3,0,1,2,3,0,1]}))).await;
    assert_eq!((r["passed"].clone(), r["certificate_issued"].clone()), (json!(true), json!(true)), "{r}");
    let (_, cat, _) = c.call("GET", "/v1/catalog", U, None).await;
    assert!(cat["phases"][1]["certificate"]["code"].is_string() && cat["phases"][0]["certificate"].is_null());

    // pass every chapter quiz
    for t in ["f", "t"] {
        for i in 1..=6 {
            let a = (i % 4) as i64;
            let (s, r, _) = c.call("POST", &format!("/v1/chapters/p1-{t}-c{i}/quiz"), U, Some(json!({"answers": [a, 0, 2]}))).await;
            assert_eq!(s, StatusCode::OK, "{r}");
            assert_eq!(r["passed"], true, "{r}");
            assert_eq!(r["completed"], true);
        }
    }
    let (_, cat, _) = c.call("GET", "/v1/catalog", U, None).await;
    assert_eq!(cat["phases"][0]["progress"]["done"], 12);
    assert_eq!(cat["phases"][0]["exam"]["unlocked"], true);
    assert_eq!(cat["me"]["streak"], 1);

    // exam view hides answers; a failing attempt; then a pass issues the certificate once
    let (_, ev, _) = c.call("GET", "/v1/exams/phase-1", U, None).await;
    assert_eq!(ev["unlocked"], true);
    assert!(ev["exam"]["questions"][0].get("answer").is_none());
    let (_, r, _) = c.call("POST", "/v1/exams/phase-1", U, Some(json!({"answers": [3,3,3,3,3,3,3,3,3,3]}))).await;
    assert_eq!(r["passed"], false);
    assert!(r["certificate"].is_null());
    let name = [("x-kalks-tenant", "kalks"), ("x-kalks-user-id", "42"), ("x-kalks-user-name", "Ana%20L%C3%B3pez"), ("x-kalks-tenant-name", "Kalks%20Markets")];
    let (_, r, _) = c.call("POST", "/v1/exams/phase-1", &name, Some(json!({"answers": [0,1,2,3,0,1,2,3,0,0]}))).await;
    assert_eq!((r["pct"].clone(), r["passed"].clone(), r["certificate_issued"].clone()), (json!(90), json!(true), json!(true)), "{r}");
    let code = r["certificate"]["code"].as_str().unwrap().to_string();
    let (_, r, _) = c.call("POST", "/v1/exams/phase-1", &name, Some(json!({"answers": [0,1,2,3,0,1,2,3,0,1]}))).await;
    assert_eq!(r["certificate_issued"], false);
    assert_eq!(r["certificate"]["code"], code.as_str());

    let (_, mine, _) = c.call("GET", "/v1/me/certificates", U, None).await;
    assert_eq!(mine["certificates"][0]["verify_url"], format!("https://my.example.com/certificate/{code}"));
    assert_eq!(mine["certificates"][1]["phase_order"], 9);
    let (s, v, _) = c.call("GET", &format!("/v1/public/certificates/{code}"), &[], None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!((v["learner_name"].clone(), v["valid"].clone()), (json!("Ana López"), json!(true)));
    let (s, _, svg) = c.call("GET", &format!("/v1/public/certificates/{code}/svg"), &[], None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(svg.starts_with("<svg") && svg.contains("Ana López") && svg.contains(&code), "{svg}");
    let (s, _, _) = c.call("GET", "/v1/public/certificates/KA-ZZZZZ-ZZZZZ", &[], None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // glossary search
    let (_, g, _) = c.call("GET", "/v1/glossary?q=term%2014", U, None).await;
    assert!(g["terms"].as_array().unwrap().iter().any(|t| t["slug"] == "term-14"));
    assert_eq!(g["total"], 150);

    // ---- Back Office CMS ----
    let (_, tree, _) = c.call("GET", "/v1/admin/tree", S, None).await;
    assert_eq!(tree["phases"][0]["sections"][0]["chapters"][0]["source"], "default");
    assert_eq!(tree["phases"][0]["sections"][0]["chapters"][0]["completed"], 1);
    assert_eq!((tree["phases"][0]["elective"].clone(), tree["phases"][1]["elective"].clone()), (json!(false), json!(true)));

    // a section's track can't be changed (it decides the phase's shape); other edits still work
    let (s, r, _) = c.call("PUT", "/v1/admin/nodes/section/p9-options", S, Some(json!({"data": {"track": "technical"}}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    let (s, r, _) = c.call("PUT", "/v1/admin/nodes/section/p9-options", S, Some(json!({"data": {"title": "Options on Kalks", "track": "options"}}))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    // chapters created in the options section get the phase prefix, like the core tracks
    let (s, r, _) = c.call("POST", "/v1/admin/chapters", S, Some(json!({"lang": "en", "section": "p9-options", "title": "Choosing a strike"}))).await;
    assert_eq!((s, r["slug"].clone()), (StatusCode::OK, json!("p9-choosing-a-strike")), "{r}");

    // edit = tenant override; another tenant still sees the default
    let (s, r, _) = c.call("PUT", "/v1/admin/nodes/chapter/p1-f-c2", S, Some(json!({"lang": "en", "data": {"title": "Edited title"}}))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["node"]["source"], "override");
    let (_, ch, _) = c.call("GET", "/v1/chapters/p1-f-c2", U, None).await;
    assert_eq!(ch["chapter"]["title"], "Edited title");
    let (_, ch, _) = c.call("GET", "/v1/chapters/p1-f-c2", &[("x-kalks-tenant", "other"), ("x-kalks-user-id", "42")], None).await;
    assert_eq!(ch["chapter"]["title"], "Chapter p1-f-c2");

    // invalid quiz rejected
    let (s, r, _) = c.call("PUT", "/v1/admin/nodes/chapter/p1-f-c2", S, Some(json!({"data": {"quiz": [{"question": "q", "options": ["a", "b"], "answer": 3, "explanation": ""}]}}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{r}");
    assert!(r["error"]["issues"].as_array().unwrap().len() >= 2);

    // unpublish hides it from learners
    let (s, _, _) = c.call("PUT", "/v1/admin/nodes/chapter/p1-f-c3", S, Some(json!({"published": false}))).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = c.call("GET", "/v1/chapters/p1-f-c3", U, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, ch, _) = c.call("GET", "/v1/chapters/p1-f-c2", U, None).await;
    assert_eq!(ch["next"]["slug"], "p1-f-c4");

    // reset to default
    let (s, _, _) = c.call("DELETE", "/v1/admin/nodes/chapter/p1-f-c3?lang=en", S, None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = c.call("GET", "/v1/chapters/p1-f-c3", U, None).await;
    assert_eq!(s, StatusCode::OK);

    // create a draft chapter (hidden until published), reorder it to the top, then publish
    let (s, r, _) = c.call("POST", "/v1/admin/chapters", S, Some(json!({"lang": "en", "section": "p1-technical", "title": "Our house rules"}))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let slug = r["slug"].as_str().unwrap().to_string();
    assert_eq!(slug, "p1-our-house-rules");
    let (s, _, _) = c.call("GET", &format!("/v1/chapters/{slug}"), U, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, r, _) = c.call("PUT", &format!("/v1/admin/nodes/chapter/{slug}"), S, Some(json!({"published": true}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "publishing an empty chapter must fail: {r}");
    let mut order = vec![slug.clone()];
    order.extend((1..=6).map(|i| format!("p1-t-c{i}")));
    let (s, _, _) = c.call("POST", "/v1/admin/reorder", S, Some(json!({"kind": "chapter", "parent": "p1-technical", "slugs": order}))).await;
    assert_eq!(s, StatusCode::OK);
    let body = format!("## Rules\n\n{}", words(620));
    let quiz = json!([{"question": "Q?", "options": ["a", "b", "c"], "answer": 1, "explanation": "E"}]);
    let (s, r, _) = c.call("PUT", &format!("/v1/admin/nodes/chapter/{slug}"), S, Some(json!({"published": true, "data": {"body": body, "quiz": quiz}}))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["node"]["source"], "custom");
    let (_, ch, _) = c.call("GET", &format!("/v1/chapters/{slug}"), U, None).await;
    assert_eq!(ch["section"]["index"], 1);
    assert_eq!(ch["prev"]["slug"], "p1-f-c6");
    // a new published chapter re-locks nothing already earned but counts toward the phase
    let (_, cat, _) = c.call("GET", "/v1/catalog", U, None).await;
    assert_eq!(cat["phases"][0]["progress"]["total"], 13);

    let (_, st, _) = c.call("GET", "/v1/admin/stats", S, None).await;
    assert_eq!((st["learners"].clone(), st["certificates"].clone(), st["exam_attempts"].clone()), (json!(1), json!(2), json!(4)), "{st}");
    let (_, au, _) = c.call("GET", "/v1/admin/audit", S, None).await;
    assert!(au["audit"].as_array().unwrap().len() >= 5);

    // re-seed after a file change updates the default but never the tenant override
    std::fs::write(dir.path().join("en/phase-1/fundamental/02-c.md"), chapter("p1-f-c2", 2, 2).replace("Chapter p1-f-c2", "Chapter two v2")).unwrap();
    let again = store::seed_all(&pool, dir.path()).await.unwrap();
    assert_eq!(again[0].1.updated, 1);
    let (_, ch, _) = c.call("GET", "/v1/chapters/p1-f-c2", U, None).await;
    assert_eq!(ch["chapter"]["title"], "Edited title");
    let (_, ch, _) = c.call("GET", "/v1/chapters/p1-f-c2", &[("x-kalks-tenant", "other"), ("x-kalks-user-id", "42")], None).await;
    assert_eq!(ch["chapter"]["title"], "Chapter two v2");

    pool.close().await;
    drop_db(&url).await;
}
