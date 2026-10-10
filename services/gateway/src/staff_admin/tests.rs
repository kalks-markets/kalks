//! RBAC, staff administration, IP allow-list, maintenance / features and Platform Owner flows against a
//! throwaway PostgreSQL database (see `testdb`); skipped with a note when no server is reachable.

#![allow(unused_must_use)]

use super::*;
use crate::testdb::TestDb;
use crate::{owner, tenancy};
use axum::extract::Path;
use serde_json::Map;

fn ctx(tok: Option<&str>, ip: &str) -> Ctx {
    Ctx { ip: ip.into(), user_agent: "test-agent".into(), device: Some("device-0123456789abcdef".into()), tenant_slug: "kalks".into(), bearer: tok.map(str::to_string) }
}

fn code(e: &ApiError) -> &'static str {
    match e {
        ApiError::Coded { code, .. } => code,
        ApiError::Forbidden => "forbidden_plain",
        ApiError::Unauthorized => "unauthorized",
        ApiError::NotFound => "not_found",
        ApiError::BadRequest(_) => "bad_request",
        ApiError::Validation { .. } => "validation",
        _ => "other",
    }
}

async fn kalks(st: &AppState) -> i64 {
    sqlx::query_scalar("SELECT id FROM tenants WHERE slug = 'kalks'").fetch_one(&st.pool).await.unwrap()
}

async fn role_id(st: &AppState, tenant: i64, key: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM roles WHERE tenant_id = $1 AND key = $2").bind(tenant).bind(key).fetch_one(&st.pool).await.unwrap()
}

/// Staff member with a real password and a role, plus a live session token.
async fn staff(st: &AppState, tenant: i64, email: &str, role: &str) -> (i64, String) {
    let hash = crate::crypto::hash_password("Correct-horse-9").unwrap();
    let rid = role_id(st, tenant, role).await;
    let id: i64 = sqlx::query_scalar("INSERT INTO staff (tenant_id, email, password_hash, name, role, role_id) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id")
        .bind(tenant)
        .bind(email)
        .bind(hash)
        .bind(format!("Staff {role}"))
        .bind(role)
        .bind(rid)
        .fetch_one(&st.pool)
        .await
        .unwrap();
    let tok = identity::create_session(st, &ctx(None, "203.0.113.9"), Kind::Staff, tenant, id).await.unwrap().token;
    (id, tok)
}

async fn me_json(st: &AppState, tok: &str, ip: &str) -> ApiResult<Value> {
    crate::staff_auth::me(State(st.clone()), ctx(Some(tok), ip)).await.map(|Json(v)| v)
}

async fn audit_count(st: &AppState, action: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = $1").bind(action).fetch_one(&st.pool).await.unwrap()
}

