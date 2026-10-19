//! Sections and targeting rules. The precedence and the catalogue check run everywhere; the flows run the real
//! handlers against a throwaway PostgreSQL database (see `testdb`) and are skipped, with a note, when none is reachable.

#![allow(unused_must_use)]

use super::*;
use crate::testdb::TestDb;
use std::collections::BTreeSet;

fn rule(scope: &str, value: &str, enabled: bool) -> Rule {
    Rule { id: 0, key: "page.news".into(), scope: scope.into(), value: value.into(), enabled, updated_at: Utc::now(), updated_by: None, label: None }
}

fn who(user: i64, country: &str, groups: &[&str]) -> Who {
    Who { user_id: Some(user), country: Some(country.into()), groups: groups.iter().map(|g| g.to_string()).collect() }
}

#[test]
fn precedence_user_group_country_all_default() {
    let all_off = rule("all", "", false);
    let in_on = rule("country", "in", true);
    let pro_off = rule("group", "pro", false);
    let me_on = rule("user", "7", true);
    let rules = [&all_off, &in_on, &pro_off, &me_on];
    // the client's own rule wins over everything
    assert_eq!(decide(true, true, &rules, &who(7, "in", &["pro"])), (true, "user"));
    // then any of their account groups
    assert_eq!(decide(true, true, &rules, &who(8, "in", &["pro"])), (false, "group"));
    // then their country
    assert_eq!(decide(true, true, &rules, &who(8, "in", &["std"])), (true, "country"));
    // then the rule for everyone
    assert_eq!(decide(true, true, &rules, &who(8, "ae", &["std"])), (false, "all"));
    // then the catalogue default
    assert_eq!(decide(true, true, &[], &who(8, "ae", &[])), (true, "default"));
    assert_eq!(decide(false, true, &[&in_on], &who(8, "ae", &[])), (false, "default"));
    // two groups that disagree: on wins
    let ecn_on = rule("group", "ecn", true);
    assert_eq!(decide(true, true, &[&pro_off, &ecn_on], &who(8, "ae", &["ecn", "pro"])), (true, "group"));
    // a switched-off module beats even the client's own rule
    assert_eq!(decide(true, false, &rules, &who(7, "in", &["pro"])), (false, "module"));
    // tenant-wide (nobody): only the rule for everyone counts
    assert_eq!(decide(true, true, &rules, &Who::default()), (false, "all"));
}

#[test]
fn groups_are_cleaned() {
    assert_eq!(clean_groups(Some(" Pro , ecn,pro,,bad group,x_y,") ), vec!["ecn".to_string(), "pro".to_string()]);
    assert!(clean_groups(None).is_empty());
    assert_eq!(clean_groups(Some(&"g,".repeat(80))).len(), 1);
    assert!(clean_groups(Some(&"a".repeat(41))).is_empty());
}

/// `block("x", "Name", "module")` / `page("/a/b", "group", "Name", "module")` lines of the apps' catalogue.
fn ts_catalogue() -> Vec<(String, String, Option<String>)> {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/ui/src/lib/sections.ts")).expect("packages/ui/src/lib/sections.ts");
    let start = src.find("export const SECTIONS").expect("SECTIONS in sections.ts");
    let mut out = Vec::new();
    for line in src[start..].lines() {
        let l = line.trim();
        if l.starts_with("];") {
            break;
        }
        let (is_page, rest) = match (l.strip_prefix("page(\""), l.strip_prefix("block(\"")) {
            (Some(r), _) => (true, r),
            (_, Some(r)) => (false, r),
            _ => continue,
        };
        let args: Vec<String> = rest.trim_end_matches("),").trim_end_matches('"').split("\", \"").map(str::to_string).collect();
        let (key, name, module) = if is_page {
            (format!("page.{}", args[0].trim_start_matches('/').replace('/', ".")), args[2].clone(), args.get(3).cloned())
        } else {
            (format!("dashboard.{}", args[0]), args[1].clone(), args.get(2).cloned())
        };
        out.push((key, name, module));
    }
    out
}

