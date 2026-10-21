//! Erasure tests: the real handlers against a throwaway PostgreSQL database (see `testdb`), skipped with a note
//! when no server is reachable.

use super::*;
use crate::client_security::{CreateRequestReq, ProcessReq, admin_process_request, create_request};
use crate::crypto;
use crate::identity::{self, Kind};
use crate::testdb::TestDb;

fn code(e: &ApiError) -> String {
    match e {
        ApiError::Coded { code, .. } => (*code).into(),
        ApiError::Validation { field, .. } => format!("field:{field}"),
        ApiError::Unauthorized => "unauthorized".into(),
        ApiError::InvalidCredentials => "invalid_credentials".into(),
        ApiError::NotFound => "not_found".into(),
        ApiError::Forbidden => "forbidden".into(),
        other => format!("{other:?}"),
    }
}

fn ctx(bearer: Option<&str>) -> Ctx {
    Ctx { ip: "203.0.113.30".into(), user_agent: "Mozilla/5.0 (Macintosh) Chrome/131".into(), device: Some("device-erase-aaaa".into()), tenant_slug: "kalks".into(), bearer: bearer.map(str::to_string) }
}

async fn user(db: &TestDb, email: &str) -> i64 {
    let hash = crypto::hash_password("Kalks@2026").unwrap();
    sqlx::query_scalar(
        "INSERT INTO users (tenant_id, email, password_hash, first_name, last_name, phone_dial, phone, country, date_of_birth,
                            referral_code, terms_accepted_at, email_verified_at, google_sub, avatar_url, utm_source, utm_campaign,
                            landing_page, first_referrer, referral_campaign)
         VALUES (1, $1, $2, 'Priya', 'Shah', '+971', '501234567', 'ae', '1990-01-01', $3, now(), now(), $4, 'https://lh3.example/p.jpg',
                 'google', 'spring', '/en/promo', 'https://news.example/', 'gold')
         RETURNING id",
    )
    .bind(email)
    .bind(hash)
    .bind(format!("T{}", crypto::random_token(6)))
    .bind(format!("g-{}", crypto::random_token(6)))
    .fetch_one(&db.st.pool)
    .await
    .unwrap()
}

/// A staff member with a built-in role and a live session token.
async fn staff(db: &TestDb, role: &str) -> String {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO staff (tenant_id, email, password_hash, name, role, role_id)
         VALUES (1, $1, 'x', $2, $3, (SELECT id FROM roles WHERE tenant_id = 1 AND key = $3)) RETURNING id",
    )
    .bind(format!("{role}-{}@kalks.test", crypto::random_token(4).to_lowercase()))
    .bind(format!("Staff {role}"))
    .bind(role)
    .fetch_one(&db.st.pool)
    .await
    .unwrap();
    identity::create_session(&db.st, &ctx(None), Kind::Staff, 1, id).await.unwrap().token
}

fn req(reason: &str, confirm: &str) -> Result<Json<EraseReq>, JsonRejection> {
    Ok(Json(EraseReq { reason: reason.into(), confirm: confirm.into() }))
}

async fn login(db: &TestDb, email: &str) -> ApiResult<Json<Value>> {
    let r = Json(crate::client_auth::LoginReq { email: email.into(), password: "Kalks@2026".into() });
    crate::client_auth::login(State(db.st.clone()), ctx(None), Ok(r)).await
}

/// Personal data the client leaves behind: sessions, codes, devices, a view-only login, presence, KYC, suitability.
async fn footprint(db: &TestDb, uid: i64) {
    let p = &db.st.pool;
    sqlx::query("INSERT INTO email_otps (id, tenant_id, subject_kind, subject_id, purpose, code_hash, expires_at) VALUES ($1, 1, 'user', $2, 'login', '\\x00', now() + interval '10 minutes')")
        .bind(crypto::random_token(12))
        .bind(uid)
        .execute(p)
        .await
        .unwrap();
    sqlx::query("INSERT INTO trusted_devices (tenant_id, subject_kind, subject_id, device_hash, user_agent) VALUES (1, 'user', $1, '\\x01', 'Safari')").bind(uid).execute(p).await.unwrap();
    sqlx::query("INSERT INTO stepup_tokens (token_hash, tenant_id, user_id, action, expires_at) VALUES ('\\x02', 1, $1, 'withdrawal', now() + interval '5 minutes')").bind(uid).execute(p).await.unwrap();
    sqlx::query("INSERT INTO client_viewers (tenant_id, user_id, label, username, password_hash) VALUES (1, $1, 'My accountant', 'priya-view-01', 'x')").bind(uid).execute(p).await.unwrap();
    sqlx::query("INSERT INTO client_presence (conn_key, tenant_id, user_id, ip, user_agent, since, last_active) VALUES ('c-erase', 1, $1, '198.51.100.7', 'Kalks Trader', now(), now())")
        .bind(uid)
        .execute(p)
        .await
        .unwrap();
    sqlx::query("INSERT INTO suitability (tenant_id, user_id, product, disclosure_version, accepted_at, ip, user_agent) VALUES (1, $1, 'options', 1, now(), '198.51.100.7', 'Safari')")
        .bind(uid)
        .execute(p)
        .await
        .unwrap();
    sqlx::query("INSERT INTO kyc_cases (tenant_id, user_id, kind, status, details) VALUES (1, $1, 'individual', 'approved', '{\"address\": {\"line1\": \"12 Palm Street\"}}')")
        .bind(uid)
        .execute(p)
        .await
        .unwrap();
    // identity verified: name and date of birth are locked (D92), erasure must still change them
    sqlx::query("UPDATE users SET kyc_status = 'verified', identity_locked_at = now() WHERE id = $1").bind(uid).execute(p).await.unwrap();
}

