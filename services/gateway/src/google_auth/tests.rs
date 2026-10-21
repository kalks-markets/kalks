//! Google sign-in tests. Ticket tests are pure; the flow tests run the real handlers against a throwaway
//! PostgreSQL database (GATEWAY_TEST_DATABASE_URL, default the local dev server on :5433) and are skipped,
//! with a note, when no server is reachable. Google itself is out of scope here: the BFF verifies the ID token
//! and these handlers receive the already-verified identity, exactly as in production.

use super::*;
use crate::testdb::TestDb;
use axum::Json;
use axum::extract::State;

fn keys() -> Keys {
    Keys::new(&"k".repeat(40))
}

fn sample(exp: i64) -> Ticket {
    Ticket {
        v: 1,
        t: 1,
        sub: "109876543210987654321".into(),
        email: "arjun@gmail.com".into(),
        first_name: "Arjun".into(),
        last_name: "Mehta".into(),
        picture: None,
        referral: Some("PRIYA1234".into()),
        exp,
    }
}

#[test]
fn ticket_roundtrip() {
    let now = Utc::now().timestamp();
    let t = sample(now + 60);
    let token = seal(&keys(), &t);
    assert_eq!(open(&keys(), &token, 1, now), Some(t));
}

#[test]
fn ticket_rejects_tamper_expiry_tenant_and_key() {
    let now = Utc::now().timestamp();
    let token = seal(&keys(), &sample(now + 60));
    // expired
    assert!(open(&keys(), &token, 1, now + 61).is_none());
    // other tenant
    assert!(open(&keys(), &token, 2, now).is_none());
    // other server secret
    assert!(open(&Keys::new(&"x".repeat(40)), &token, 1, now).is_none());
    // payload swapped for a forged one, old signature kept
    let (_, mac) = token.split_once('.').unwrap();
    let mut forged = sample(now + 60);
    forged.email = "victim@gmail.com".into();
    let forged_payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&forged).unwrap());
    assert!(open(&keys(), &format!("{forged_payload}.{mac}"), 1, now).is_none());
    // garbage
    for bad in ["", ".", "abc", "abc.def", &"a".repeat(5000)] {
        assert!(open(&keys(), bad, 1, now).is_none());
    }
}

#[test]
fn input_cleanup() {
    assert_eq!(clean_sub(" 1234567890 ").as_deref(), Some("1234567890"));
    assert!(clean_sub("").is_none());
    assert!(clean_sub("has space").is_none());
    assert!(clean_sub(&"9".repeat(256)).is_none());
    assert_eq!(clean_name(Some("  Arjun  ")), "Arjun");
    assert_eq!(clean_name(Some("DJ 3000")), "");
    assert_eq!(clean_name(None), "");
    assert_eq!(clean_picture(Some("https://lh3.googleusercontent.com/a/x")).as_deref(), Some("https://lh3.googleusercontent.com/a/x"));
    assert!(clean_picture(Some("http://example.com/a.png")).is_none());
    assert!(clean_picture(Some("javascript:alert(1)")).is_none());
}

// ---------- flow tests against PostgreSQL ----------

fn ctx(device: &str) -> Ctx {
    Ctx { ip: "203.0.113.7".into(), user_agent: "test".into(), device: Some(device.into()), tenant_slug: "kalks".into(), bearer: None }
}

fn greq(sub: &str, email: &str, verified: bool) -> GoogleReq {
    GoogleReq {
        sub: sub.into(),
        email: email.into(),
        email_verified: verified,
        given_name: Some("Arjun".into()),
        family_name: Some("Mehta".into()),
        picture: Some("https://lh3.googleusercontent.com/a/abc".into()),
        referral: Some("nosuchcode1".into()),
    }
}

fn creq(ticket: &str, dob: &str) -> CompleteReq {
    CompleteReq {
        ticket: ticket.into(),
        first_name: None,
        last_name: None,
        phone_dial: "+91".into(),
        phone: "98201 44721".into(),
        country: "IN".into(),
        date_of_birth: dob.into(),
        referral_code: None,
        referral_campaign: None,
        attribution: Some(crate::marketing::AttributionReq { utm_source: Some("google".into()), utm_campaign: Some("g-signup".into()), ..Default::default() }),
        marketing_consent: Some(false),
        accept_terms: true,
    }
}