#[test]
fn catalogue_matches_the_apps() {
    let ts = ts_catalogue();
    assert!(ts.len() > 50, "parsed {} sections from sections.ts", ts.len());
    let ours: Vec<(String, String, Option<String>)> = BUILTIN_SECTIONS.iter().map(|d| (d.key.to_string(), d.name.to_string(), d.module.map(str::to_string))).collect();
    assert_eq!(ours, ts, "BUILTIN_SECTIONS and packages/ui/src/lib/sections.ts SECTIONS must list the same keys, names and modules in the same order");
    let keys: BTreeSet<&str> = BUILTIN_SECTIONS.iter().map(|d| d.key).collect();
    assert_eq!(keys.len(), BUILTIN_SECTIONS.len(), "unique keys");
    for d in BUILTIN_SECTIONS {
        assert!(d.key.len() <= 64 && (d.key.starts_with("page.") || d.key.starts_with("dashboard.")), "{}", d.key);
        if let Some(m) = d.module {
            assert!(tenancy::BUILTIN_FEATURES.iter().any(|f| f.key == m && f.kind == "module"), "{} -> unknown module {m}", d.key);
        }
    }
}

// ---------- flows ----------

fn ctx(tok: Option<&str>) -> Ctx {
    Ctx { ip: "203.0.113.9".into(), user_agent: "test-agent".into(), device: Some("device-0123456789abcdef".into()), tenant_slug: "kalks".into(), bearer: tok.map(str::to_string) }
}

fn code(e: &ApiError) -> String {
    match e {
        ApiError::Coded { code, .. } => (*code).into(),
        ApiError::Validation { field, .. } => format!("field:{field}"),
        ApiError::Forbidden => "forbidden".into(),
        ApiError::NotFound => "not_found".into(),
        ApiError::Unauthorized => "unauthorized".into(),
        other => format!("{other:?}"),
    }
}

async fn kalks(st: &AppState) -> i64 {
    sqlx::query_scalar("SELECT id FROM tenants WHERE slug = 'kalks'").fetch_one(&st.pool).await.unwrap()
}

async fn staff(st: &AppState, tenant: i64, email: &str, role: &str) -> String {
    let rid: i64 = sqlx::query_scalar("SELECT id FROM roles WHERE tenant_id = $1 AND key = $2").bind(tenant).bind(role).fetch_one(&st.pool).await.unwrap();
    let id: i64 = sqlx::query_scalar("INSERT INTO staff (tenant_id, email, password_hash, name, role, role_id) VALUES ($1,$2,'x',$3,$4,$5) RETURNING id")
        .bind(tenant)
        .bind(email)
        .bind(format!("Staff {role}"))
        .bind(role)
        .bind(rid)
        .fetch_one(&st.pool)
        .await
        .unwrap();
    identity::create_session(st, &ctx(None), Kind::Staff, tenant, id).await.unwrap().token
}

/// A client of `tenant` from `country`, with a live session token.
async fn client(st: &AppState, tenant: i64, email: &str, country: &str) -> (i64, String) {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO users (tenant_id, email, password_hash, first_name, last_name, phone_dial, phone, country, date_of_birth, referral_code, terms_accepted_at, email_verified_at)
         VALUES ($1, $2, 'x', 'Priya', 'Shah', '+971', '501234567', $3, '1990-01-01', $4, now(), now()) RETURNING id",
    )
    .bind(tenant)
    .bind(email)
    .bind(country)
    .bind(format!("T{}", crate::crypto::random_token(6)))
    .fetch_one(&st.pool)
    .await
    .unwrap();
    (id, identity::create_session(st, &ctx(None), Kind::User, tenant, id).await.unwrap().token)
}

async fn mine(st: &AppState, tok: &str, groups: &str) -> Value {
    let Json(v) = me(State(st.clone()), ctx(Some(tok)), Ok(Query(MeQuery { groups: Some(groups.into()) }))).await.unwrap();
    v
}

async fn toggle_as(st: &AppState, tok: &str, key: &str, enabled: Option<bool>, reason: &str) -> ApiResult<Json<Value>> {
    toggle(State(st.clone()), ctx(Some(tok)), Path(key.into()), Ok(Json(ToggleReq { enabled, reason: Some(reason.into()) }))).await
}