#[tokio::test]
async fn roles_invites_and_staff_management() {
    let Some(db) = TestDb::new("rbac").await else { return };
    let st = db.st.clone();
    let t = kalks(&st).await;
    let ip = "203.0.113.9";

    // presets exist; the owner's /me is authoritative and complete
    let presets: i64 = sqlx::query_scalar("SELECT count(*) FROM roles WHERE tenant_id = $1").bind(t).fetch_one(&st.pool).await.unwrap();
    assert_eq!(presets as usize, rbac::BUILTIN_ROLES.len());
    let (owner_id, owner) = staff(&st, t, "owner@example.com", "platform_owner").await;
    let v = me_json(&st, &owner, ip).await.unwrap();
    assert_eq!(v["staff"]["rbac"], true);
    assert_eq!(v["staff"]["is_owner"], true);
    assert_eq!(v["staff"]["permissions"].as_array().unwrap().len(), rbac::PERMS.len());
    let (_sup_id, support) = staff(&st, t, "support@example.com", "support").await;
    let v = me_json(&st, &support, ip).await.unwrap();
    assert_eq!(v["staff"]["role"], "support");
    assert!(v["staff"]["permissions"].as_array().unwrap().contains(&json!("support.write")));
    assert!(!v["staff"]["permissions"].as_array().unwrap().contains(&json!("staff.read")));

    // support can't read staff or build roles
    assert!(matches!(roles(State(st.clone()), ctx(Some(&support), ip)).await, Err(ApiError::Forbidden)));
    let (_, admin) = staff(&st, t, "admin@example.com", "admin").await;

    // role builder: can't grant owner perms, can't grant what you don't hold
    let mk = |tok: &str, name: &str, perms: &[&str]| {
        let (st, c) = (st.clone(), ctx(Some(tok), ip));
        let body = RoleReq { name: Some(name.into()), description: Some("KYC desk".into()), permissions: Some(perms.iter().map(|s| s.to_string()).collect()) };
        async move { create_role(State(st), c, Ok(Json(body))).await }
    };
    assert_eq!(code(&mk(&admin, "Bad", &["owner.tenants"]).await.unwrap_err()), "forbidden");
    let (_, compliance) = staff(&st, t, "compliance@example.com", "compliance").await;
    assert!(matches!(mk(&compliance, "Nope", &["kyc.read"]).await, Err(ApiError::Forbidden)), "compliance lacks staff.roles");
    let (_, Json(r)) = mk(&admin, "KYC Desk", &["kyc.review", "clients.read"]).await.unwrap();
    let kyc_role = r["role"]["id"].as_i64().unwrap();
    assert_eq!(r["role"]["permissions"], json!(["clients.read", "kyc.read", "kyc.review"]));
    assert_eq!(code(&mk(&admin, "kyc desk", &["kyc.read"]).await.unwrap_err()), "name_taken");

    // invite with that role → info → accept (password) → emailed code → session
    let inv = |tok: &str, email: &str, role: i64| {
        let (st, c) = (st.clone(), ctx(Some(tok), ip));
        let body = InviteReq { email: email.into(), name: "Kay Why".into(), role_id: Some(role) };
        async move { invite(State(st), c, Ok(Json(body))).await }
    };
    let owner_role = role_id(&st, t, "platform_owner").await;
    assert_eq!(code(&inv(&admin, "x@example.com", owner_role).await.unwrap_err()), "forbidden");
    let (status, Json(v)) = inv(&admin, "kyc.agent@example.com", kyc_role).await.unwrap();
    assert_eq!(status, StatusCode::CREATED);
    let new_id = v["staff"]["id"].as_i64().unwrap();
    let path = v["invite"]["dev_invite_path"].as_str().unwrap().to_string();
    let token = path.trim_start_matches("/invite/").to_string();
    assert_eq!(code(&inv(&admin, "kyc.agent@example.com", kyc_role).await.unwrap_err()), "already_invited");
    let Json(info) = invite_info(State(st.clone()), ctx(None, ip), Ok(Query(InviteLookup { token: Some(token.clone()) }))).await.unwrap();
    assert_eq!(info["email"], "kyc.agent@example.com");
    assert_eq!(info["role_label"], "KYC Desk");
    // an invited account can't sign in yet
    let login = |email: &str, pw: &str, ip: &str| {
        let (st, c) = (st.clone(), ctx(None, ip));
        let body = crate::client_auth::LoginReq { email: email.into(), password: pw.into() };
        async move { crate::staff_auth::login(State(st), c, Ok(Json(body))).await }
    };
    assert!(login("kyc.agent@example.com", "Correct-horse-9", ip).await.is_err());
    assert!(matches!(invite_accept(State(st.clone()), ctx(None, ip), Ok(Json(AcceptReq { token: token.clone(), password: "short".into() }))).await, Err(ApiError::Validation { .. })));
    let Json(ch) = invite_accept(State(st.clone()), ctx(None, ip), Ok(Json(AcceptReq { token: token.clone(), password: "Correct-horse-9".into() }))).await.unwrap();
    assert_eq!(ch["status"], "otp_required");
    assert_eq!(code(&invite_accept(State(st.clone()), ctx(None, ip), Ok(Json(AcceptReq { token, password: "Correct-horse-9".into() }))).await.unwrap_err()), "invite_expired");
    let Json(sess) = crate::staff_auth::verify_otp(
        State(st.clone()),
        ctx(None, ip),
        Ok(Json(crate::client_auth::VerifyReq { challenge: ch["challenge"].as_str().unwrap().into(), code: ch["dev_code"].as_str().unwrap().into() })),
    )
    .await
    .unwrap();
    let agent = sess["session"]["token"].as_str().unwrap().to_string();
    let v = me_json(&st, &agent, ip).await.unwrap();
    assert_eq!(v["staff"]["permissions"], json!(["clients.read", "kyc.read", "kyc.review"]));
    assert_eq!(v["staff"]["role_label"], "KYC Desk");
    assert_eq!(v["staff"]["role"], "viewer", "service role covers no service permissions");
    // the gateway enforces the custom role: KYC queue yes, audit log no
    assert!(crate::admin::require(&st, &ctx(Some(&agent), ip), crate::admin::Perm::KycReview).await.is_ok());
    assert!(crate::admin::require(&st, &ctx(Some(&agent), ip), crate::admin::Perm::AuditRead).await.is_err());

    // editing the role changes access on the next request (and is audited with a diff)
    let Json(_) = update_role(State(st.clone()), ctx(Some(&admin), ip), Path(kyc_role), Ok(Json(RoleReq { name: None, description: None, permissions: Some(vec!["kyc.read".into(), "audit.read".into()]) })))
        .await
        .unwrap();
    assert!(crate::admin::require(&st, &ctx(Some(&agent), ip), crate::admin::Perm::AuditRead).await.is_ok());
    assert!(crate::admin::require(&st, &ctx(Some(&agent), ip), crate::admin::Perm::KycReview).await.is_err());
    let meta: Value = sqlx::query_scalar("SELECT meta FROM audit_log WHERE action = 'role.updated' ORDER BY id DESC LIMIT 1").fetch_one(&st.pool).await.unwrap();
    assert_eq!(meta["added"], json!(["audit.read"]));
    assert_eq!(meta["removed"], json!(["clients.read", "kyc.review"]));

    // presets: edit → customised, reset → code defaults; system roles are locked
    let dealer = role_id(&st, t, "dealer").await;
    update_role(State(st.clone()), ctx(Some(&admin), ip), Path(dealer), Ok(Json(RoleReq { name: None, description: None, permissions: Some(vec!["dealing.read".into()]) }))).await.unwrap();
    assert_eq!(rbac::role_by_id(&st.pool, t, dealer).await.unwrap().unwrap().perms, vec!["dealing.read"]);
    reset_role(State(st.clone()), ctx(Some(&admin), ip), Path(dealer)).await.unwrap();
    assert!(rbac::role_by_id(&st.pool, t, dealer).await.unwrap().unwrap().perms.contains(&"dealing.write".to_string()));
    let sa = role_id(&st, t, "super_admin").await;
    assert_eq!(code(&update_role(State(st.clone()), ctx(Some(&owner), ip), Path(sa), Ok(Json(RoleReq { name: None, description: None, permissions: Some(vec![]) }))).await.unwrap_err()), "forbidden");
    assert_eq!(code(&delete_role(State(st.clone()), ctx(Some(&admin), ip), Path(kyc_role)).await.unwrap_err()), "role_in_use");

    // staff management guards
    let (sa_id, _) = staff(&st, t, "super@example.com", "super_admin").await;
    assert_eq!(code(&disable(State(st.clone()), ctx(Some(&admin), ip), Path(sa_id), Ok(Json(ReasonReq::default()))).await.unwrap_err()), "forbidden", "admin can't touch a super admin");
    assert_eq!(code(&disable(State(st.clone()), ctx(Some(&owner), ip), Path(owner_id), Ok(Json(ReasonReq::default()))).await.unwrap_err()), "bad_request", "not yourself");
    // role change + disable (revokes sessions) + enable
    update(State(st.clone()), ctx(Some(&admin), ip), Path(new_id), Ok(Json(UpdateReq { name: Some("Kay Agent".into()), role_id: Some(role_id(&st, t, "support").await) }))).await.unwrap();
    assert_eq!(me_json(&st, &agent, ip).await.unwrap()["staff"]["role_label"], "Support Agent");
    disable(State(st.clone()), ctx(Some(&admin), ip), Path(new_id), Ok(Json(ReasonReq { reason: Some("left".into()) }))).await.unwrap();
    assert!(me_json(&st, &agent, ip).await.is_err());
    assert!(matches!(login("kyc.agent@example.com", "Correct-horse-9", ip).await, Err(ApiError::AccountDisabled)));
    let Json(v) = enable(State(st.clone()), ctx(Some(&admin), ip), Path(new_id)).await.unwrap();
    assert_eq!(v["account_status"], "active");
    // 2FA reset forgets trusted devices; force sign-out revokes sessions
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM trusted_devices WHERE subject_kind = 'staff' AND subject_id = $1").bind(new_id).fetch_one(&st.pool).await.unwrap();
    assert_eq!(n, 1);
    let Json(v) = reset_2fa(State(st.clone()), ctx(Some(&admin), ip), Path(new_id)).await.unwrap();
    assert_eq!(v["trusted_devices_removed"], 1);
    let (_, s2) = (0, identity::create_session(&st, &ctx(None, ip), Kind::Staff, t, new_id).await.unwrap().token);
    let Json(v) = sign_out(State(st.clone()), ctx(Some(&admin), ip), Path(new_id)).await.unwrap();
    assert_eq!(v["sessions_revoked"], 1);
    assert!(me_json(&st, &s2, ip).await.is_err());
    let Json(d) = detail(State(st.clone()), ctx(Some(&admin), ip), Path(new_id)).await.unwrap();
    assert!(d["events"].as_array().unwrap().iter().any(|e| e["action"] == "staff.2fa_reset"));
    for a in ["staff.invited", "staff.invite_accepted", "staff.role_changed", "staff.disabled", "staff.enabled", "staff.signed_out", "role.created", "role.reset"] {
        assert!(audit_count(&st, a).await >= 1, "{a} audited");
    }
    db.drop_db().await;
}