fn code(e: &ApiError) -> &'static str {
    match e {
        ApiError::Coded { code, .. } => code,
        ApiError::EmailTaken => "email_taken",
        ApiError::AccountDisabled => "account_disabled",
        ApiError::Validation { field, .. } => field,
        ApiError::RateLimited { .. } => "rate_limited",
        ApiError::BadRequest(_) => "bad_request",
        _ => "other",
    }
}

async fn password_user(db: &TestDb, email: &str, verified: bool, password: &str) -> i64 {
    let hash = crypto::hash_password(password).unwrap();
    sqlx::query_scalar(
        "INSERT INTO users (tenant_id, email, password_hash, first_name, last_name, phone_dial, phone, country, date_of_birth,
                            referral_code, terms_accepted_at, email_verified_at)
         VALUES (1, $1, $2, 'Priya', 'Shah', '+971', '501234567', 'ae', '1990-01-01', $3, now(), CASE WHEN $4 THEN now() END) RETURNING id",
    )
    .bind(email)
    .bind(hash)
    .bind(format!("T{}", crypto::random_token(6)))
    .bind(verified)
    .fetch_one(&db.st.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn google_flows() {
    let Some(db) = TestDb::new("Google sign-in").await else { return };
    let st = || State(db.st.clone());

    // unverified Google email is refused
    let e = google(st(), ctx("device-aaaaaaaaaaaa"), Ok(Json(greq("111", "new@gmail.com", false)))).await.unwrap_err();
    assert_eq!(code(&e), "google_unverified");

    // new person -> profile_required with a signed ticket and prefilled profile
    let Json(v) = google(st(), ctx("device-aaaaaaaaaaaa"), Ok(Json(greq("111", "New@Gmail.com", true)))).await.unwrap();
    assert_eq!(v["status"], "profile_required");
    assert_eq!(v["profile"]["email"], "new@gmail.com");
    assert_eq!(v["profile"]["first_name"], "Arjun");
    assert_eq!(v["profile"]["referral_code"], "NOSUCHCODE1");
    let ticket_str = v["ticket"].as_str().unwrap().to_string();
    let Json(t) = ticket(st(), ctx("device-aaaaaaaaaaaa"), Ok(Json(TicketReq { ticket: ticket_str.clone() }))).await.unwrap();
    assert_eq!(t["profile"]["last_name"], "Mehta");
    let e = ticket(st(), ctx("device-aaaaaaaaaaaa"), Ok(Json(TicketReq { ticket: format!("{ticket_str}x") }))).await.unwrap_err();
    assert_eq!(code(&e), "google_expired");

    // profile step validates like email sign-up (18+, terms)
    let e = complete(st(), ctx("device-aaaaaaaaaaaa"), Ok(Json(creq(&ticket_str, "2015-01-01")))).await.unwrap_err();
    assert_eq!(code(&e), "date_of_birth");
    let mut no_terms = creq(&ticket_str, "1995-05-05");
    no_terms.accept_terms = false;
    assert_eq!(code(&complete(st(), ctx("device-aaaaaaaaaaaa"), Ok(Json(no_terms))).await.unwrap_err()), "accept_terms");
    // the prefilled partner code matches nobody: refused on the field, so the visitor can fix or clear it
    let mut unknown_ref = creq(&ticket_str, "1995-05-05");
    unknown_ref.referral_code = Some("NOSUCHCODE1".into());
    assert_eq!(code(&complete(st(), ctx("device-aaaaaaaaaaaa"), Ok(Json(unknown_ref))).await.unwrap_err()), "referral_code");

    // complete -> account created, verified, signed in, device trusted, audited
    let (status, Json(v)) = complete(st(), ctx("device-aaaaaaaaaaaa"), Ok(Json(creq(&ticket_str, "1995-05-05")))).await.unwrap();
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(v["status"], "ok");
    assert!(v["session"]["token"].as_str().unwrap().len() > 30);
    assert_eq!(v["user"]["email_verified"], true);
    assert_eq!(v["user"]["google_linked"], true);
    assert_eq!(v["user"]["country"], "in");
    let uid = v["user"]["id"].as_i64().unwrap();
    assert_eq!(db.count("SELECT count(*) FROM trusted_devices WHERE subject_id = $1", uid).await, 1);
    assert_eq!(db.count("SELECT count(*) FROM audit_log WHERE actor_id = $1 AND action = 'user.register' AND meta->>'method' = 'google'", uid).await, 1);
    assert_eq!(db.count("SELECT count(*) FROM audit_log WHERE actor_id = $1 AND action = 'user.login' AND meta->>'via' = 'google'", uid).await, 1);
    // no password was set: password sign-in stays closed
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1").bind(uid).fetch_one(&db.st.pool).await.unwrap();
    assert!(hash.starts_with("$argon2id$"));

    // the same ticket cannot create a second account
    let e = complete(st(), ctx("device-aaaaaaaaaaaa"), Ok(Json(creq(&ticket_str, "1995-05-05")))).await.unwrap_err();
    assert_eq!(code(&e), "google_account_exists");

    // returning Google user on a brand-new device -> signed in straight away (no email code), device trusted
    let Json(v) = google(st(), ctx("device-bbbbbbbbbbbb"), Ok(Json(greq("111", "new@gmail.com", true)))).await.unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["user"]["id"], uid);
    assert_eq!(db.count("SELECT count(*) FROM trusted_devices WHERE subject_id = $1", uid).await, 2);

    // existing verified password account with the same email -> linked, password kept
    let priya = password_user(&db, "priya@gmail.com", true, "Kalks@2026").await;
    let Json(v) = google(st(), ctx("device-cccccccccccc"), Ok(Json(greq("222", "priya@gmail.com", true)))).await.unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["user"]["id"], priya);
    assert_eq!(db.count("SELECT count(*) FROM audit_log WHERE actor_id = $1 AND action = 'user.google_linked'", priya).await, 1);
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1").bind(priya).fetch_one(&db.st.pool).await.unwrap();
    assert!(crypto::verify_password("Kalks@2026", &hash));

    // a different Google account with an already-linked email is refused
    let e = google(st(), ctx("device-cccccccccccc"), Ok(Json(greq("333", "priya@gmail.com", true)))).await.unwrap_err();
    assert_eq!(code(&e), "google_conflict");

    // unverified account squatting on the address -> linked, verified, squatter's password and sessions dropped
    let squat = password_user(&db, "owner@gmail.com", false, "Squat@2026").await;
    sqlx::query("INSERT INTO sessions (tenant_id, subject_kind, subject_id, token_hash, expires_at) VALUES (1, 'user', $1, 'x', now() + interval '1 day')")
        .bind(squat)
        .execute(&db.st.pool)
        .await
        .unwrap();
    let Json(v) = google(st(), ctx("device-dddddddddddd"), Ok(Json(greq("444", "owner@gmail.com", true)))).await.unwrap();
    assert_eq!(v["user"]["id"], squat);
    assert_eq!(v["user"]["email_verified"], true);
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1").bind(squat).fetch_one(&db.st.pool).await.unwrap();
    assert!(!crypto::verify_password("Squat@2026", &hash));
    assert_eq!(db.count("SELECT count(*) FROM sessions WHERE subject_id = $1 AND token_hash = 'x' AND revoked_at IS NULL", squat).await, 0);

    // a Google sign-up for an email that registered in the meantime is refused
    let Json(v) = google(st(), ctx("device-eeeeeeeeeeee"), Ok(Json(greq("555", "race@gmail.com", true)))).await.unwrap();
    let race_ticket = v["ticket"].as_str().unwrap().to_string();
    password_user(&db, "race@gmail.com", true, "Kalks@2026").await;
    let e = complete(st(), ctx("device-eeeeeeeeeeee"), Ok(Json(creq(&race_ticket, "1990-01-01")))).await.unwrap_err();
    assert_eq!(code(&e), "email_taken");

    // blocked (suspended) accounts cannot sign in with Google
    sqlx::query("UPDATE users SET status = 'blocked' WHERE id = $1").bind(uid).execute(&db.st.pool).await.unwrap();
    let e = google(st(), ctx("device-aaaaaaaaaaaa"), Ok(Json(greq("111", "new@gmail.com", true)))).await.unwrap_err();
    assert_eq!(code(&e), "account_suspended");

    // rate limit per IP
    let mut limited = false;
    for _ in 0..40 {
        if let Err(e) = google(st(), ctx("device-ffffffffffff"), Ok(Json(greq("", "x@gmail.com", true)))).await
            && code(&e) == "rate_limited"
        {
            limited = true;
            break;
        }
    }
    assert!(limited);

    db.drop_db().await;
}