async fn rule_as(st: &AppState, tok: &str, key: &str, scope: &str, value: &str, enabled: Option<bool>) -> ApiResult<(StatusCode, Json<Value>)> {
    set_rule(State(st.clone()), ctx(Some(tok)), Path(key.into()), Ok(Json(RuleReq { scope: scope.into(), value: value.into(), enabled, reason: Some("Regional launch plan".into()) }))).await
}

async fn audit_count(st: &AppState, action: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = $1").bind(action).fetch_one(&st.pool).await.unwrap()
}

#[tokio::test]
async fn rules_precedence_permissions_and_audit() {
    let Some(db) = TestDb::new("sections").await else { return };
    let st = db.st.clone();
    let t = kalks(&st).await;
    let admin = staff(&st, t, "admin@example.com", "admin").await;
    let support = staff(&st, t, "support@example.com", "support").await;
    let owner = staff(&st, t, "owner@example.com", "platform_owner").await;
    let (priya, priya_tok) = client(&st, t, "priya@example.com", "in").await;
    let (_omar, omar_tok) = client(&st, t, "omar@example.com", "ae").await;

    // the catalogue is seeded: every section on, tenant-wide and for a client
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM feature_flags WHERE kind = 'section'").fetch_one(&st.pool).await.unwrap();
    assert_eq!(n as usize, BUILTIN_SECTIONS.len());
    let v = mine(&st, &priya_tok, "").await;
    assert_eq!(v["sections"].as_object().unwrap().len(), BUILTIN_SECTIONS.len());
    assert!(v["sections"].as_object().unwrap().values().all(|x| x == &json!(true)));
    assert_eq!((v["user_id"].as_i64(), v["country"].as_str(), v["group_rules"].as_bool()), (Some(priya), Some("in"), Some(false)));
    let v0 = v["version"].as_i64().unwrap();

    // staff without the settings permissions can't read or switch; a reason is required; modules aren't sections
    assert!(matches!(list(State(st.clone()), ctx(Some(&support))).await, Err(ApiError::Forbidden)));
    assert_eq!(code(&toggle_as(&st, &support, "page.news", Some(false), "testing").await.unwrap_err()), "forbidden");
    assert_eq!(code(&toggle_as(&st, &admin, "page.news", Some(false), " ").await.unwrap_err()), "field:reason");
    assert_eq!(code(&toggle_as(&st, &admin, "news", Some(false), "testing").await.unwrap_err()), "not_found");
    assert_eq!(code(&rule_as(&st, &admin, "page.news", "country", "zz1", Some(true)).await.unwrap_err()), "field:value");
    assert_eq!(code(&rule_as(&st, &admin, "page.news", "user", "999999", Some(true)).await.unwrap_err()), "field:value");
    assert_eq!(code(&rule_as(&st, &admin, "page.news", "planet", "x", Some(true)).await.unwrap_err()), "field:scope");

    // all off < country on (India) < group off (pro) < user on (Priya)
    toggle_as(&st, &admin, "page.news", Some(false), "News licence pending").await.unwrap();
    let (s, _) = rule_as(&st, &admin, "page.news", "country", "IN", Some(true)).await.unwrap();
    assert_eq!(s, StatusCode::CREATED);
    rule_as(&st, &admin, "page.news", "group", "Pro", Some(false)).await.unwrap();
    assert_eq!(mine(&st, &omar_tok, "").await["sections"]["page.news"], false, "everyone: off");
    assert_eq!(mine(&st, &priya_tok, "").await["sections"]["page.news"], true, "India: on");
    let v = mine(&st, &priya_tok, "std,pro").await;
    assert_eq!(v["sections"]["page.news"], false, "a pro account: off");
    assert_eq!(v["group_rules"], true);
    rule_as(&st, &admin, "page.news", "user", &priya.to_string(), Some(true)).await.unwrap();
    assert_eq!(mine(&st, &priya_tok, "pro").await["sections"]["page.news"], true, "her own rule: on");
    assert_eq!(mine(&st, &omar_tok, "pro").await["sections"]["page.news"], false);
    // the other sections are untouched
    assert_eq!(mine(&st, &omar_tok, "pro").await["sections"]["page.calendar"], true);
    assert!(mine(&st, &priya_tok, "").await["version"].as_i64().unwrap() > v0, "every change moves the version");

    // the Back Office preview answers the same, and says why
    let p = |country: Option<&str>, groups: Option<&str>, user_id: Option<i64>| {
        let (st, c) = (st.clone(), ctx(Some(&admin)));
        let q = PreviewQuery { country: country.map(str::to_string), groups: groups.map(str::to_string), user_id };
        async move { preview(State(st), c, Ok(Query(q))).await.map(|Json(v)| v) }
    };
    let v = p(Some("in"), None, None).await.unwrap();
    assert_eq!((v["sections"]["page.news"].as_bool(), v["sources"]["page.news"].as_str()), (Some(true), Some("country")));
    let v = p(Some("in"), Some("pro"), None).await.unwrap();
    assert_eq!(v["sources"]["page.news"], "group");
    let v = p(None, Some("pro"), Some(priya)).await.unwrap();
    assert_eq!((v["sections"]["page.news"].as_bool(), v["sources"]["page.news"].as_str(), v["who"]["country"].as_str()), (Some(true), Some("user"), Some("in")));
    assert_eq!(p(None, None, None).await.unwrap()["sources"]["page.news"], "all");

    // the list carries the rules (user rules with the client's email)
    let Json(l) = list(State(st.clone()), ctx(Some(&admin))).await.unwrap();
    let news = l["sections"].as_array().unwrap().iter().find(|s| s["key"] == "page.news").unwrap().clone();
    assert_eq!(news["enabled"], false);
    assert_eq!(news["rules"].as_array().unwrap().len(), 4);
    let user_rule = news["rules"].as_array().unwrap().iter().find(|r| r["scope"] == "user").unwrap().clone();
    assert_eq!(user_rule["label"], "priya@example.com");
    assert_eq!(user_rule["updated_by"], "Staff admin");

    // removing a rule needs a reason too; then the next rule down decides
    let rid = user_rule["id"].as_i64().unwrap();
    assert_eq!(code(&remove_rule(State(st.clone()), ctx(Some(&admin)), Path(rid), Ok(Json(RemoveReq { reason: None }))).await.unwrap_err()), "field:reason");
    remove_rule(State(st.clone()), ctx(Some(&admin)), Path(rid), Ok(Json(RemoveReq { reason: Some("Trial ended".into()) }))).await.unwrap();
    assert_eq!(mine(&st, &priya_tok, "pro").await["sections"]["page.news"], false);
    assert_eq!(code(&remove_rule(State(st.clone()), ctx(Some(&admin)), Path(rid), Ok(Json(RemoveReq { reason: Some("again".into()) }))).await.unwrap_err()), "not_found");

    // a switched-off module beats every rule of its sections, even the client's own
    rule_as(&st, &admin, "page.calendar", "user", &priya.to_string(), Some(true)).await.unwrap();
    let mut m = Map::new();
    m.insert("calendar".into(), json!(false));
    crate::owner::set_tenant_features(State(st.clone()), ctx(Some(&owner)), Path(t), Ok(Json(m))).await.unwrap();
    let v = mine(&st, &priya_tok, "").await;
    assert_eq!((v["modules"]["calendar"].as_bool(), v["sections"]["page.calendar"].as_bool()), (Some(false), Some(false)));
    let Json(cfg) = tenancy::public_config(State(st.clone()), ctx(None)).await.unwrap();
    assert_eq!(cfg["sections"]["page.calendar"], false);
    // the public config (anonymous pages): sections for everyone, never in the flags
    assert_eq!(cfg["sections"]["page.news"], false);
    assert_eq!(cfg["sections"]["page.wallet"], true);
    assert!(cfg["flags"].get("page.news").is_none() && cfg["modules"].get("page.news").is_none());
    assert_eq!(cfg["features_version"], mine(&st, &priya_tok, "").await["version"]);

    // the platform owner's tenant switch accepts a section too (its rule for everyone)
    let mut m = Map::new();
    m.insert("dashboard.banners".into(), json!(false));
    m.insert("reason".into(), json!("No banners for this broker"));
    crate::owner::set_tenant_features(State(st.clone()), ctx(Some(&owner)), Path(t), Ok(Json(m))).await.unwrap();
    assert_eq!(mine(&st, &omar_tok, "").await["sections"]["dashboard.banners"], false);
    // back to the default
    toggle_as(&st, &admin, "dashboard.banners", None, "Banners are back").await.unwrap();
    assert_eq!(mine(&st, &omar_tok, "").await["sections"]["dashboard.banners"], true);

    // every change is audited with its reason
    assert_eq!(audit_count(&st, "settings.section_toggled").await, 3);
    assert_eq!(audit_count(&st, "settings.section_rule_set").await, 4);
    assert_eq!(audit_count(&st, "settings.section_rule_removed").await, 1);
    let r: (Option<String>, Option<String>, Option<String>) = sqlx::query_as("SELECT meta->>'key', meta->>'scope', meta->>'reason' FROM audit_log WHERE action = 'settings.section_toggled' ORDER BY id LIMIT 1").fetch_one(&st.pool).await.unwrap();
    assert_eq!(r, (Some("page.news".into()), Some("all".into()), Some("News licence pending".into())));
    // no client session, no features
    assert_eq!(code(&me(State(st.clone()), ctx(None), Ok(Query(MeQuery { groups: None }))).await.unwrap_err()), "unauthorized");
    db.drop_db().await;
}

