//! Unified Back Office RBAC (D108 / D109): the permission catalogue, preset roles, role resolution and the
//! staff IP allow-list matcher.
//!
//! The gateway is the single source of truth for staff permissions. `/v1/admin/auth/me` returns the full list
//! of the staff member's permission keys plus `rbac: true`; every app BFF checks those keys, and the local
//! role maps in the apps (lib/*-perms.ts) are only a fallback for gateways that predate this module.
//!
//! Hierarchy: Platform Owner (all tenants, `owner.*`) → Tenant Super Admin (everything in the tenant) → staff
//! with preset or custom roles. Downstream services (trading, wallet, IB, prop, academy, algo) still check a
//! role name from `x-kalks-staff-role`; the gateway maps each role to the smallest built-in role that covers its
//! service permissions (`service_role`), and the BFF enforces the exact permission first.

use std::collections::BTreeSet;
use std::net::IpAddr;

use sqlx::{PgPool, Row};

/// One permission key and where it sits in the role-builder matrix.
pub struct PermDef {
    pub key: &'static str,
    pub module: &'static str,
    /// Matrix column: view | create | edit | approve | export.
    pub action: &'static str,
    pub label: &'static str,
}

pub struct ModuleDef {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub const MODULES: &[ModuleDef] = &[
    ModuleDef { key: "dashboard", label: "Command Center", description: "Overview, KPIs and system health" },
    ModuleDef { key: "clients", label: "Clients", description: "Client list, profiles, sessions" },
    ModuleDef { key: "kyc", label: "KYC & compliance", description: "Verification queue and decisions" },
    ModuleDef { key: "dealing", label: "Dealing desk", description: "Positions, orders, routing, dealer trades" },
    ModuleDef { key: "accounts", label: "Trading accounts", description: "Accounts, leverage, groups" },
    ModuleDef { key: "config", label: "Groups & spreads", description: "Account groups and spread markups" },
    ModuleDef { key: "finance", label: "Finance & wallets", description: "Deposits, withdrawals, wallets, balance operations" },
    ModuleDef { key: "partners", label: "IB programme", description: "Partners, plans, commissions, payout batches" },
    ModuleDef { key: "social", label: "Copy trading & PAMM", description: "Masters, funds, applications, fee payouts" },
    ModuleDef { key: "prop", label: "Prop firm", description: "Plans, challenges, funded traders, payouts" },
    ModuleDef { key: "algo", label: "Algo & API", description: "Strategies, deployments, API keys, marketplace" },
    ModuleDef { key: "options", label: "FX Options", description: "Options risk desk, series, vol surfaces, dealer controls, settlements" },
    ModuleDef { key: "content", label: "Content & Academy", description: "Academy, news, legal, templates" },
    ModuleDef { key: "marketing", label: "Marketing & rewards", description: "Bonuses, contests, promo codes, notifications" },
    ModuleDef { key: "support", label: "Support desk", description: "Tickets, canned replies, knowledge base" },
    ModuleDef { key: "reports", label: "Analytics & reports", description: "Broker P&L, funnels, regulatory reports" },
    ModuleDef { key: "audit", label: "Audit & sessions", description: "Audit log, live sessions" },
    ModuleDef { key: "security", label: "Security", description: "IP allow-list for the Back Office" },
    ModuleDef { key: "staff", label: "Staff & roles", description: "Invite and manage staff, build roles" },
    ModuleDef { key: "settings", label: "Tenant settings", description: "Maintenance mode, feature flags" },
    ModuleDef { key: "owner", label: "Platform owner", description: "Tenants, billing, system operations (Platform Owner only)" },
];

pub const PERMS: &[PermDef] = &[
    PermDef { key: "stats.read", module: "dashboard", action: "view", label: "View dashboard" },
    PermDef { key: "clients.read", module: "clients", action: "view", label: "View clients" },
    PermDef { key: "clients.write", module: "clients", action: "edit", label: "Edit and block clients" },
    PermDef { key: "clients.export", module: "clients", action: "export", label: "Export clients" },
    PermDef { key: "clients.restrict", module: "clients", action: "edit", label: "Restrict clients (trading, funding, transfers, IB, copy)" },
    PermDef { key: "clients.block", module: "clients", action: "approve", label: "Block and unblock client sign-in" },
    PermDef { key: "clients.impersonate", module: "clients", action: "create", label: "Open the Client Area as the client (read-only)" },
    PermDef { key: "clients.impersonate_full", module: "clients", action: "approve", label: "Full-access staff sessions as the client (Super Admin only)" },
    PermDef { key: "kyc.read", module: "kyc", action: "view", label: "View KYC cases and documents" },
    PermDef { key: "kyc.review", module: "kyc", action: "approve", label: "Approve / reject KYC" },
    PermDef { key: "dealing.read", module: "dealing", action: "view", label: "View positions, orders, routing" },
    PermDef { key: "dealing.write", module: "dealing", action: "edit", label: "Dealer trades and controls" },
    PermDef { key: "dealing.policy", module: "dealing", action: "approve", label: "Tenant dealing policy" },
    PermDef { key: "accounts.read", module: "accounts", action: "view", label: "View trading accounts" },
    PermDef { key: "accounts.write", module: "accounts", action: "edit", label: "Status, group, leverage" },
    PermDef { key: "accounts.close", module: "accounts", action: "create", label: "Request permanent closure of trading accounts" },
    PermDef { key: "accounts.close.approve", module: "accounts", action: "approve", label: "Approve or reject account closures (closure queue)" },
    PermDef { key: "spreads.read", module: "config", action: "view", label: "View groups and spreads" },
    PermDef { key: "groups.write", module: "config", action: "create", label: "Create / edit account groups" },
    PermDef { key: "spreads.write", module: "config", action: "edit", label: "Edit spread markups" },
    PermDef { key: "finance.read", module: "finance", action: "view", label: "View deposits, withdrawals, wallets" },
    PermDef { key: "finance.write", module: "finance", action: "create", label: "Process deposits, adjust wallets, mark paid" },
    PermDef { key: "finance.adjust", module: "finance", action: "edit", label: "Add / deduct funds on wallets and trading accounts" },
    PermDef { key: "finance.credit", module: "finance", action: "edit", label: "Give / take credit on trading accounts" },
    PermDef { key: "finance.settings", module: "finance", action: "edit", label: "Wallet settings (addresses, limits, fees)" },
    PermDef { key: "finance.approve", module: "finance", action: "approve", label: "Approve / reject withdrawals" },
    PermDef { key: "finance.adjust_approve", module: "finance", action: "approve", label: "Approve balance adjustments above the 4-eyes threshold" },
    PermDef { key: "finance.adjust_force", module: "finance", action: "approve", label: "Force a deduction beyond free margin (Super Admin)" },
    PermDef { key: "finance.export", module: "finance", action: "export", label: "Export finance data" },
    PermDef { key: "partners.read", module: "partners", action: "view", label: "View IB programme" },
    PermDef { key: "partners.write", module: "partners", action: "edit", label: "Plans, levels, partner changes, batches" },
    PermDef { key: "partners.approve", module: "partners", action: "approve", label: "Approve payout batches" },
    PermDef { key: "partners.export", module: "partners", action: "export", label: "Export partner data" },
    PermDef { key: "social.read", module: "social", action: "view", label: "View masters, funds, fees" },
    PermDef { key: "social.write", module: "social", action: "edit", label: "Suspend, freeze, settings" },
    PermDef { key: "social.approve", module: "social", action: "approve", label: "Applications and fee payouts" },
    PermDef { key: "prop.read", module: "prop", action: "view", label: "View prop firm" },
    PermDef { key: "prop.write", module: "prop", action: "edit", label: "Plans, pass / fail, news, certificates" },
    PermDef { key: "prop.approve", module: "prop", action: "approve", label: "Approve prop payouts" },
    PermDef { key: "algo.read", module: "algo", action: "view", label: "View strategies, deployments, keys" },
    PermDef { key: "algo.write", module: "algo", action: "edit", label: "Kill switches, moderation, revoke keys" },
    PermDef { key: "algo.settings", module: "algo", action: "approve", label: "Platform kill switch and ALGO settings" },
    PermDef { key: "options.read", module: "options", action: "view", label: "View the options risk desk, series, surfaces and settlements" },
    PermDef { key: "options.config", module: "options", action: "edit", label: "Underlyings, vol surfaces, rates, holidays, spreads / fees and broker switches" },
    PermDef { key: "options.dealing", module: "options", action: "create", label: "Halt / close-only / freeze / manual vol, client limits, void option trades" },
    PermDef { key: "options.settle", module: "options", action: "approve", label: "Re-fix expiries and re-run option settlements (within 1 hour)" },
    PermDef { key: "content.read", module: "content", action: "view", label: "View content" },
    PermDef { key: "content.write", module: "content", action: "edit", label: "Edit and publish content" },
    PermDef { key: "marketing.read", module: "marketing", action: "view", label: "View campaigns and rewards" },
    PermDef { key: "marketing.write", module: "marketing", action: "edit", label: "Create and edit campaigns, bonuses" },
    PermDef { key: "marketing.approve", module: "marketing", action: "approve", label: "Approve bonuses and rewards" },
    PermDef { key: "notifications.write", module: "marketing", action: "create", label: "Send client notifications" },
    PermDef { key: "support.read", module: "support", action: "view", label: "View tickets" },
    PermDef { key: "support.write", module: "support", action: "edit", label: "Reply, assign, close tickets" },
    PermDef { key: "reports.read", module: "reports", action: "view", label: "View reports" },
    PermDef { key: "reports.export", module: "reports", action: "export", label: "Export reports" },
    PermDef { key: "audit.read", module: "audit", action: "view", label: "View audit log" },
    PermDef { key: "audit.export", module: "audit", action: "export", label: "Export audit log" },
    PermDef { key: "sessions.read", module: "audit", action: "view", label: "View live sessions" },
    PermDef { key: "sessions.revoke", module: "audit", action: "edit", label: "Revoke sessions" },
    PermDef { key: "security.read", module: "security", action: "view", label: "View IP allow-list" },
    PermDef { key: "security.write", module: "security", action: "edit", label: "Edit IP allow-list" },
    PermDef { key: "staff.read", module: "staff", action: "view", label: "View staff and roles" },
    PermDef { key: "staff.write", module: "staff", action: "create", label: "Invite, edit, disable staff" },
    PermDef { key: "staff.roles", module: "staff", action: "edit", label: "Create and edit roles" },
    PermDef { key: "settings.read", module: "settings", action: "view", label: "View tenant settings" },
    PermDef { key: "settings.write", module: "settings", action: "edit", label: "Maintenance mode and feature flags" },
    PermDef { key: "owner.tenants", module: "owner", action: "edit", label: "Create, edit and suspend tenants" },
    PermDef { key: "owner.billing", module: "owner", action: "approve", label: "Tenant billing and invoices" },
    PermDef { key: "owner.system", module: "owner", action: "view", label: "Cross-tenant dashboard and system ops" },
];

pub fn perm(key: &str) -> Option<&'static PermDef> {
    PERMS.iter().find(|p| p.key == key)
}