#[tokio::test]
async fn ip_allowlist_blocks_and_bypasses() {
    let Some(db) = TestDb::new("ip allow-list").await else { return };
    let st = db.st.clone();
    let t = kalks(&st).await;
    let (office, home) = ("198.51.100.20", "192.0.2.77");
    let (_, owner) = staff(&st, t, "owner@example.com", "platform_owner").await;
    let (_, admin) = staff(&st, t, "admin@example.com", "admin").await;
    let (_, dealer) = staff(&st, t, "dealer@example.com", "dealer").await;

    let add = |tok: &str, cidr: &str, ip: &str| {
        let (st, c) = (st.clone(), ctx(Some(tok), ip));
        let body = IpReq { cidr: cidr.into(), label: "Office".into() };
        async move { ip_add(State(st), c, Ok(Json(body))).await }
    };
    let set = |tok: &str, enabled: bool, ip: &str| {
        let (st, c) = (st.clone(), ctx(Some(tok), ip));
        async move { ip_settings(State(st), c, Ok(Json(IpSettingsReq { enabled: Some(enabled), owner_bypass: None }))).await }
    };
    assert!(matches!(add(&dealer, "198.51.100.0/24", office).await, Err(ApiError::Forbidden)));
    assert_eq!(code(&set(&admin, true, office).await.unwrap_err()), "empty_list");
    assert!(matches!(add(&admin, "not-an-ip", office).await, Err(ApiError::Validation { .. })));
    let (_, Json(e)) = add(&admin, "203.0.113.5/24", office).await.unwrap();
    assert_eq!(e["cidr"], "203.0.113.0/24");
    // the admin's own IP isn't listed → refusing to lock them out
    assert_eq!(code(&set(&admin, true, office).await.unwrap_err()), "would_lock_out");
    let (_, Json(office_entry)) = add(&admin, "198.51.100.0/24", office).await.unwrap();
    set(&admin, true, office).await.unwrap();

    // every staff request from an unlisted IP is refused; the listed one works
    assert!(me_json(&st, &dealer, office).await.is_ok());
    assert_eq!(code(&me_json(&st, &dealer, home).await.unwrap_err()), "ip_not_allowed");
    assert_eq!(code(&crate::admin::require(&st, &ctx(Some(&admin), home), crate::admin::Perm::StatsRead).await.err().unwrap()), "ip_not_allowed");
    // owner bypass (default on)
    assert!(me_json(&st, &owner, home).await.is_ok());
    // sign-in from an unlisted IP is refused before the password is checked
    let login = |email: &str, pw: &str, ip: &str| {
        let (st, c) = (st.clone(), ctx(None, ip));
        let body = crate::client_auth::LoginReq { email: email.into(), password: pw.into() };
        async move { crate::staff_auth::login(State(st), c, Ok(Json(body))).await }
    };
    assert_eq!(code(&login("dealer@example.com", "wrong-password", home).await.unwrap_err()), "ip_not_allowed");
    assert!(login("dealer@example.com", "Correct-horse-9", office).await.is_ok());
    assert!(audit_count(&st, "security.ip_blocked").await >= 2);
    // can't remove the entry that keeps you in
    let oid = office_entry["id"].as_i64().unwrap();
    assert_eq!(code(&ip_remove(State(st.clone()), ctx(Some(&admin), office), Path(oid)).await.unwrap_err()), "would_lock_out");
    // listing shows the caller's IP status
    let Json(l) = ip_list(State(st.clone()), ctx(Some(&admin), office)).await.unwrap();
    assert_eq!(l["enabled"], true);
    assert_eq!(l["your_ip_allowed"], true);
    assert_eq!(l["items"].as_array().unwrap().len(), 2);
    // turning the owner bypass off is owner-only; the owner then needs a listed IP too
    assert!(matches!(ip_settings(State(st.clone()), ctx(Some(&admin), office), Ok(Json(IpSettingsReq { enabled: None, owner_bypass: Some(false) }))).await, Err(ApiError::Coded { .. })));
    ip_settings(State(st.clone()), ctx(Some(&owner), office), Ok(Json(IpSettingsReq { enabled: None, owner_bypass: Some(false) }))).await.unwrap();
    assert_eq!(code(&me_json(&st, &owner, home).await.unwrap_err()), "ip_not_allowed");
    set(&owner, false, office).await.unwrap();
    assert!(me_json(&st, &dealer, home).await.is_ok());
    db.drop_db().await;
}