#[tokio::test]
async fn rules_are_isolated_per_tenant() {
    let Some(db) = TestDb::new("sections_rls").await else { return };
    let st = db.st.clone();
    let t = kalks(&st).await;
    let other: i64 = sqlx::query_scalar("INSERT INTO tenants (slug, name) VALUES ('acme-fx', 'Acme FX') RETURNING id").fetch_one(&st.pool).await.unwrap();
    for (tenant, enabled) in [(t, true), (other, false)] {
        sqlx::query("INSERT INTO feature_rules (tenant_id, key, scope, value, enabled) VALUES ($1, 'page.news', 'all', '', $2)").bind(tenant).bind(enabled).execute(&st.pool).await.unwrap();
    }
    // inside a tenant scope only that tenant's rules are visible ...
    let mut tx = tenant_tx(&st.pool, other).await.unwrap();
    let seen: Vec<i64> = sqlx::query_scalar("SELECT tenant_id FROM feature_rules").fetch_all(&mut *tx).await.unwrap();
    assert_eq!(seen, vec![other]);
    // ... and nothing can be written for another tenant
    let err = sqlx::query("INSERT INTO feature_rules (tenant_id, key, scope, value, enabled) VALUES ($1, 'page.calendar', 'all', '', false)").bind(t).execute(&mut *tx).await;
    assert!(err.is_err(), "row-level security refuses another tenant's rule");
    drop(tx);
    let mut tx = tenant_tx(&st.pool, other).await.unwrap();
    let n = sqlx::query("UPDATE feature_rules SET enabled = false").execute(&mut *tx).await.unwrap().rows_affected();
    assert_eq!(n, 1, "an update only reaches the tenant's own rows");
    drop(tx);
    // the evaluation reads through that scope
    assert!(rules(&st.pool, t).await.unwrap().iter().all(|r| r.enabled));
    assert_eq!(rules(&st.pool, other).await.unwrap().len(), 1);
    assert_eq!(effective(&st.pool, t, None).await.unwrap().sections["page.news"], true);
    assert_eq!(effective(&st.pool, other, None).await.unwrap().sections["page.news"], false);
    // a client of one broker can't be targeted from another
    let (uid, _) = client(&st, t, "kalks.client@example.com", "in").await;
    assert!(rule_value(&st, other, "user", &uid.to_string()).await.is_err());
    assert_eq!(rule_value(&st, t, "user", &format!("#{uid}")).await.unwrap(), uid.to_string());
    db.drop_db().await;
}
