//! Service-to-service reads (no end-user session): `/v1/internal/*`, behind the same `X-Kalks-Internal`
//! check as every other /v1 route.
//!
//! `GET /v1/internal/referrals/users?since=<RFC 3339>&after_id=<id>&limit=<1..1000>`
//!
//! Used by the IB service (services/ib) to mirror the referral tree. Rows come in keyset order of
//! `(changed_at, id)`, where `changed_at = greatest(updated_at, last_login_at)`, so a poller that stores the
//! last `(changed_at, id)` it saw gets every new client and every client that changed or signed in again
//! (new IPs / devices). Identity and device signals for self-referral checks are sent as keyed hashes only:
//! `identity` = HMAC of (first name, last name, date of birth) and of the full phone number, `devices` = the
//! stored device hashes, `ips` = sign-up and session IPs (last 20).

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ReferralUsersQ {
    since: Option<String>,
    after_id: Option<i64>,
    limit: Option<i64>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub async fn referral_users(State(st): State<AppState>, q: Result<Query<ReferralUsersQ>, QueryRejection>) -> ApiResult<Json<Value>> {
    let Query(q) = q.map_err(|_| ApiError::BadRequest("Invalid query."))?;
    let since = match q.since.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => DateTime::parse_from_rfc3339(s).map_err(|_| ApiError::BadRequest("since must be RFC 3339."))?.with_timezone(&Utc),
        None => DateTime::<Utc>::UNIX_EPOCH,
    };
    let after_id = q.after_id.unwrap_or(0);
    let limit = q.limit.unwrap_or(500).clamp(1, 1000);
    let rows = sqlx::query(
        "SELECT * FROM (
            SELECT u.id, t.slug AS tenant, u.email, u.first_name, u.last_name, u.country, u.date_of_birth,
                   u.phone_dial || u.phone AS phone, u.referral_code, u.referred_by, u.referred_code_raw, u.referral_campaign,
                   u.kyc_status, u.status, u.email_verified_at IS NOT NULL AS email_verified, u.created_at,
                   u.email_verified_at, u.last_login_at, to_char(u.date_of_birth, 'MM-DD') AS birthday,
                   u.utm_source, u.utm_medium, u.utm_campaign, u.utm_term, u.utm_content, u.landing_page, u.first_referrer,
                   u.marketing_consent,
                   (SELECT max(k.decided_at) FROM kyc_cases k WHERE k.user_id = u.id AND k.status = 'approved') AS kyc_verified_at,
                   GREATEST(u.updated_at, COALESCE(u.last_login_at, u.updated_at)) AS changed_at,
                   ARRAY(SELECT DISTINCT x.ip FROM (
                            (SELECT s.ip FROM sessions s WHERE s.subject_kind = 'user' AND s.subject_id = u.id AND s.ip IS NOT NULL
                             ORDER BY s.created_at DESC LIMIT 20)
                            UNION
                            (SELECT a.ip FROM audit_log a WHERE a.actor_kind = 'user' AND a.actor_id = u.id AND a.action = 'user.register' AND a.ip IS NOT NULL LIMIT 1)
                         ) x) AS ips,
                   ARRAY(SELECT d.device_hash FROM trusted_devices d WHERE d.subject_kind = 'user' AND d.subject_id = u.id
                         ORDER BY d.last_seen_at DESC LIMIT 20) AS devices
            FROM users u JOIN tenants t ON t.id = u.tenant_id
            WHERE NOT u.is_house
         ) z
         WHERE (z.changed_at, z.id) > ($1, $2)
         ORDER BY z.changed_at, z.id
         LIMIT $3",
    )
    .bind(since)
    .bind(after_id)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let first: String = r.get("first_name");
            let last: String = r.get("last_name");
            let dob: NaiveDate = r.get("date_of_birth");
            let phone: String = r.get("phone");
            let ident = format!("{}|{}|{}", first.trim().to_lowercase(), last.trim().to_lowercase(), dob);
            let devices: Vec<Vec<u8>> = r.get("devices");
            json!({
                "id": r.get::<i64, _>("id"),
                "tenant": r.get::<String, _>("tenant"),
                "email": r.get::<String, _>("email"),
                "first_name": first,
                "last_name": last,
                "country": r.get::<String, _>("country"),
                "referral_code": r.get::<String, _>("referral_code"),
                "referred_by": r.get::<Option<i64>, _>("referred_by"),
                "referred_code_raw": r.get::<Option<String>, _>("referred_code_raw"),
                "referral_campaign": r.get::<Option<String>, _>("referral_campaign"),
                "kyc_status": r.get::<String, _>("kyc_status"),
                "status": r.get::<String, _>("status"),
                "email_verified": r.get::<bool, _>("email_verified"),
                "created_at": r.get::<DateTime<Utc>, _>("created_at"),
                "email_verified_at": r.get::<Option<DateTime<Utc>>, _>("email_verified_at"),
                "kyc_verified_at": r.get::<Option<DateTime<Utc>>, _>("kyc_verified_at"),
                "last_login_at": r.get::<Option<DateTime<Utc>>, _>("last_login_at"),
                "birthday": r.get::<Option<String>, _>("birthday"),
                "utm_source": r.get::<Option<String>, _>("utm_source"),
                "utm_medium": r.get::<Option<String>, _>("utm_medium"),
                "utm_campaign": r.get::<Option<String>, _>("utm_campaign"),
                "utm_term": r.get::<Option<String>, _>("utm_term"),
                "utm_content": r.get::<Option<String>, _>("utm_content"),
                "landing_page": r.get::<Option<String>, _>("landing_page"),
                "referrer": r.get::<Option<String>, _>("first_referrer"),
                "marketing_consent": r.get::<bool, _>("marketing_consent"),
                "changed_at": r.get::<DateTime<Utc>, _>("changed_at"),
                "identity": [hex(&st.keys.hash("ib-identity", &ident)), hex(&st.keys.hash("ib-phone", &phone))],
                "ips": r.get::<Vec<String>, _>("ips"),
                "devices": devices.iter().map(|d| hex(d)).collect::<Vec<_>>(),
            })
        })
        .collect();
    let next = items.last().map(|v| json!({"since": v["changed_at"], "after_id": v["id"]}));
    Ok(Json(json!({ "items": items, "next": next })))
}