#[tokio::test]
async fn maintenance_and_features() {
    let Some(db) = TestDb::new("maintenance").await else { return };
    let st = db.st.clone();
    let t = kalks(&st).await;
    let ip = "203.0.113.9";
    let (_, owner) = staff(&st, t, "owner@example.com", "platform_owner").await;
    let (_, admin) = staff(&st, t, "admin@example.com", "admin").await;
    let (_, support) = staff(&st, t, "support@example.com", "support").await;

    let Json(cfg) = tenancy::public_config(State(st.clone()), ctx(None, ip)).await.unwrap();
    assert_eq!(cfg["maintenance"]["active"], false);
    assert_eq!(cfg["modules"]["prop"], true);

    // maintenance: clients are held at the door, staff keep working
    assert!(matches!(tenancy::set_maintenance(State(st.clone()), ctx(Some(&support), ip), Ok(Json(serde_json::from_value(json!({"enabled": true})).unwrap()))).await, Err(ApiError::Forbidden)));
    tenancy::set_maintenance(State(st.clone()), ctx(Some(&admin), ip), Ok(Json(serde_json::from_value(json!({"enabled": true, "message": "Upgrading the matching engine"})).unwrap()))).await.unwrap();
    let Json(cfg) = tenancy::public_config(State(st.clone()), ctx(None, ip)).await.unwrap();
    assert_eq!(cfg["maintenance"]["active"], true);
    assert_eq!(cfg["maintenance"]["message"], "Upgrading the matching engine");
    let login = crate::client_auth::login(State(st.clone()), ctx(None, ip), Ok(Json(crate::client_auth::LoginReq { email: "c@example.com".into(), password: "whatever-123".into() }))).await;
    assert_eq!(code(&login.unwrap_err()), "maintenance");
    assert!(me_json(&st, &support, ip).await.is_ok());
    // an end time in the past ends the window
    sqlx::query("UPDATE tenants SET maintenance_until = now() - interval '1 minute' WHERE id = $1").bind(t).execute(&st.pool).await.unwrap();
    assert!(!tenancy::maintenance(&st.pool, t).await.unwrap().active);
    tenancy::set_maintenance(State(st.clone()), ctx(Some(&admin), ip), Ok(Json(serde_json::from_value(json!({"enabled": false})).unwrap()))).await.unwrap();
    assert!(audit_count(&st, "settings.maintenance_on").await == 1);

    // flags: tenant admins switch flags; modules are owner-only
    let flag = |tok: &str, key: &str, v: Option<bool>| {
        let (st, c) = (st.clone(), ctx(Some(tok), ip));
        let key = key.to_string();
        async move { tenancy::set_feature(State(st), c, Path(key), Ok(Json(serde_json::from_value(json!({"enabled": v})).unwrap()))).await }
    };
    flag(&admin, "trade_sharing", Some(false)).await.unwrap();
    assert_eq!(code(&tenancy::require_feature(&st, t, "trade_sharing").await.unwrap_err()), "feature_disabled");
    flag(&admin, "trade_sharing", None).await.unwrap();
    assert!(tenancy::require_feature(&st, t, "trade_sharing").await.is_ok());
    assert_eq!(code(&flag(&admin, "prop", Some(false)).await.unwrap_err()), "forbidden");
    flag(&owner, "prop", Some(false)).await.unwrap();
    let Json(cfg) = tenancy::public_config(State(st.clone()), ctx(None, ip)).await.unwrap();
    assert_eq!(cfg["modules"]["prop"], false);
    flag(&admin, "client_registration", Some(false)).await.unwrap();
    assert_eq!(code(&tenancy::require_feature(&st, t, "client_registration").await.unwrap_err()), "feature_disabled");
    db.drop_db().await;
}