pub fn is_owner_perm(key: &str) -> bool {
    key.starts_with("owner.")
}

/// Every tenant-level permission (everything except `owner.*`).
pub fn tenant_perms() -> Vec<&'static str> {
    PERMS.iter().map(|p| p.key).filter(|k| !is_owner_perm(k)).collect()
}

pub fn all_perms() -> Vec<&'static str> {
    PERMS.iter().map(|p| p.key).collect()
}

/// The module's "view" key that any other permission of the module implies.
fn view_key(module: &str) -> Option<&'static str> {
    match module {
        // audit has two view keys; sessions.revoke implies sessions.read, audit.export implies audit.read
        "audit" => None,
        _ => PERMS.iter().find(|p| p.module == module && p.action == "view").map(|p| p.key),
    }
}

/// Validates keys, adds implied view permissions, sorts and de-duplicates.
pub fn normalize<S: AsRef<str>>(keys: &[S]) -> Result<Vec<String>, &'static str> {
    let mut out = BTreeSet::new();
    for k in keys {
        let k = k.as_ref().trim();
        let def = perm(k).ok_or("Unknown permission.")?;
        out.insert(def.key);
        if def.action != "view" {
            match k {
                "sessions.revoke" => {
                    out.insert("sessions.read");
                }
                "audit.export" => {
                    out.insert("audit.read");
                }
                _ => {
                    if let Some(v) = view_key(def.module) {
                        out.insert(v);
                    }
                }
            }
        }
    }
    Ok(out.into_iter().map(str::to_string).collect())
}