/// `GET /v1/internal/tenants/{slug}`: a broker's id, name and status. The trading engine and the wallet keep their
/// own tenant rows (ids mirror this table) and provision a broker the Platform Owner created (D110) on its first
/// request. `modules` is the broker's effective module map (`key -> on`, tenancy.rs): the services refuse a switched-off
/// module's client calls with 403 `module_disabled` (each caches it for 30 s; a missing key counts as on).
pub async fn tenant(State(st): State<AppState>, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let slug = slug.trim().to_lowercase();
    if slug.is_empty() || slug.len() > 64 || !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(ApiError::NotFound);
    }
    let row = sqlx::query("SELECT id, slug, name, status FROM tenants WHERE slug = $1").bind(&slug).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let id: i64 = row.get("id");
    Ok(Json(json!({
        "id": id,
        "slug": row.get::<String, _>("slug"),
        "name": row.get::<String, _>("name"),
        "status": row.get::<String, _>("status"),
        "modules": crate::tenancy::module_map(&st.pool, id).await?,
    })))
}

#[cfg(test)]
mod tests {
    use crate::error::ApiError;
    use crate::testdb::TestDb;
    use axum::extract::{Path, Query, State};

    #[tokio::test]
    async fn looks_up_a_broker_by_slug() {
        let Some(db) = TestDb::new("internal tenant").await else { return };
        let v = super::tenant(State(db.st.clone()), Path("Kalks".into())).await.unwrap().0;
        assert_eq!((v["slug"].as_str(), v["status"].as_str()), (Some("kalks"), Some("active")));
        assert!(v["id"].as_i64().is_some_and(|id| id > 0));
        // the effective modules: every built-in one, on by default; an override shows
        assert_eq!(v["modules"]["options"], true);
        assert_eq!(v["modules"]["support_chat"], true);
        assert!(v["modules"].get("client_registration").is_none(), "flags are not modules");
        sqlx::query("INSERT INTO tenant_features (tenant_id, key, enabled) VALUES ($1, 'options', false)").bind(v["id"].as_i64().unwrap()).execute(&db.st.pool).await.unwrap();
        let v = super::tenant(State(db.st.clone()), Path("kalks".into())).await.unwrap().0;
        assert_eq!((v["modules"]["options"].as_bool(), v["modules"]["news"].as_bool()), (Some(false), Some(true)));
        assert!(matches!(super::tenant(State(db.st.clone()), Path("no-such-broker".into())).await, Err(ApiError::NotFound)));
        assert!(matches!(super::tenant(State(db.st.clone()), Path("../etc".into())).await, Err(ApiError::NotFound)));
        db.drop_db().await;
    }

    #[tokio::test]
    async fn lists_users_in_keyset_order_with_signals() {
        let Some(db) = TestDb::new("internal referrals").await else { return };
        let pool = &db.st.pool;
        let tid: i64 = sqlx::query_scalar("SELECT id FROM tenants WHERE slug = 'kalks'").fetch_one(pool).await.unwrap();
        let mut ids = vec![];
        for (i, code) in ["AAAA1111", "BBBB2222"].iter().enumerate() {
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO users (tenant_id, email, password_hash, first_name, last_name, phone_dial, phone, country, date_of_birth,
                                    referral_code, referred_by, referral_campaign, terms_accepted_at)
                 VALUES ($1, $2, 'x', 'Ann', 'Lee', '+91', '9000000000', 'IN', '1990-01-01', $3, $4, $5, now()) RETURNING id",
            )
            .bind(tid)
            .bind(format!("u{i}@example.com"))
            .bind(code)
            .bind(ids.first().copied())
            .bind(if i == 1 { Some("yt") } else { None })
            .fetch_one(pool)
            .await
            .unwrap();
            ids.push(id);
        }
        sqlx::query("INSERT INTO sessions (tenant_id, subject_kind, subject_id, token_hash, ip, expires_at) VALUES ($1,'user',$2,'\\x01','203.0.113.9', now())")
            .bind(tid)
            .bind(ids[1])
            .execute(pool)
            .await
            .unwrap();
        let first = super::ReferralUsersQ { since: None, after_id: None, limit: Some(10) };
        let v = super::referral_users(State(db.st.clone()), Ok(Query(first))).await.unwrap().0;
        let items = v["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1]["referred_by"], ids[0]);
        assert_eq!(items[1]["referral_campaign"], "yt");
        assert_eq!(items[1]["ips"][0], "203.0.113.9");
        // same name + date of birth -> same identity hash
        assert_eq!(items[0]["identity"][0], items[1]["identity"][0]);
        // resume after the last row: nothing new
        let next = &v["next"];
        let v2 = super::referral_users(
            State(db.st.clone()),
            Ok(Query(super::ReferralUsersQ { since: next["since"].as_str().map(str::to_string), after_id: next["after_id"].as_i64(), limit: None })),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(v2["items"].as_array().unwrap().len(), 0);
        db.drop_db().await;
    }
}