#[tokio::test]
async fn module_switches_owner_only_with_reason() {
    let Some(db) = TestDb::new("modules").await else { return };
    let st = db.st.clone();
    let t = kalks(&st).await;
    let ip = "203.0.113.9";
    let (_, owner) = staff(&st, t, "owner@example.com", "platform_owner").await;
    let (_, admin) = staff(&st, t, "admin@example.com", "admin").await;

    // every module key, on by default (flags stay out of the module map)
    let Json(cfg) = tenancy::public_config(State(st.clone()), ctx(None, ip)).await.unwrap();
    for k in ["copy_trading", "pamm", "mam", "prop", "ib", "algo", "api", "academy", "wallet", "rewards", "options", "news", "calendar", "markets", "ai", "support_chat"] {
        assert_eq!(cfg["modules"][k], true, "{k} on by default");
    }
    assert_eq!(cfg["modules"].as_object().unwrap().len(), 16);
    assert!(cfg["flags"].get("options").is_none());

    // the owner switches modules of the Kalks tenant itself, with a reason that is audited with each switch
    let mut m = Map::new();
    m.insert("options".into(), json!(false));
    m.insert("support_chat".into(), json!(false));
    m.insert("reason".into(), json!("  Options launch postponed  "));
    let Json(v) = owner::set_tenant_features(State(st.clone()), ctx(Some(&owner), ip), Path(t), Ok(Json(m))).await.unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 2);
    let Json(cfg) = tenancy::public_config(State(st.clone()), ctx(None, ip)).await.unwrap();
    assert_eq!((cfg["modules"]["options"].as_bool(), cfg["modules"]["support_chat"].as_bool(), cfg["modules"]["news"].as_bool()), (Some(false), Some(false), Some(true)));
    let reasons: Vec<Option<String>> = sqlx::query_scalar("SELECT meta->>'reason' FROM audit_log WHERE action = 'settings.module_toggled' ORDER BY id").fetch_all(&st.pool).await.unwrap();
    assert_eq!(reasons, vec![Some("Options launch postponed".to_string()); 2]);
    // a reason must be text; an unknown key is still refused
    let mut bad = Map::new();
    bad.insert("reason".into(), json!(5));
    assert_eq!(code(&owner::set_tenant_features(State(st.clone()), ctx(Some(&owner), ip), Path(t), Ok(Json(bad))).await.unwrap_err()), "validation");
    let mut bad = Map::new();
    bad.insert("no_such_module".into(), json!(false));
    assert_eq!(code(&owner::set_tenant_features(State(st.clone()), ctx(Some(&owner), ip), Path(t), Ok(Json(bad))).await.unwrap_err()), "validation");

    // a tenant admin can't switch a module, by either route
    let mut m = Map::new();
    m.insert("options".into(), json!(true));
    assert!(matches!(owner::set_tenant_features(State(st.clone()), ctx(Some(&admin), ip), Path(t), Ok(Json(m))).await, Err(ApiError::Forbidden)));
    let r = tenancy::set_feature(State(st.clone()), ctx(Some(&admin), ip), Path("options".into()), Ok(Json(serde_json::from_value(json!({"enabled": true, "reason": "x"})).unwrap()))).await;
    assert_eq!(code(&r.unwrap_err()), "forbidden");
    assert!(!tenancy::enabled(&st.pool, t, "options").await.unwrap());

    // the staff's /me carries the tenant's modules (the Back Office hides off modules from its nav)
    let me = me_json(&st, &admin, ip).await.unwrap();
    assert_eq!(me["staff"]["tenant"]["modules"]["options"], false);
    assert_eq!(me["staff"]["tenant"]["modules"]["prop"], true);

    // back to the platform default (null)
    let mut m = Map::new();
    m.insert("options".into(), Value::Null);
    owner::set_tenant_features(State(st.clone()), ctx(Some(&owner), ip), Path(t), Ok(Json(m))).await.unwrap();
    assert!(tenancy::enabled(&st.pool, t, "options").await.unwrap());

    // MAM was split out of copy trading: when the key first appears, a broker with copy trading off keeps MAM off
    sqlx::query("INSERT INTO tenant_features (tenant_id, key, enabled) VALUES ($1, 'copy_trading', false)").bind(t).execute(&st.pool).await.unwrap();
    sqlx::query("DELETE FROM feature_flags WHERE key = 'mam'").execute(&st.pool).await.unwrap();
    tenancy::seed_catalogue(&st.pool).await.unwrap();
    assert!(!tenancy::enabled(&st.pool, t, "mam").await.unwrap());
    // later starts leave the owner's choice alone
    owner::set_tenant_features(State(st.clone()), ctx(Some(&owner), ip), Path(t), Ok(Json([("mam".to_string(), json!(true))].into_iter().collect()))).await.unwrap();
    tenancy::seed_catalogue(&st.pool).await.unwrap();
    assert!(tenancy::enabled(&st.pool, t, "mam").await.unwrap());
    db.drop_db().await;
}