/// The client asks to close the account and staff complete the request (client_security.rs).
async fn close(db: &TestDb, uid: i64, staff_tok: &str) {
    let s = identity::create_session(&db.st, &ctx(None), Kind::User, 1, uid).await.unwrap();
    let r: CreateRequestReq = serde_json::from_value(json!({ "kind": "closure", "reason": "Moving to another country, Priya Shah" })).unwrap();
    let (_, Json(v)) = create_request(State(db.st.clone()), ctx(Some(&s.token)), Ok(Json(r))).await.unwrap();
    let id = v["request"]["id"].as_i64().unwrap();
    let p: ProcessReq = serde_json::from_value(json!({ "status": "completed", "note": "Closed on request" })).unwrap();
    let Json(done) = admin_process_request(State(db.st.clone()), ctx(Some(staff_tok)), Path(id), Ok(Json(p))).await.unwrap();
    assert_eq!(done["request"]["status"], "completed");
}

#[test]
fn placeholder_email_is_unique_and_undeliverable() {
    assert_eq!(erased_email(42), "erased-42@erased.invalid");
    assert_ne!(erased_email(1), erased_email(11));
}

#[tokio::test]
async fn erase_a_closed_client() {
    let Some(db) = TestDb::new("client-erase").await else { return };
    let st = || State(db.st.clone());
    let owner = staff(&db, "super_admin").await;
    let sa = || ctx(Some(&owner));
    let referrer = user(&db, "referrer@gmail.com").await;
    let uid = user(&db, "priya.erase@gmail.com").await;
    sqlx::query("UPDATE users SET referred_by = $2 WHERE id = $1").bind(uid).bind(referrer).execute(&db.st.pool).await.unwrap();
    let code_before: String = sqlx::query_scalar("SELECT referral_code FROM users WHERE id = $1").bind(uid).fetch_one(&db.st.pool).await.unwrap();
    footprint(&db, uid).await;
    let live = identity::create_session(&db.st, &ctx(None), Kind::User, 1, uid).await.unwrap();

    // permission: admins, Compliance and Support don't hold clients.erase
    for role in ["admin", "compliance", "support"] {
        let tok = staff(&db, role).await;
        assert_eq!(code(&erase(st(), ctx(Some(&tok)), Path(uid), req("GDPR request #12", "ERASE")).await.unwrap_err()), "forbidden", "{role}");
    }
    // an active client is refused; nothing changes
    assert_eq!(code(&erase(st(), sa(), Path(uid), req("GDPR request #12", "ERASE")).await.unwrap_err()), "client_not_closed");
    assert!(login(&db, "priya.erase@gmail.com").await.is_ok());
    assert_eq!(db.count("SELECT count(*) FROM kyc_cases WHERE user_id = $1", uid).await, 1);
    assert!(identity::resolve_session(&db.st, &ctx(Some(&live.token)), Kind::User).await.is_ok());

    close(&db, uid, &owner).await;
    // confirmation and reason are required
    assert_eq!(code(&erase(st(), sa(), Path(uid), req("GDPR request #12", "erase it")).await.unwrap_err()), "field:confirm");
    assert_eq!(code(&erase(st(), sa(), Path(uid), req(" x ", "ERASE")).await.unwrap_err()), "field:reason");
    assert_eq!(code(&erase(st(), sa(), Path(999_999), req("GDPR request #12", "ERASE")).await.unwrap_err()), "not_found");

    let Json(v) = erase(st(), sa(), Path(uid), req("GDPR request #12", "ERASE")).await.unwrap();
    assert_eq!(v["already_erased"], false);
    assert!(v["erased_at"].is_string());

    // personal columns are placeholders; id, referral code and referral link stay
    let r = sqlx::query(
        "SELECT email, password_hash, first_name, last_name, phone_dial, phone, date_of_birth::text AS dob, google_sub, avatar_url, utm_source,
                utm_campaign, landing_page, first_referrer, marketing_consent, referral_code, referred_by, referral_campaign, status, erased_at
         FROM users WHERE id = $1",
    )
    .bind(uid)
    .fetch_one(&db.st.pool)
    .await
    .unwrap();
    assert_eq!(r.get::<String, _>("email"), format!("erased-{uid}@erased.invalid"));
    assert_eq!(r.get::<String, _>("password_hash"), "!erased");
    assert_eq!((r.get::<String, _>("first_name"), r.get::<String, _>("last_name")), ("Erased".into(), "client".into()));
    assert_eq!((r.get::<String, _>("phone_dial"), r.get::<String, _>("phone")), (String::new(), String::new()));
    assert_eq!(r.get::<String, _>("dob"), "1900-01-01");
    for col in ["google_sub", "avatar_url", "utm_source", "utm_campaign", "landing_page", "first_referrer"] {
        assert!(r.get::<Option<String>, _>(col).is_none(), "{col}");
    }
    assert!(!r.get::<bool, _>("marketing_consent"));
    assert_eq!(r.get::<String, _>("referral_code"), code_before);
    assert_eq!(r.get::<Option<i64>, _>("referred_by"), Some(referrer));
    assert_eq!(r.get::<Option<String>, _>("referral_campaign").as_deref(), Some("gold"));
    assert_eq!(r.get::<String, _>("status"), "closed");
    assert!(r.get::<Option<DateTime<Utc>>, _>("erased_at").is_some());

    // sign-in data, view-only logins, presence and KYC are gone
    for sql in [
        "SELECT count(*) FROM sessions WHERE subject_kind = 'user' AND subject_id = $1",
        "SELECT count(*) FROM email_otps WHERE subject_kind = 'user' AND subject_id = $1",
        "SELECT count(*) FROM trusted_devices WHERE subject_kind = 'user' AND subject_id = $1",
        "SELECT count(*) FROM stepup_tokens WHERE user_id = $1",
        "SELECT count(*) FROM client_viewers WHERE user_id = $1",
        "SELECT count(*) FROM client_presence WHERE user_id = $1",
        "SELECT count(*) FROM kyc_cases WHERE user_id = $1",
        "SELECT count(*) FROM suitability WHERE user_id = $1 AND (ip IS NOT NULL OR user_agent IS NOT NULL)",
        "SELECT count(*) FROM client_requests WHERE user_id = $1 AND reason IS NOT NULL",
    ] {
        assert_eq!(db.count(sql, uid).await, 0, "{sql}");
    }
    // the options acceptance record itself stays
    assert_eq!(db.count("SELECT count(*) FROM suitability WHERE user_id = $1 AND accepted_at IS NOT NULL", uid).await, 1);

    // nobody can sign in: not with the old email, not with the placeholder, not with an old session
    assert_eq!(code(&login(&db, "priya.erase@gmail.com").await.unwrap_err()), "invalid_credentials");
    assert!(login(&db, &erased_email(uid)).await.is_err());
    assert!(identity::resolve_session(&db.st, &ctx(Some(&live.token)), Kind::User).await.is_err());

    // the Back Office shows the client as erased
    let Json(d) = crate::admin::user_detail(st(), sa(), Path(uid)).await.unwrap();
    assert!(d["user"]["erased_at"].is_string());
    assert_eq!(d["user"]["name"], "Erased client");

    // idempotent: a second call changes nothing and is not audited again
    let Json(again) = erase(st(), sa(), Path(uid), req("GDPR request #12", "ERASE")).await.unwrap();
    assert_eq!(again["already_erased"], true);
    assert_eq!(erase_personal_data(&db.st.pool, uid).await.unwrap().map(|_| ()), None);
    assert_eq!(db.count("SELECT count(*) FROM audit_log WHERE action = 'admin.client_erased' AND target_id = $1", uid).await, 1);
    let meta: Value = sqlx::query_scalar::<_, sqlx::types::Json<Value>>("SELECT meta FROM audit_log WHERE action = 'admin.client_erased' AND target_id = $1")
        .bind(uid)
        .fetch_one(&db.st.pool)
        .await
        .unwrap()
        .0;
    assert_eq!(meta["reason"], "GDPR request #12");
    assert!(!meta.to_string().contains("priya") && !meta.to_string().contains("Shah"));

    // the referrer is untouched
    assert_eq!(db.count("SELECT count(*) FROM users WHERE id = $1 AND erased_at IS NULL AND email = 'referrer@gmail.com'", referrer).await, 1);
    db.drop_db().await;
}
