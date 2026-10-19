//! Client profiles: the gateway users feed plus the segment headers the Client Area BFF sends on every call.

use crate::calc::Segment;
use crate::clients;
use crate::db;
use crate::state::AppState;
use chrono::{DateTime, Utc};
use sqlx::Row;

/// What the Client Area BFF tells us about the signed-in client (from its gateway session).
#[derive(Clone, Debug, Default)]
pub struct Hints {
    pub country: Option<String>,
    pub kyc: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub referral_code: Option<String>,
}

/// Upserts the header facts (only fields that were sent overwrite stored ones).
pub async fn touch(st: &AppState, tenant: &str, user_id: i64, h: &Hints) -> anyhow::Result<()> {
    if h.country.is_none() && h.kyc.is_none() && h.created_at.is_none() && h.first_name.is_none() && h.referral_code.is_none() {
        sqlx::query("INSERT INTO profiles (tenant, user_id) VALUES ($1,$2) ON CONFLICT DO NOTHING").bind(tenant).bind(user_id).execute(&st.pool).await?;
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO profiles (tenant, user_id, first_name, last_name, country, kyc_status, referral_code, signed_up_at)
         VALUES ($1,$2,COALESCE($3,''),COALESCE($4,''),COALESCE($5,''),COALESCE($6,'unverified'),COALESCE($7,''),$8)
         ON CONFLICT (tenant, user_id) DO UPDATE SET
           first_name = COALESCE($3, profiles.first_name), last_name = COALESCE($4, profiles.last_name),
           country = COALESCE($5, profiles.country), kyc_status = COALESCE($6, profiles.kyc_status),
           referral_code = COALESCE($7, profiles.referral_code), signed_up_at = COALESCE($8, profiles.signed_up_at)",
    )
    .bind(tenant)
    .bind(user_id)
    .bind(&h.first_name)
    .bind(&h.last_name)
    .bind(h.country.as_ref().map(|c| c.to_uppercase()))
    .bind(&h.kyc)
    .bind(&h.referral_code)
    .bind(h.created_at)
    .execute(&st.pool)
    .await?;
    Ok(())
}

#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub country: String,
    pub kyc: String,
    pub referral_code: String,
    pub signed_up_at: Option<DateTime<Utc>>,
}

impl Profile {
    pub fn segment(&self) -> Segment {
        Segment { country: self.country.clone(), kyc: self.kyc.clone(), signed_up_at: self.signed_up_at }
    }
    pub fn full_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name).trim().to_string()
    }
}

pub async fn get(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<Profile> {
    let r = sqlx::query("SELECT * FROM profiles WHERE tenant = $1 AND user_id = $2").bind(tenant).bind(user_id).fetch_optional(&st.pool).await?;
    Ok(r.map(|r| Profile {
        first_name: r.get("first_name"),
        last_name: r.get("last_name"),
        email: r.get("email"),
        country: r.get("country"),
        kyc: r.get("kyc_status"),
        referral_code: r.get("referral_code"),
        signed_up_at: r.get("signed_up_at"),
    })
    .unwrap_or_default())
}

/// Pulls new and changed clients from the gateway (keyset cursor `(changed_at, id)`).
pub async fn sync_once(st: &AppState) -> anyhow::Result<usize> {
    let cur = db::cursor(&st.pool, "profiles").await?;
    let (mut since, mut after) = match cur.as_deref().and_then(|c| c.split_once('|')) {
        Some((t, id)) => (Some(t.to_string()), id.parse().unwrap_or(0)),
        None => (None, 0),
    };
    let mut n = 0;
    loop {
        let users = clients::gateway_users(st, since.as_deref(), after, 500).await?;
        if users.is_empty() {
            break;
        }
        for u in &users {
            sqlx::query(
                "INSERT INTO profiles (tenant, user_id, first_name, last_name, email, country, kyc_status, referral_code, signed_up_at, synced_at,
                                       status, email_verified_at, kyc_verified_at, last_login_at, birthday, marketing_consent, utm_source, utm_campaign, referred_by)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9, now(), $10,$11,$12,$13,$14,$15,$16,$17,$18)
                 ON CONFLICT (tenant, user_id) DO UPDATE SET first_name = $3, last_name = $4, email = $5, country = $6, kyc_status = $7,
                   referral_code = $8, signed_up_at = $9, synced_at = now(), status = $10, email_verified_at = $11, kyc_verified_at = $12,
                   last_login_at = $13, birthday = $14, marketing_consent = $15, utm_source = $16, utm_campaign = $17, referred_by = $18",
            )
            .bind(&u.tenant)
            .bind(u.id)
            .bind(&u.first_name)
            .bind(&u.last_name)
            .bind(&u.email)
            .bind(u.country.to_uppercase())
            .bind(if u.kyc_status.is_empty() { "unverified" } else { u.kyc_status.as_str() })
            .bind(&u.referral_code)
            .bind(u.created_at)
            .bind(u.status.as_deref().unwrap_or("active"))
            .bind(u.email_verified_at)
            .bind(u.kyc_verified_at)
            .bind(u.last_login_at)
            .bind(u.birthday.as_deref().filter(|b| b.len() == 5))
            .bind(u.marketing_consent.unwrap_or(true))
            .bind(&u.utm_source)
            .bind(&u.utm_campaign)
            .bind(u.referred_by.filter(|r| *r > 0 && *r != u.id))
            .execute(&st.pool)
            .await?;
        }
        n += users.len();
        let last = users.last().unwrap();
        since = Some(last.changed_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true));
        after = last.id;
        db::set_cursor(&st.pool, "profiles", &format!("{}|{}", since.as_deref().unwrap_or(""), after)).await?;
        if users.len() < 500 {
            break;
        }
    }
    Ok(n)
}