#[tokio::test]
async fn owner_tenants_billing_and_dashboard() {
    let Some(db) = TestDb::new("owner").await else { return };
    let st = db.st.clone();
    let t = kalks(&st).await;
    let ip = "203.0.113.9";
    let (_, owner) = staff(&st, t, "owner@example.com", "platform_owner").await;
    let (_, admin) = staff(&st, t, "admin@example.com", "admin").await;

    let body: owner::CreateTenantReq = serde_json::from_value(json!({
        "slug": "acme-fx", "name": "Acme FX", "domains": ["acmefx.com", "app.acmefx.com"], "brand": {"primary": "#22c55e", "accent": "#e9b949"},
        "plan": "growth", "limits": {"max_clients": 5000, "max_staff": 2}, "modules": {"prop": false, "algo": false},
        "billing": {"setup_fee": 5000, "monthly_licence": 2500, "revenue_share_pct": 12.5, "billing_email": "billing@acmefx.com"},
        "admin": {"email": "boss@acmefx.com", "name": "Acme Boss"}
    }))
    .unwrap();
    assert!(matches!(owner::create_tenant(State(st.clone()), ctx(Some(&admin), ip), Ok(Json(serde_json::from_value(json!({"slug": "x-y-z", "name": "X"})).unwrap()))).await, Err(ApiError::Forbidden)));
    let (s, Json(v)) = owner::create_tenant(State(st.clone()), ctx(Some(&owner), ip), Ok(Json(body))).await.unwrap();
    assert_eq!(s, StatusCode::CREATED);
    let acme = v["tenant"]["id"].as_i64().unwrap();
    assert!(v["admin_invite"]["dev_invite_path"].as_str().unwrap().starts_with("/invite/"));
    // roles seeded (no owner role), modules switched, duplicate slug / domain refused
    let keys: Vec<String> = sqlx::query_scalar("SELECT key FROM roles WHERE tenant_id = $1").bind(acme).fetch_all(&st.pool).await.unwrap();
    assert!(keys.contains(&"super_admin".to_string()) && !keys.contains(&"platform_owner".to_string()));
    assert!(!tenancy::enabled(&st.pool, acme, "prop").await.unwrap());
    assert!(tenancy::enabled(&st.pool, acme, "copy_trading").await.unwrap());
    let dup: owner::CreateTenantReq = serde_json::from_value(json!({"slug": "acme-fx", "name": "Dup"})).unwrap();
    assert_eq!(code(&owner::create_tenant(State(st.clone()), ctx(Some(&owner), ip), Ok(Json(dup))).await.unwrap_err()), "slug_taken");
    let dup: owner::CreateTenantReq = serde_json::from_value(json!({"slug": "other-fx", "name": "Other", "domains": ["APP.acmefx.com"]})).unwrap();
    assert_eq!(code(&owner::create_tenant(State(st.clone()), ctx(Some(&owner), ip), Ok(Json(dup))).await.unwrap_err()), "domain_taken");

    // the invited Super Admin of the new tenant accepts and gets that tenant's full access (no owner.*)
    let token = v["admin_invite"]["dev_invite_path"].as_str().unwrap().trim_start_matches("/invite/").to_string();
    let Json(ch) = invite_accept(State(st.clone()), ctx(None, ip), Ok(Json(AcceptReq { token, password: "Correct-horse-9".into() }))).await.unwrap();
    let Json(sess) = crate::staff_auth::verify_otp(State(st.clone()), ctx(None, ip), Ok(Json(crate::client_auth::VerifyReq { challenge: ch["challenge"].as_str().unwrap().into(), code: ch["dev_code"].as_str().unwrap().into() }))).await.unwrap();
    let boss = sess["session"]["token"].as_str().unwrap().to_string();
    let me = me_json(&st, &boss, ip).await.unwrap();
    assert_eq!(me["staff"]["tenant"]["slug"], "acme-fx");
    assert_eq!(me["staff"]["is_owner"], false);
    assert!(matches!(owner::tenants(State(st.clone()), ctx(Some(&boss), ip)).await, Err(ApiError::Forbidden)));
    // staff limit (2) applies to the tenant's invites
    let sup = role_id(&st, acme, "support").await;
    invite(State(st.clone()), ctx(Some(&boss), ip), Ok(Json(InviteReq { email: "a1@acmefx.com".into(), name: "Agent One".into(), role_id: Some(sup) }))).await.unwrap();
    let err = invite(State(st.clone()), ctx(Some(&boss), ip), Ok(Json(InviteReq { email: "a2@acmefx.com".into(), name: "Agent Two".into(), role_id: Some(sup) }))).await.unwrap_err();
    assert_eq!(code(&err), "limit_reached");

    // list + detail + update
    let Json(l) = owner::tenants(State(st.clone()), ctx(Some(&owner), ip)).await.unwrap();
    assert_eq!(l["total"], 2);
    let Json(d) = owner::tenant_detail(State(st.clone()), ctx(Some(&owner), ip), Path(acme)).await.unwrap();
    assert_eq!(d["billing"]["revenue_share_bps"], 1250);
    assert_eq!(d["admins"][0]["email"], "boss@acmefx.com");
    owner::update_tenant(State(st.clone()), ctx(Some(&owner), ip), Path(acme), Ok(Json(serde_json::from_value(json!({"name": "Acme Markets", "limits": {"max_staff": 10}})).unwrap()))).await.unwrap();

    // suspension ends every session of the tenant and closes sign-in; the owner's own tenant can't be suspended
    assert!(matches!(owner::suspend_tenant(State(st.clone()), ctx(Some(&owner), ip), Path(acme), Ok(Json(owner::SuspendReq::default()))).await, Err(ApiError::Validation { .. })));
    assert!(owner::suspend_tenant(State(st.clone()), ctx(Some(&owner), ip), Path(t), Ok(Json(serde_json::from_value(json!({"reason": "test"})).unwrap()))).await.is_err());
    owner::suspend_tenant(State(st.clone()), ctx(Some(&owner), ip), Path(acme), Ok(Json(serde_json::from_value(json!({"reason": "Unpaid invoices"})).unwrap()))).await.unwrap();
    assert!(me_json(&st, &boss, ip).await.is_err());
    owner::activate_tenant(State(st.clone()), ctx(Some(&owner), ip), Path(acme)).await.unwrap();

    // owner switches a module for the tenant
    let mut m = Map::new();
    m.insert("prop".into(), json!(true));
    m.insert("pamm".into(), json!(false));
    owner::set_tenant_features(State(st.clone()), ctx(Some(&owner), ip), Path(acme), Ok(Json(m))).await.unwrap();
    assert!(tenancy::enabled(&st.pool, acme, "prop").await.unwrap() && !tenancy::enabled(&st.pool, acme, "pamm").await.unwrap());

    // invoices: setup + 1 month licence + 12.5% of 10,000 → 5,000 + 2,500 + 1,250
    let inv: owner::CreateInvoiceReq = serde_json::from_value(json!({"tenant_id": acme, "period_start": "2026-09-01", "period_end": "2026-09-30", "include_setup_fee": true, "revenue_base": 10000, "issue": true})).unwrap();
    let (_, Json(i)) = owner::create_invoice(State(st.clone()), ctx(Some(&owner), ip), Ok(Json(inv))).await.unwrap();
    assert_eq!(i["invoice"]["total_cents"], 875_000);
    assert_eq!(i["invoice"]["status"], "issued");
    let iid = i["invoice"]["id"].as_i64().unwrap();
    let st_req = |s: &str| Ok(Json(serde_json::from_value::<owner::InvoiceStatusReq>(json!({"status": s})).unwrap()));
    assert!(owner::invoice_status(State(st.clone()), ctx(Some(&owner), ip), Path(iid), st_req("draft")).await.is_err());
    owner::invoice_status(State(st.clone()), ctx(Some(&owner), ip), Path(iid), st_req("paid")).await.unwrap();
    assert!(owner::invoice_status(State(st.clone()), ctx(Some(&owner), ip), Path(iid), st_req("void")).await.is_err());

    // cross-tenant dashboard
    let Json(dash) = owner::dashboard(State(st.clone()), ctx(Some(&owner), ip)).await.unwrap();
    assert_eq!(dash["totals"]["tenants"], 2);
    assert_eq!(dash["totals"]["mrr_cents"], 250_000);
    assert_eq!(dash["totals"]["paid_30d_cents"], 875_000);
    assert!(matches!(owner::dashboard(State(st.clone()), ctx(Some(&admin), ip)).await, Err(ApiError::Forbidden)));
    for a in ["owner.tenant_created", "owner.tenant_suspended", "owner.tenant_activated", "owner.invoice_created", "owner.invoice_status", "settings.module_toggled"] {
        assert!(audit_count(&st, a).await >= 1, "{a} audited");
    }
    db.drop_db().await;
}