// ---------- built-in roles ----------

pub struct RoleDef {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub kind: &'static str,
}

/// System roles (not editable) and presets (editable per tenant, resettable), in display order.
pub const BUILTIN_ROLES: &[RoleDef] = &[
    RoleDef { key: "platform_owner", name: "Platform Owner", description: "Everything, across all brokers: tenants, billing, system operations.", kind: "system" },
    RoleDef { key: "super_admin", name: "Super Admin", description: "Everything in this broker, including staff, roles and security.", kind: "system" },
    RoleDef { key: "admin", name: "Administrator", description: "Day-to-day administration of every module.", kind: "preset" },
    RoleDef { key: "dealer", name: "Dealer", description: "Dealing desk, trading accounts and spreads.", kind: "preset" },
    RoleDef { key: "risk_manager", name: "Risk Manager", description: "Exposure, dealing controls, copy / PAMM and prop risk.", kind: "preset" },
    RoleDef { key: "finance", name: "Finance", description: "Deposits, withdrawals, balance operations and payouts.", kind: "preset" },
    RoleDef { key: "compliance", name: "Compliance / KYC", description: "KYC decisions, audit log, sessions, master applications.", kind: "preset" },
    RoleDef { key: "support", name: "Support Agent", description: "Client lookup and support tickets.", kind: "preset" },
    RoleDef { key: "sales", name: "Sales", description: "Clients, leads, partners and campaigns (read-mostly).", kind: "preset" },
    RoleDef { key: "partner_manager", name: "IB Manager", description: "IB programme: partners, plans, levels, batches.", kind: "preset" },
    RoleDef { key: "marketing", name: "Marketing", description: "Content, Academy, campaigns and notifications.", kind: "preset" },
    RoleDef { key: "viewer", name: "Viewer", description: "Read-only access to trading and programme pages.", kind: "preset" },
    RoleDef { key: "options_risk", name: "Options Risk", description: "FX Options risk desk: Greeks, vol surfaces, spreads and fees, dealer controls and settlements.", kind: "preset" },
];

