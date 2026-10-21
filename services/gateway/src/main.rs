//! Kalks gateway: identity and sessions for the Client Area (clients) and Back Office (staff).
//!
//! Browsers never talk to this service directly. Each Next.js app proxies `/api/auth/*` through its own
//! route handlers, which keep the opaque session token in a first-party HttpOnly cookie and forward it here
//! as a bearer token together with `X-Kalks-Internal`.

mod admin;
mod audit;
mod client_auth;
mod client_controls;
mod client_erase;
mod client_security;
mod config;
mod crypto;
mod db;
mod domains;
mod error;
mod flows;
mod google_auth;
mod house;
mod identity;
mod kyc;
mod internal;
mod mail_i18n;
mod mailer;
mod marketing;
mod owner;
mod ratelimit;
mod rbac;
mod sections;
mod shares;
mod staff_admin;
mod staff_auth;
mod state;
mod suitability;
mod tenancy;
mod stepup;
#[cfg(test)]
mod testdb;
mod users_internal;
mod validate;

use axum::extract::{Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use std::sync::Arc;
use std::time::Duration;

use crate::error::ApiError;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // repo-root .env.local in development; real env vars win in production
    let env_file = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.local"));
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into()))
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();
    if let Err(e) = env_file
        && !e.not_found()
    {
        tracing::warn!(error = %e, ".env.local could not be fully parsed");
    }

    let cfg = config::Config::from_env()?;
    tracing::info!(?cfg, "gateway starting");
    if cfg.internal_token.is_empty() {
        tracing::warn!("GATEWAY_INTERNAL_TOKEN is empty: any local process can call the gateway (dev only)");
    }
    let pool = db::connect(&cfg.database_url).await?;
    db::seed_super_admin(&pool, &cfg).await?;
    let _ = crypto::dummy_hash();
    kyc::init();

    let mailer = if cfg.smtp_configured {
        let m = mailer::Mailer::new(mailer::SmtpSettings {
            host: &cfg.smtp_host,
            port: cfg.smtp_port,
            user: &cfg.smtp_user,
            password: &cfg.smtp_password,
            from: &cfg.smtp_from,
        }, mailer::Links {
            site: cfg.site_url.clone(),
            app: cfg.app_url.clone(),
            trade: cfg.trade_url.clone(),
            support_email: cfg.support_email.clone(),
        })?;
        match m.test_connection().await {
            Ok(true) => tracing::info!(host = %cfg.smtp_host, "SMTP login ok"),
            Ok(false) => tracing::error!(host = %cfg.smtp_host, "SMTP server did not accept the connection"),
            Err(e) => tracing::error!(host = %cfg.smtp_host, error = %e, "SMTP connection failed"),
        }
        Some(m)
    } else {
        None
    };
    // `gateway send-test-emails <address>`: sends one of every email template, then exits.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("send-test-emails") {
        let to = args.get(2).ok_or_else(|| anyhow::anyhow!("usage: gateway send-test-emails <address>"))?;
        let m = mailer.as_ref().ok_or_else(|| anyhow::anyhow!("SMTP is not configured"))?;
        m.send_welcome(to, "Shivam").await?;
        for p in [identity::Purpose::VerifyEmail, identity::Purpose::Login, identity::Purpose::ResetPassword] {
            m.send_code(to, p, "482915", 10, None).await?;
        }
        m.send_code(to, identity::Purpose::Confirm, "482915", 10, Some(&stepup::describe("trading_password", "10000123"))).await?;
        let reference = "KYC-000042".to_string();
        for mail in [
            mailer::KycMail::Submitted { reference: reference.clone(), hours: 4 },
            mailer::KycMail::MoreInfo { reference: reference.clone(), items: vec!["Proof of address".into()], message: Some("The bank statement is older than 3 months.".into()) },
            mailer::KycMail::Rejected { reference: reference.clone(), reason: "The identity document has expired".into(), message: None, can_resubmit: true },
            mailer::KycMail::Approved { reference },
        ] {
            m.send_kyc(to, "Shivam", &mail).await?;
        }
        println!("sent 9 test emails to {to}");
        return Ok(());
    }
    // `gateway kyc-erase-user <user_id>`: removes a client's KYC documents (files + rows), then exits.
    if args.get(1).map(String::as_str) == Some("kyc-erase-user") {
        let id: i64 = args.get(2).and_then(|v| v.parse().ok()).ok_or_else(|| anyhow::anyhow!("usage: gateway kyc-erase-user <user_id>"))?;
        let n = kyc::erase_user(&pool, id).await?;
        println!("erased {n} KYC file(s) of user {id}");
        return Ok(());
    }
    let st = AppState { pool, keys: crypto::Keys::new(&cfg.session_secret), cfg: Arc::new(cfg), limiter: Default::default(), mailer };

    // keep the limiter bounded; purge long-dead sessions and codes
    {
        let st = st.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(300));
            loop {
                tick.tick().await;
                st.limiter.sweep(Duration::from_secs(3600));
                let _ = sqlx::query("DELETE FROM email_otps WHERE expires_at < now() - interval '1 day'").execute(&st.pool).await;
                let _ = sqlx::query("DELETE FROM stepup_tokens WHERE expires_at < now() - interval '1 day'").execute(&st.pool).await;
                let _ = sqlx::query("DELETE FROM sessions WHERE expires_at < now() - interval '30 days'").execute(&st.pool).await;
                let _ = owner::mark_overdue(&st.pool).await;
            }
        });
    }
    // client controls: expired restrictions and staff sessions, stale Kalks Trader connections (client_controls.rs)
    {
        let st = st.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(30));
            loop {
                tick.tick().await;
                if let Err(e) = client_controls::sweep(&st).await {
                    tracing::warn!(error = %e, "client controls sweep failed");
                }
            }
        });
    }

    let bind = st.cfg.bind.clone();
    let app = router(st);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, "http listening");
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn router(st: AppState) -> Router {
    let v1 = Router::new()
        .route("/v1/auth/register", post(client_auth::register))
        .route("/v1/auth/login", post(client_auth::login))
        .route("/v1/auth/logout", post(client_auth::logout))
        .route("/v1/auth/me", get(client_auth::me))
        .route("/v1/auth/forgot", post(client_auth::forgot))
        .route("/v1/auth/reset", post(client_auth::reset))
        .route("/v1/auth/verify-email", post(client_auth::verify_email))
        .route("/v1/auth/resend", post(client_auth::resend))
        .route("/v1/auth/password", post(stepup::change_password))
        .route("/v1/auth/stepup", post(stepup::request))
        .route("/v1/auth/stepup/resend", post(stepup::resend))
        .route("/v1/auth/stepup/verify", post(stepup::verify))
        .route("/v1/auth/stepup/consume", post(stepup::consume))
        // Client Area security: sessions, login history, view-only logins, closure / export (client_security.rs)
        .route("/v1/auth/sessions", get(client_security::sessions))
        .route("/v1/auth/sessions/revoke-others", post(client_security::revoke_others))
        .route("/v1/auth/sessions/{id}/revoke", post(client_security::revoke_session))
        .route("/v1/auth/logins", get(client_security::logins))
        .route("/v1/auth/viewers", get(client_security::viewers).post(client_security::create_viewer))
        .route("/v1/auth/viewers/{id}", patch(client_security::update_viewer))
        .route("/v1/auth/viewers/{id}/password", post(client_security::viewer_password))
        .route("/v1/auth/viewers/{id}/revoke", post(client_security::revoke_viewer))
        .route("/v1/auth/viewer/activity", post(client_security::viewer_activity))
        .route("/v1/auth/requests", get(client_security::requests).post(client_security::create_request))
        .route("/v1/auth/requests/{id}/cancel", post(client_security::cancel_request))
        .route("/v1/auth/requests/{id}/export", get(client_security::export))
        .route("/v1/admin/users/{id}/security", get(client_security::admin_user_security))
        .route("/v1/admin/users/{id}/sessions/revoke-all", post(client_security::admin_revoke_all))
        .route("/v1/admin/users/{id}/viewers/{vid}/revoke", post(client_security::admin_revoke_viewer))
        .route("/v1/admin/requests", get(client_security::admin_requests))
        .route("/v1/admin/requests/{id}", post(client_security::admin_process_request))
        .route("/v1/admin/settings/sessions", get(client_security::get_session_settings).put(client_security::set_session_settings))
        // client controls: presence, restrictions, staff sessions as the client (client_controls.rs)
        .route("/v1/admin/presence", get(client_controls::presence_list))
        .route("/v1/admin/users/{id}/controls", get(client_controls::controls))
        .route("/v1/admin/users/{id}/restrictions/{kind}", put(client_controls::put_restriction))
        .route("/v1/admin/users/{id}/restrictions/{kind}/lift", post(client_controls::lift))
        .route("/v1/admin/restrictions/bulk", post(client_controls::bulk))
        .route("/v1/admin/users/{id}/impersonate", post(client_controls::impersonate))
        .route("/v1/admin/users/{id}/erase", post(client_erase::erase))
        .route("/v1/admin/users/{id}/impersonate/trader", post(client_controls::impersonate_trader))
        .route("/v1/admin/impersonations/{id}/end", post(client_controls::end_impersonation))
        .route("/v1/auth/impersonate/redeem", post(client_controls::redeem))
        .route("/v1/auth/heartbeat", post(client_controls::heartbeat))
        .route("/v1/auth/impersonation/event", post(client_controls::event))
        .route("/v1/internal/restrictions", get(client_controls::internal_list))
        .route("/v1/internal/presence/trader", post(client_controls::trader_report))
        .route("/v1/internal/impersonation/ended", post(client_controls::trader_ended))
        .route("/v1/auth/google", post(google_auth::google))
        .route("/v1/auth/google/ticket", post(google_auth::ticket))
        .route("/v1/auth/google/complete", post(google_auth::complete))
        .route("/v1/admin/auth/login", post(staff_auth::login))
        .route("/v1/admin/auth/verify-otp", post(staff_auth::verify_otp))
        .route("/v1/admin/auth/resend", post(staff_auth::resend))
        .route("/v1/admin/auth/logout", post(staff_auth::logout))
        .route("/v1/admin/auth/me", get(staff_auth::me))
        .route("/v1/admin/stats", get(admin::stats))
        .route("/v1/admin/users", get(admin::users))
        .route("/v1/admin/users/{id}", get(admin::user_detail))
        .route("/v1/admin/audit", get(admin::audit_log))
        .route("/v1/admin/audit/record", post(admin::record))
        .route("/v1/admin/staff", get(admin::staff_list))
        .route("/v1/admin/sessions", get(admin::sessions))
        .route("/v1/admin/sessions/{id}/revoke", post(admin::revoke_session))
        // staff, roles, security, tenant settings (staff_admin.rs, tenancy.rs)
        .route("/v1/admin/auth/invite", get(staff_admin::invite_info))
        .route("/v1/admin/auth/invite/accept", post(staff_admin::invite_accept))
        .route("/v1/admin/permissions", get(staff_admin::permissions_catalogue))
        .route("/v1/admin/staff/invite", post(staff_admin::invite))
        .route("/v1/admin/staff/{id}", get(staff_admin::detail).patch(staff_admin::update))
        .route("/v1/admin/staff/{id}/resend-invite", post(staff_admin::resend_invite))
        .route("/v1/admin/staff/{id}/disable", post(staff_admin::disable))
        .route("/v1/admin/staff/{id}/enable", post(staff_admin::enable))
        .route("/v1/admin/staff/{id}/reset-2fa", post(staff_admin::reset_2fa))
        .route("/v1/admin/staff/{id}/sign-out", post(staff_admin::sign_out))
        .route("/v1/admin/roles", get(staff_admin::roles).post(staff_admin::create_role))
        .route("/v1/admin/roles/{id}", patch(staff_admin::update_role).delete(staff_admin::delete_role))
        .route("/v1/admin/roles/{id}/reset", post(staff_admin::reset_role))
        .route("/v1/admin/security/ip", get(staff_admin::ip_list).post(staff_admin::ip_add))
        .route("/v1/admin/security/ip/settings", put(staff_admin::ip_settings))
        .route("/v1/admin/security/ip/{id}", delete(staff_admin::ip_remove))
        .route("/v1/admin/settings/maintenance", get(tenancy::get_maintenance).put(tenancy::set_maintenance))
        .route("/v1/admin/settings/features", get(tenancy::get_features))
        .route("/v1/admin/settings/features/{key}", put(tenancy::set_feature))
        // Client Area sections and targeting rules (sections.rs)
        .route("/v1/admin/settings/sections", get(sections::list))
        .route("/v1/admin/settings/sections/preview", get(sections::preview))
        .route("/v1/admin/settings/sections/rules/{id}", delete(sections::remove_rule))
        .route("/v1/admin/settings/sections/{key}", put(sections::toggle))
        .route("/v1/admin/settings/sections/{key}/rules", post(sections::set_rule))
        .route("/v1/me/features", get(sections::me))
        .route("/v1/public/tenant-config", get(domains::tenant_config))
        .route("/v1/public/tenant-by-host", get(domains::tenant_by_host))
        .route("/v1/owner/tenants/{id}/domains", get(domains::owner_list).post(domains::owner_add))
        .route("/v1/owner/tenants/{id}/domains/{domain_id}", patch(domains::owner_update).delete(domains::owner_delete))
        .route("/v1/owner/tenants/{id}/domains/{domain_id}/check", post(domains::owner_check))
        // Platform Owner (owner.rs)
        .route("/v1/owner/dashboard", get(owner::dashboard))
        .route("/v1/owner/tenants", get(owner::tenants).post(owner::create_tenant))
        .route("/v1/owner/tenants/{id}", get(owner::tenant_detail).patch(owner::update_tenant))
        .route("/v1/owner/tenants/{id}/suspend", post(owner::suspend_tenant))
        .route("/v1/owner/tenants/{id}/activate", post(owner::activate_tenant))
        .route("/v1/owner/tenants/{id}/features", put(owner::set_tenant_features))
        .route("/v1/owner/tenants/{id}/invite-admin", post(owner::invite_admin))
        .route("/v1/owner/tenants/{id}/billing", put(owner::set_billing))
        .route("/v1/owner/features", get(owner::feature_catalogue).post(owner::create_flag))
        .route("/v1/owner/features/{key}", patch(owner::update_flag).delete(owner::delete_flag))
        .route("/v1/owner/billing", get(owner::billing_overview))
        .route("/v1/owner/invoices", get(owner::invoices).post(owner::create_invoice))
        .route("/v1/owner/invoices/{id}/status", post(owner::invoice_status))
        .route("/v1/kyc", get(kyc::get))
        .route("/v1/kyc/start", post(kyc::start))
        .route("/v1/kyc/details", post(kyc::details))
        .route("/v1/kyc/documents", post(kyc::upload).layer(DefaultBodyLimit::max(kyc::UPLOAD_BODY_LIMIT)))
        .route("/v1/kyc/submit", post(kyc::submit))
        .route("/v1/admin/kyc/cases", get(kyc::staff::queue))
        .route("/v1/admin/kyc/cases/{id}", get(kyc::staff::case_detail))
        .route("/v1/admin/kyc/cases/{id}/claim", post(kyc::staff::claim))
        .route("/v1/admin/kyc/cases/{id}/approve", post(kyc::staff::approve))
        .route("/v1/admin/kyc/cases/{id}/reject", post(kyc::staff::reject))
        .route("/v1/admin/kyc/cases/{id}/request-info", post(kyc::staff::request_info))
        .route("/v1/admin/kyc/cases/{id}/notes", post(kyc::staff::note))
        .route("/v1/admin/kyc/documents/{id}/file", get(kyc::staff::file))
        // suitability for complex products: KYC + risk disclosure + knowledge quiz (suitability.rs)
        .route("/v1/suitability/{product}", get(suitability::get))
        .route("/v1/suitability/{product}/accept", post(suitability::accept))
        .route("/v1/suitability/{product}/quiz", post(suitability::quiz))
        .route("/v1/internal/suitability/{user_id}", get(suitability::internal))
        .route("/v1/shares", post(shares::create))
        .route("/v1/shares/lookup", post(shares::lookup))
        .route("/v1/shares/{code}/trades", patch(shares::update_trades))
        .route("/v1/shares/{code}/revoke", post(shares::revoke))
        .route("/v1/public/shares/{code}", get(shares::public))
        .route("/v1/internal/referrals/users", get(internal::referral_users))
        .route("/v1/internal/users/{id}", get(users_internal::user))
        .route("/v1/internal/tenants/{slug}", get(internal::tenant))
        .route("/v1/internal/house-users", post(house::create))
        .route("/v1/internal/mail/marketing", post(marketing::send_marketing))
        .route("/v1/public/unsubscribe", post(marketing::unsubscribe))
        .route("/v1/auth/marketing", get(marketing::get_consent).put(marketing::put_consent).post(marketing::put_consent))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn(edge_country))
        .layer(middleware::from_fn(mail_i18n::locale_layer))
        .layer(middleware::from_fn_with_state(st.clone(), internal_only));
    Router::new()
        .route("/health", get(health))
        // Caddy on-demand TLS `ask` (no internal token: loopback-only, yes / no answer; domains.rs)
        .route("/v1/public/domain-check", get(domains::domain_check))
        .merge(v1)
        .fallback(|| async { (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"error": {"code": "not_found", "message": "Not found."}}))) })
        .with_state(st)
}

async fn health(State(st): State<AppState>) -> impl IntoResponse {
    let db = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&st.pool).await.is_ok();
    Json(serde_json::json!({ "status": if db { "ok" } else { "degraded" }, "db": db, "service": "gateway" }))
}

/// Only the apps' server-side route handlers may call /v1 (shared secret in `X-Kalks-Internal`).
async fn internal_only(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let expected = st.cfg.internal_token.as_bytes();
    if !expected.is_empty() {
        let got = req.headers().get("x-kalks-internal").map(|v| v.as_bytes()).unwrap_or_default();
        if !crypto::ct_eq(got, expected) {
            return ApiError::Forbidden.into_response();
        }
    }
    next.run(req).await
}

/// Approximate location of the caller (`X-Kalks-Country`, from the edge's CF-IPCountry via the apps), available to
/// session creation for the request's lifetime.
async fn edge_country(req: Request, next: Next) -> Response {
    let c = identity::clean_country(req.headers().get("x-kalks-country").and_then(|v| v.to_str().ok()));
    identity::COUNTRY.scope(c, next.run(req)).await
}
