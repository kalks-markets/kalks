//! Append-only audit log (UPDATE / DELETE rejected by trigger).

use serde_json::Value;

pub struct Actor {
    /// `staff:<id>`, `user:<id>`, `service:<name>` or `system`
    pub id: String,
    pub name: Option<String>,
    pub tenant: Option<String>,
}

impl Actor {
    pub fn system() -> Self {
        Actor { id: "system".into(), name: Some("Circle service".into()), tenant: None }
    }
    pub fn user(id: i64) -> Self {
        Actor { id: format!("user:{id}"), name: None, tenant: None }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn record<'e, E: sqlx::PgExecutor<'e>>(ex: E, actor: &Actor, action: &str, target: Option<String>, before: Option<Value>, after: Option<Value>, note: Option<&str>) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO audit_log (actor, actor_name, actor_tenant, action, target, before, after, note) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(&actor.id)
        .bind(&actor.name)
        .bind(&actor.tenant)
        .bind(action)
        .bind(target)
        .bind(before.map(sqlx::types::Json))
        .bind(after.map(sqlx::types::Json))
        .bind(note)
        .execute(ex)
        .await?;
    Ok(())
}