pub fn builtin(key: &str) -> Option<&'static RoleDef> {
    BUILTIN_ROLES.iter().find(|r| r.key == key)
}

/// Default permissions of a built-in role. These reproduce the role maps the apps and services used before the
/// gateway owned RBAC (apps/admin/lib/*-perms.ts, services/*: ROLES_*), so existing staff keep their access.
pub fn preset_perms(key: &str) -> Option<Vec<&'static str>> {
    let v: Vec<&'static str> = match key {
        "platform_owner" => all_perms(),
        "super_admin" => tenant_perms(),
        // forcing a deduction past the free margin is a Super Admin power only
        "admin" => tenant_perms().into_iter().filter(|k| *k != "finance.adjust_force").collect(),
        "dealer" => vec![
            "stats.read", "clients.read", "spreads.read", "spreads.write", "dealing.read", "dealing.write", "accounts.read", "accounts.write",
            "social.read", "prop.read", "prop.write", "algo.read", "algo.write", "options.read", "options.dealing",
        ],
        "risk_manager" => vec![
            "stats.read", "clients.read", "spreads.read", "spreads.write", "dealing.read", "dealing.write", "accounts.read", "accounts.write",
            "finance.read", "partners.read", "social.read", "social.write", "prop.read", "prop.write", "prop.approve", "algo.read", "algo.write",
            "marketing.read", "reports.read",
        ],
        "finance" => vec![
            "stats.read", "clients.read", "dealing.read", "accounts.read", "finance.read", "finance.write", "finance.adjust", "finance.credit",
            "finance.approve", "finance.adjust_approve", "finance.export", "partners.read", "partners.approve", "social.read", "prop.read", "prop.approve", "algo.read", "content.read",
            "marketing.read", "marketing.approve", "reports.read", "reports.export",
        ],
        "compliance" => vec![
            "stats.read", "clients.read", "clients.export", "clients.restrict", "clients.block", "clients.impersonate", "kyc.read", "kyc.review", "audit.read", "audit.export", "sessions.read", "spreads.read",
            "dealing.read", "accounts.read", "accounts.close", "accounts.close.approve", "finance.read", "partners.read", "social.read", "social.approve", "prop.read", "algo.read",
            "content.read", "marketing.read", "support.read", "reports.read",
        ],
        "support" => vec![
            "stats.read", "clients.read", "dealing.read", "accounts.read", "partners.read", "social.read", "prop.read", "algo.read", "content.read",
            "marketing.read", "support.read", "support.write",
        ],
        "sales" => vec!["stats.read", "clients.read", "partners.read", "content.read", "marketing.read", "support.read", "reports.read"],
        "partner_manager" => vec!["stats.read", "clients.read", "partners.read", "partners.write", "partners.export", "content.read", "marketing.read", "reports.read"],
        "marketing" => vec!["stats.read", "content.read", "content.write", "marketing.read", "marketing.write", "notifications.write", "reports.read"],
        "viewer" => vec!["stats.read", "spreads.read", "dealing.read", "accounts.read", "partners.read", "social.read", "prop.read", "algo.read", "content.read", "marketing.read"],
        "options_risk" => vec![
            "stats.read", "clients.read", "dealing.read", "accounts.read", "options.read", "options.config", "options.dealing", "options.settle",
        ],
        _ => return None,
    };
    Some(v)
}

