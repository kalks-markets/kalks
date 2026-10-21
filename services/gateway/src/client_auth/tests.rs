//! Email sign-up tests against a throwaway PostgreSQL database (GATEWAY_TEST_DATABASE_URL, default the local
//! dev server on :5433), skipped with a note when no server is reachable.

use super::*;
use crate::testdb::TestDb;

fn ctx() -> Ctx {
    Ctx { ip: "203.0.113.9".into(), user_agent: "test".into(), device: Some("device-rrrrrrrrrrrr".into()), tenant_slug: "kalks".into(), bearer: None }
}

fn req(email: &str, referral: Option<&str>) -> RegisterReq {
    RegisterReq {
        first_name: "Arjun".into(),
        last_name: "Mehta".into(),
        email: email.into(),
        phone_dial: "+91".into(),
        phone: "98201 44721".into(),
        country: "IN".into(),
        date_of_birth: "1995-05-05".into(),
        referral_code: referral.map(Into::into),
        referral_campaign: Some("spring-promo".into()),
        attribution: None,
        marketing_consent: Some(false),
        password: "Kalks@2026x".into(),
        accept_terms: true,
    }
}

async fn referral_of(db: &TestDb, email: &str) -> (Option<i64>, Option<String>, Option<String>) {
    sqlx::query_as("SELECT referred_by, referred_code_raw, referral_campaign FROM users WHERE tenant_id = 1 AND email = $1")
        .bind(email)
        .fetch_one(&db.st.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn register_referral_codes() {
    let Some(db) = TestDb::new("register").await else { return };
    let st = || State(db.st.clone());

    // the partner whose code is shared
    let partner: i64 = sqlx::query_scalar(
        "INSERT INTO users (tenant_id, email, password_hash, first_name, last_name, phone_dial, phone, country, date_of_birth,
                            referral_code, terms_accepted_at, email_verified_at)
         VALUES (1, 'partner@example.com', 'x', 'Priya', 'Shah', '+971', '501234567', 'ae', '1990-01-01', 'PRIYA1234', now(), now()) RETURNING id",
    )
    .fetch_one(&db.st.pool)
    .await
    .unwrap();

    // a code that resolves links the new client to the partner (typed in any case, campaign kept)
    let (status, _) = register(st(), ctx(), Ok(Json(req("linked@example.com", Some(" priya1234 "))))).await.unwrap();
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(referral_of(&db, "linked@example.com").await, (Some(partner), Some("PRIYA1234".into()), Some("spring-promo".into())));

    // an unknown code is refused on the referral field, and no account is created
    let e = register(st(), ctx(), Ok(Json(req("typo@example.com", Some("PRIYA1243"))))).await.unwrap_err();
    assert!(matches!(e, ApiError::Validation { field: "referral_code", message: UNKNOWN_REFERRAL }));
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = 'typo@example.com'").fetch_one(&db.st.pool).await.unwrap();
    assert_eq!(n, 0);

    // no code (absent or blank) -> account created without a partner
    for (email, code) in [("plain@example.com", None), ("blank@example.com", Some("  "))] {
        let (status, _) = register(st(), ctx(), Ok(Json(req(email, code)))).await.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(referral_of(&db, email).await, (None, None, None));
    }

    db.drop_db().await;
}