/// Permissions only the system roles (Super Admin, Platform Owner) hold: a preset (edited or not) or a custom role
/// never gets them, whatever is stored.
pub const SUPER_ADMIN_ONLY: &[&str] = &["clients.impersonate_full"];

/// Effective permissions of a stored role row.
pub fn effective(kind: &str, key: &str, customised: bool, stored: &[String]) -> Vec<String> {
    let from_code = |k: &str| preset_perms(k).map(|v| v.into_iter().map(str::to_string).collect::<Vec<_>>());
    let system_only = |k: &String| SUPER_ADMIN_ONLY.contains(&k.as_str());
    match kind {
        "system" => from_code(key).unwrap_or_default(),
        "preset" if !customised => from_code(key).unwrap_or_else(|| stored.to_vec()).into_iter().filter(|k| !system_only(k)).collect(),
        _ => stored.iter().filter(|k| perm(k).is_some() && !is_owner_perm(k) && !system_only(k)).cloned().collect(),
    }
}

/// Permissions that downstream services check by role name.
fn service_relevant(k: &str) -> bool {
    // options.* reach the trading engine (option book, void, settlement re-run)
    ["dealing.", "accounts.", "finance.", "groups.", "partners.", "social.", "prop.", "algo.", "content.", "spreads.", "marketing.", "options."].iter().any(|p| k.starts_with(p))
}

/// Built-in roles the downstream services don't know by name: they are always mapped to a covering legacy role.
const NOT_DOWNSTREAM: &[&str] = &["sales", "options_risk"];

/// Built-in role sent to downstream services as `x-kalks-staff-role`: the least-privileged legacy role whose
/// service permissions cover this role's; `admin` when none does. The BFF has already enforced the exact key.
pub fn service_role(key: &str, perms: &[String]) -> &'static str {
    if let Some(b) = builtin(key)
        && !NOT_DOWNSTREAM.contains(&key)
        && perms.iter().filter(|p| service_relevant(p)).all(|p| preset_perms(key).unwrap_or_default().contains(&p.as_str()))
    {
        return b.key;
    }
    for cand in ["viewer", "support", "marketing", "partner_manager", "compliance", "finance", "dealer", "risk_manager"] {
        let have = preset_perms(cand).unwrap_or_default();
        if perms.iter().filter(|p| service_relevant(p)).all(|p| have.contains(&p.as_str())) {
            return cand;
        }
    }
    if perms.iter().any(|p| is_owner_perm(p)) { "platform_owner" } else { "admin" }
}

/// Creates missing built-in roles for every tenant and links staff rows without a role_id to the role named by
/// `staff.role`. Idempotent; runs on every start (and in tests via `db::connect`).
pub async fn ensure_roles(pool: &PgPool) -> anyhow::Result<()> {
    let tenants: Vec<i64> = sqlx::query_scalar("SELECT id FROM tenants").fetch_all(pool).await?;
    for t in tenants {
        seed_tenant_roles(pool, t).await?;
    }
    sqlx::query(
        "UPDATE staff s SET role_id = r.id FROM roles r
         WHERE s.role_id IS NULL AND r.tenant_id = s.tenant_id AND r.key = s.role",
    )
    .execute(pool)
    .await?;
    // anything left (unknown legacy role) becomes a viewer
    sqlx::query(
        "UPDATE staff s SET role_id = r.id, role = 'viewer' FROM roles r
         WHERE s.role_id IS NULL AND r.tenant_id = s.tenant_id AND r.key = 'viewer'",
    )
    .execute(pool)
    .await?;
    crate::tenancy::seed_catalogue(pool).await?;
    Ok(())
}

pub async fn seed_tenant_roles(pool: &PgPool, tenant_id: i64) -> anyhow::Result<()> {
    for r in BUILTIN_ROLES {
        // the platform owner role only exists in the owner's own tenant (#1)
        if r.key == "platform_owner" && tenant_id != owner_tenant_id(pool).await? {
            continue;
        }
        sqlx::query(
            "INSERT INTO roles (tenant_id, key, name, description, kind) VALUES ($1,$2,$3,$4,$5)
             ON CONFLICT DO NOTHING",
        )
        .bind(tenant_id)
        .bind(r.key)
        .bind(r.name)
        .bind(r.description)
        .bind(r.kind)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// The Platform Owner's tenant (the first one, `kalks`).
pub async fn owner_tenant_id(pool: &PgPool) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar("SELECT COALESCE((SELECT id FROM tenants WHERE slug = 'kalks'), (SELECT min(id) FROM tenants))").fetch_one(pool).await?)
}

/// Role row → (key, name, kind, effective permissions).
pub struct RoleInfo {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub kind: String,
    pub perms: Vec<String>,
}

pub async fn role_by_id(pool: &PgPool, tenant_id: i64, id: i64) -> anyhow::Result<Option<RoleInfo>> {
    let r = sqlx::query("SELECT id, key, name, kind, customised, permissions FROM roles WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await?;
    Ok(r.map(|r| {
        let kind: String = r.get("kind");
        let key: String = r.get("key");
        let perms = effective(&kind, &key, r.get("customised"), &r.get::<Vec<String>, _>("permissions"));
        RoleInfo { id: r.get("id"), key, name: r.get("name"), kind, perms }
    }))
}

// ---------- IP allow-list ----------

/// Parses `a.b.c.d`, `a.b.c.d/n`, IPv6 and IPv6/n. Returns the canonical text (network address) and parts.
pub fn parse_cidr(raw: &str) -> Result<(String, IpAddr, u8), &'static str> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 64 {
        return Err("Enter an IP address or CIDR range.");
    }
    let (ip_s, bits_s) = match raw.split_once('/') {
        Some((a, b)) => (a, Some(b)),
        None => (raw, None),
    };
    let ip: IpAddr = ip_s.parse().map_err(|_| "Enter a valid IPv4 or IPv6 address, e.g. 203.0.113.0/24.")?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    let bits: u8 = match bits_s {
        Some(b) => b.parse().map_err(|_| "The prefix length must be a number.")?,
        None => max,
    };
    if bits > max {
        return Err("The prefix length is too long.");
    }
    if (ip.is_ipv4() && bits < 8) || (ip.is_ipv6() && bits < 16) {
        return Err("That range is too wide (IPv4 /8 or narrower, IPv6 /16 or narrower).");
    }
    let net = network(ip, bits);
    let text = if bits == max { net.to_string() } else { format!("{net}/{bits}") };
    Ok((text, net, bits))
}

fn network(ip: IpAddr, bits: u8) -> IpAddr {
    match ip {
        IpAddr::V4(v) => {
            let m: u32 = if bits == 0 { 0 } else { u32::MAX << (32 - bits as u32) };
            IpAddr::V4((u32::from(v) & m).into())
        }
        IpAddr::V6(v) => {
            let m: u128 = if bits == 0 { 0 } else { u128::MAX << (128 - bits as u32) };
            IpAddr::V6((u128::from(v) & m).into())
        }
    }
}

/// Normalises a client IP (strips an IPv4-mapped IPv6 prefix and a port). None for "unknown" / garbage.
pub fn client_ip(raw: &str) -> Option<IpAddr> {
    let s = raw.trim();
    let ip: IpAddr = s.parse().ok().or_else(|| s.rsplit_once(':').and_then(|(h, _)| h.trim_matches(['[', ']']).parse().ok()))?;
    Some(match ip {
        IpAddr::V6(v) => v.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(v)),
        v => v,
    })
}

pub fn ip_allowed(ip: &str, cidrs: &[String]) -> bool {
    let Some(ip) = client_ip(ip) else { return false };
    cidrs.iter().filter_map(|c| parse_cidr(c).ok()).any(|(_, net, bits)| ip.is_ipv4() == net.is_ipv4() && network(ip, bits) == net)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_consistent() {
        let mut seen = BTreeSet::new();
        for p in PERMS {
            assert!(seen.insert(p.key), "duplicate {}", p.key);
            assert!(MODULES.iter().any(|m| m.key == p.module), "module of {}", p.key);
            assert!(["view", "create", "edit", "approve", "export"].contains(&p.action));
        }
        for r in BUILTIN_ROLES {
            for k in preset_perms(r.key).unwrap() {
                assert!(perm(k).is_some(), "{} grants unknown {k}", r.key);
            }
        }
        // every key the apps check today is in the catalogue
        for k in [
            "stats.read", "clients.read", "audit.read", "sessions.read", "sessions.revoke", "staff.read", "spreads.read", "spreads.write", "kyc.read",
            "kyc.review", "dealing.read", "dealing.write", "dealing.policy", "accounts.read", "accounts.write", "finance.adjust", "groups.write",
            "finance.credit", "finance.adjust_approve", "finance.adjust_force", "finance.read", "finance.write", "finance.approve", "finance.settings", "partners.read", "partners.write", "partners.approve",
            "social.read", "social.write", "social.approve", "prop.read", "prop.write", "prop.approve", "algo.read", "algo.write", "algo.settings",
            "content.read", "content.write", "support.read", "support.write", "notifications.write", "marketing.read", "marketing.write",
            "options.read", "options.config", "options.dealing", "options.settle",
        ] {
            assert!(perm(k).is_some(), "missing {k}");
        }
    }

    #[test]
    fn presets_match_the_old_role_maps() {
        let has = |r: &str, k: &str| preset_perms(r).unwrap().contains(&k);
        assert!(has("dealer", "dealing.write") && !has("dealer", "finance.read") && !has("dealer", "kyc.read"));
        assert!(has("compliance", "kyc.review") && has("compliance", "social.approve") && !has("compliance", "sessions.revoke"));
        // client controls: Compliance restricts, blocks and opens read-only staff sessions
        assert!(has("compliance", "clients.restrict") && has("compliance", "clients.block") && has("compliance", "clients.impersonate"));
        assert!(!has("compliance", "clients.impersonate_full") && !has("support", "clients.block") && has("super_admin", "clients.block"));
        assert!(has("finance", "finance.approve") && has("finance", "partners.approve") && !has("finance", "finance.settings"));
        // balance controls: finance adjusts, gives credit and approves; only Super Admin may force
        assert!(has("finance", "finance.adjust") && has("finance", "finance.credit") && has("finance", "finance.adjust_approve"));
        assert!(!has("finance", "finance.adjust_force") && !has("admin", "finance.adjust_force") && has("super_admin", "finance.adjust_force"));
        assert!(!has("dealer", "finance.adjust") && !has("support", "finance.credit"));
        assert!(!has("admin", "owner.tenants") && has("platform_owner", "owner.tenants"));
        assert!(has("marketing", "content.write") && !has("marketing", "clients.read"));
        assert!(has("partner_manager", "partners.write") && !has("support", "partners.write"));
        assert!(!has("viewer", "dealing.write"));
        // account closures (C8): admins and Compliance request and approve; dealers and support don't
        for r in ["admin", "super_admin", "compliance"] {
            assert!(has(r, "accounts.close") && has(r, "accounts.close.approve"), "{r}");
        }
        assert!(!has("dealer", "accounts.close.approve") && !has("support", "accounts.close") && !has("finance", "accounts.close"));
    }

    #[test]
    fn options_permissions() {
        let has = |r: &str, k: &str| preset_perms(r).unwrap().contains(&k);
        let all = ["options.read", "options.config", "options.dealing", "options.settle"];
        for r in ["platform_owner", "super_admin", "admin", "options_risk"] {
            for k in all {
                assert!(has(r, k), "{r} {k}");
            }
        }
        // dealers watch the book and use the dealer controls, but don't configure or settle
        assert!(has("dealer", "options.read") && has("dealer", "options.dealing"));
        assert!(!has("dealer", "options.config") && !has("dealer", "options.settle"));
        for r in ["risk_manager", "finance", "compliance", "support", "sales", "partner_manager", "marketing", "viewer"] {
            assert!(!all.iter().any(|k| has(r, k)), "{r}");
        }
        // any options key implies options.read
        assert_eq!(normalize(&["options.settle"]).unwrap(), vec!["options.read", "options.settle"]);
        // the preset is new: downstream services get a legacy role that covers it
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let svc = service_role("options_risk", &s(&preset_perms("options_risk").unwrap()));
        assert!(svc != "options_risk" && builtin(svc).is_some());
        assert_eq!(service_role("dealer", &s(&preset_perms("dealer").unwrap())), "dealer");
        assert_eq!(service_role("c-opt", &s(&["options.read", "options.dealing"])), "dealer");
    }

    #[test]
    fn normalize_validates_and_adds_view() {
        assert_eq!(normalize(&["finance.approve"]).unwrap(), vec!["finance.approve", "finance.read"]);
        assert_eq!(normalize(&["sessions.revoke", "sessions.revoke"]).unwrap(), vec!["sessions.read", "sessions.revoke"]);
        assert!(normalize(&["root.all"]).is_err());
        assert_eq!(normalize::<&str>(&[]).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn effective_permissions() {
        assert!(effective("system", "platform_owner", false, &[]).contains(&"owner.system".to_string()));
        assert!(!effective("system", "super_admin", false, &[]).iter().any(|k| k.starts_with("owner.")));
        let custom = vec!["kyc.read".to_string(), "owner.tenants".to_string(), "nope".to_string()];
        assert_eq!(effective("custom", "c-x", false, &custom), vec!["kyc.read"]);
        assert_eq!(effective("preset", "dealer", true, &["kyc.read".into()]), vec!["kyc.read"]);
        // full-access staff sessions: Super Admin and Platform Owner only, never a preset or custom role
        let full = "clients.impersonate_full".to_string();
        assert!(effective("system", "super_admin", false, &[]).contains(&full) && effective("system", "platform_owner", false, &[]).contains(&full));
        assert!(!effective("preset", "admin", false, &[]).contains(&full) && effective("preset", "admin", false, &[]).contains(&"clients.impersonate".to_string()));
        assert!(!effective("preset", "admin", true, &[full.clone()]).contains(&full));
        assert_eq!(effective("custom", "c-x", false, &[full.clone(), "clients.impersonate".into()]), vec!["clients.impersonate"]);
        assert!(effective("preset", "dealer", false, &[]).contains(&"dealing.write".to_string()));
    }

    #[test]
    fn service_roles() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(service_role("dealer", &s(&preset_perms("dealer").unwrap())), "dealer");
        assert_eq!(service_role("c-kyc", &s(&["kyc.read", "kyc.review", "clients.read"])), "viewer");
        assert_eq!(service_role("c-desk", &s(&["dealing.read", "dealing.write"])), "dealer");
        assert_eq!(service_role("c-fin", &s(&["finance.read", "finance.approve"])), "finance");
        assert_eq!(service_role("c-mix", &s(&["dealing.write", "finance.approve"])), "admin");
        assert_eq!(service_role("sales", &s(&preset_perms("sales").unwrap())), "viewer");
        // an edited preset that outgrew its built-in role gets a covering one
        assert_eq!(service_role("dealer", &s(&["dealing.write", "finance.read"])), "risk_manager");
    }

    #[test]
    fn cidr_matching() {
        assert_eq!(parse_cidr("203.0.113.7/24").unwrap().0, "203.0.113.0/24");
        assert_eq!(parse_cidr(" 198.51.100.4 ").unwrap().0, "198.51.100.4");
        assert_eq!(parse_cidr("2001:db8::1/48").unwrap().0, "2001:db8::/48");
        assert!(parse_cidr("10.0.0.0/4").is_err());
        assert!(parse_cidr("10.0.0.0/33").is_err());
        assert!(parse_cidr("nope").is_err());
        let list = vec!["203.0.113.0/24".to_string(), "2001:db8::/32".to_string(), "198.51.100.9".to_string()];
        assert!(ip_allowed("203.0.113.200", &list));
        assert!(ip_allowed("::ffff:203.0.113.5", &list));
        assert!(ip_allowed("2001:db8:1::5", &list));
        assert!(ip_allowed("198.51.100.9", &list));
        assert!(!ip_allowed("198.51.100.10", &list));
        assert!(!ip_allowed("unknown", &list));
        assert!(!ip_allowed("203.0.114.1", &list));
    }
}
