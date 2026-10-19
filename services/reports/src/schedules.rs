//! Scheduled reports (D145): staff-defined daily / weekly / monthly reports emailed as XLSX or CSV to a
//! recipient list. The runner checks every minute; every run is recorded in `schedule_runs`.

use chrono::{DateTime, Datelike, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use crate::broker;
use crate::db::{self, Actor};
use crate::error::{ApiError, ApiResult};
use crate::export::{self, Table};
use crate::mailer::Mail;
use crate::state::App;
use crate::time;

pub const REPORTS: &[(&str, &str)] = &[
    ("pnl", "Broker P&L and revenue"),
    ("deposits", "Deposits, withdrawals and FTDs"),
    ("funnel", "Acquisition funnel"),
    ("cohorts", "Cohorts and LTV"),
    ("activity", "Accounts and activity"),
    ("partners", "IB, PAMM and copy"),
    ("transactions", "Transactions (regulatory)"),
    ("clients", "Client list (regulatory)"),
    ("trades", "Deals (regulatory)"),
    ("aml", "AML review list"),
];

pub fn report_label(k: &str) -> Option<&'static str> {
    REPORTS.iter().find(|(x, _)| *x == k).map(|(_, l)| *l)
}

/// Tables of a report for [from, to) (cohorts ignore the period: last 12 months).
pub async fn report_tables(app: &App, tenant: &str, report: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> ApiResult<(Vec<Table>, Option<Value>)> {
    Ok(match report {
        "pnl" => {
            let (j, t) = broker::pnl(app, tenant, from, to).await?;
            (t, Some(j["totals"].clone()))
        }
        "deposits" => {
            let (j, t) = broker::deposits(app, tenant, from, to).await?;
            (t, Some(j["totals"].clone()))
        }
        "funnel" => {
            let (j, t) = broker::funnel(app, tenant, from, to).await?;
            (t, Some(j["stages"].clone()))
        }
        "cohorts" => {
            let (j, t) = broker::cohorts(app, tenant, 12).await?;
            (t, Some(j["totals"].clone()))
        }
        "activity" => {
            let (j, t) = broker::activity(app, tenant, from, to).await?;
            (t, Some(j["totals"].clone()))
        }
        "partners" => (broker::partners(app, tenant, from, to).await?.1, None),
        "transactions" => (broker::transactions(app, tenant, from, to).await?, None),
        "clients" => (broker::client_list(app, tenant).await?, None),
        "trades" => (broker::trades_export(app, tenant, from, to).await?, None),
        "aml" => (broker::aml(app, tenant, from, to, 10_000.0).await?, None),
        _ => return Err(ApiError::Validation { field: "report", message: "Unknown report.".into() }),
    })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleIn {
    pub name: String,
    pub report: String,
    pub format: Option<String>,
    pub frequency: String,
    pub weekday: Option<i32>,
    pub month_day: Option<i32>,
    pub hour: Option<i32>,
    pub recipients: Vec<String>,
    pub enabled: Option<bool>,
}

fn valid_email(e: &str) -> bool {
    let e = e.trim();
    let Some((u, d)) = e.split_once('@') else { return false };
    !u.is_empty() && d.contains('.') && !d.starts_with('.') && !d.ends_with('.') && e.len() <= 254 && !e.contains(char::is_whitespace) && !e.contains([',', ';', '<', '>'])
}

impl ScheduleIn {
    pub fn validate(&mut self) -> ApiResult<()> {
        self.name = self.name.trim().chars().take(80).collect();
        if self.name.len() < 2 {
            return Err(ApiError::Validation { field: "name", message: "Give the schedule a name.".into() });
        }
        if report_label(&self.report).is_none() {
            return Err(ApiError::Validation { field: "report", message: "Unknown report.".into() });
        }
        let fmt = self.format.clone().unwrap_or_else(|| "xlsx".into());
        if fmt != "xlsx" && fmt != "csv" {
            return Err(ApiError::Validation { field: "format", message: "Format must be xlsx or csv.".into() });
        }
        self.format = Some(fmt);
        if !["daily", "weekly", "monthly"].contains(&self.frequency.as_str()) {
            return Err(ApiError::Validation { field: "frequency", message: "Frequency must be daily, weekly or monthly.".into() });
        }
        if !(1..=7).contains(&self.weekday.unwrap_or(1)) || !(1..=28).contains(&self.month_day.unwrap_or(1)) || !(0..=23).contains(&self.hour.unwrap_or(7)) {
            return Err(ApiError::Validation { field: "hour", message: "Invalid schedule time.".into() });
        }
        self.recipients = self.recipients.iter().map(|r| r.trim().to_lowercase()).filter(|r| !r.is_empty()).collect();
        self.recipients.dedup();
        if self.recipients.is_empty() || self.recipients.len() > 20 || !self.recipients.iter().all(|r| valid_email(r)) {
            return Err(ApiError::Validation { field: "recipients", message: "Add 1 to 20 valid email addresses.".into() });
        }
        Ok(())
    }
}

/// Next run strictly after `after`, at `hour`:00 server time on the matching day.
pub fn next_run(freq: &str, weekday: i32, month_day: i32, hour: i32, after: DateTime<Utc>) -> DateTime<Utc> {
    let mut d = time::server_day(after);
    for _ in 0..800 {
        let ok = match freq {
            "weekly" => d.weekday().number_from_monday() as i32 == weekday,
            "monthly" => d.day() as i32 == month_day,
            _ => true,
        };
        if ok {
            let at = time::day_start(d) + Duration::hours(hour as i64);
            if at > after {
                return at;
            }
        }
        d = d.succ_opt().unwrap();
    }
    after + Duration::days(1)
}

/// The period a run at `at` reports on: the previous server day / 7 days / calendar month.
pub fn period(freq: &str, at: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let today = time::server_day(at);
    match freq {
        "weekly" => (time::day_start(today - Duration::days(7)), time::day_start(today)),
        "monthly" => {
            let m = time::month_start(today);
            (time::day_start(time::add_months(m, -1)), time::day_start(m))
        }
        _ => (time::day_start(today - Duration::days(1)), time::day_start(today)),
    }
}

pub fn schedule_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"), "name": r.get::<String, _>("name"), "report": r.get::<String, _>("report"),
        "reportLabel": report_label(&r.get::<String, _>("report")), "format": r.get::<String, _>("format"),
        "frequency": r.get::<String, _>("frequency"), "weekday": r.get::<i32, _>("weekday"), "monthDay": r.get::<i32, _>("month_day"),
        "hour": r.get::<i32, _>("hour"), "recipients": r.get::<Vec<String>, _>("recipients"), "enabled": r.get::<bool, _>("enabled"),
        "createdBy": r.get::<String, _>("created_by"), "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
        "updatedBy": r.get::<Option<String>, _>("updated_by"), "updatedAt": r.get::<DateTime<Utc>, _>("updated_at"),
        "lastRunAt": r.get::<Option<DateTime<Utc>>, _>("last_run_at"), "lastStatus": r.get::<Option<String>, _>("last_status"),
        "nextRunAt": r.get::<DateTime<Utc>, _>("next_run_at"),
    })
}

pub async fn list(app: &App, tenant: &str) -> ApiResult<Value> {
    let rows = sqlx::query("SELECT * FROM schedules WHERE tenant = $1 ORDER BY id").bind(tenant).fetch_all(&app.pool).await?;
    let runs = sqlx::query("SELECT r.*, s.name FROM schedule_runs r JOIN schedules s ON s.id = r.schedule_id WHERE r.tenant = $1 ORDER BY r.id DESC LIMIT 50").bind(tenant).fetch_all(&app.pool).await?;
    Ok(json!({
        "items": rows.iter().map(schedule_json).collect::<Vec<_>>(),
        "runs": runs.iter().map(|r| json!({"id": r.get::<i64, _>("id"), "scheduleId": r.get::<i64, _>("schedule_id"), "name": r.get::<String, _>("name"), "at": r.get::<DateTime<Utc>, _>("at"),
            "trigger": r.get::<String, _>("trigger"), "status": r.get::<String, _>("status"), "from": r.get::<DateTime<Utc>, _>("period_from"), "to": r.get::<DateTime<Utc>, _>("period_to"),
            "recipients": r.get::<Vec<String>, _>("recipients"), "bytes": r.get::<i32, _>("bytes"), "error": r.get::<Option<String>, _>("error")})).collect::<Vec<_>>(),
        "reports": REPORTS.iter().map(|(k, l)| json!({"key": k, "label": l})).collect::<Vec<_>>(),
        "email": app.mailer.is_some(),
    }))
}

pub async fn create(app: &App, tenant: &str, actor: &Actor, mut s: ScheduleIn) -> ApiResult<Value> {
    s.validate()?;
    let (wd, md, hr) = (s.weekday.unwrap_or(1), s.month_day.unwrap_or(1), s.hour.unwrap_or(7));
    let next = next_run(&s.frequency, wd, md, hr, Utc::now());
    let r = sqlx::query(
        "INSERT INTO schedules (tenant, name, report, format, frequency, weekday, month_day, hour, recipients, enabled, created_by, next_run_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING *",
    )
    .bind(tenant)
    .bind(&s.name)
    .bind(&s.report)
    .bind(s.format.as_deref().unwrap_or("xlsx"))
    .bind(&s.frequency)
    .bind(wd)
    .bind(md)
    .bind(hr)
    .bind(&s.recipients)
    .bind(s.enabled.unwrap_or(true))
    .bind(&actor.name)
    .bind(next)
    .fetch_one(&app.pool)
    .await?;
    let v = schedule_json(&r);
    db::audit(&app.pool, tenant, actor, "schedule.create", Some(&v["id"].to_string()), Some(v.clone())).await;
    Ok(v)
}

pub async fn update(app: &App, tenant: &str, actor: &Actor, id: i64, mut s: ScheduleIn) -> ApiResult<Value> {
    s.validate()?;
    let before = sqlx::query("SELECT * FROM schedules WHERE id = $1 AND tenant = $2").bind(id).bind(tenant).fetch_optional(&app.pool).await?.ok_or_else(|| ApiError::NotFound("Schedule not found.".into()))?;
    let (wd, md, hr) = (s.weekday.unwrap_or(1), s.month_day.unwrap_or(1), s.hour.unwrap_or(7));
    let next = next_run(&s.frequency, wd, md, hr, Utc::now());
    let r = sqlx::query(
        "UPDATE schedules SET name = $3, report = $4, format = $5, frequency = $6, weekday = $7, month_day = $8, hour = $9, recipients = $10, enabled = $11,
           updated_by = $12, updated_at = now(), next_run_at = $13 WHERE id = $1 AND tenant = $2 RETURNING *",
    )
    .bind(id)
    .bind(tenant)
    .bind(&s.name)
    .bind(&s.report)
    .bind(s.format.as_deref().unwrap_or("xlsx"))
    .bind(&s.frequency)
    .bind(wd)
    .bind(md)
    .bind(hr)
    .bind(&s.recipients)
    .bind(s.enabled.unwrap_or(true))
    .bind(&actor.name)
    .bind(next)
    .fetch_one(&app.pool)
    .await?;
    let v = schedule_json(&r);
    db::audit(&app.pool, tenant, actor, "schedule.update", Some(&id.to_string()), Some(json!({"before": schedule_json(&before), "after": v}))).await;
    Ok(v)
}

pub async fn delete(app: &App, tenant: &str, actor: &Actor, id: i64) -> ApiResult<Value> {
    let r = sqlx::query("DELETE FROM schedules WHERE id = $1 AND tenant = $2 RETURNING *").bind(id).bind(tenant).fetch_optional(&app.pool).await?.ok_or_else(|| ApiError::NotFound("Schedule not found.".into()))?;
    db::audit(&app.pool, tenant, actor, "schedule.delete", Some(&id.to_string()), Some(schedule_json(&r))).await;
    Ok(json!({"status": "deleted", "id": id}))
}

fn summary_lines(summary: &Option<Value>) -> Vec<String> {
    match summary {
        Some(Value::Object(m)) => m.iter().filter(|(_, v)| v.is_number()).take(12).map(|(k, v)| format!("{k}: {v}")).collect(),
        Some(Value::Array(a)) => a.iter().map(|x| format!("{}: {}", x["key"].as_str().unwrap_or(""), x["count"])).collect(),
        _ => vec![],
    }
}

/// Builds and sends one run. Returns the run row.
pub async fn run_one(app: &App, id: i64, trigger: &str, actor: &Actor) -> ApiResult<Value> {
    let s = sqlx::query("SELECT * FROM schedules WHERE id = $1").bind(id).fetch_optional(&app.pool).await?.ok_or_else(|| ApiError::NotFound("Schedule not found.".into()))?;
    let tenant: String = s.get("tenant");
    let now = Utc::now();
    let freq: String = s.get("frequency");
    let (from, to) = period(&freq, now);
    let report: String = s.get("report");
    let format: String = s.get("format");
    let name: String = s.get("name");
    let recipients: Vec<String> = s.get("recipients");
    let label = report_label(&report).unwrap_or("Report");
    let day = |t: DateTime<Utc>| time::server_day(t);
    let span = format!("{} to {}", day(from), day(to - Duration::seconds(1)));
    let result: anyhow::Result<(String, usize)> = async {
        let (tables, summary) = report_tables(app, &tenant, &report, from, to).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let (bytes, ctype, ext) = if format == "csv" { (export::csv(&tables), "text/csv", "csv") } else { (export::xlsx(&tables)?, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", "xlsx") };
        let size = bytes.len();
        let file = format!("kalks-{report}-{}-{}.{ext}", day(from), day(to - Duration::seconds(1)));
        let lines = summary_lines(&summary);
        let text = format!("{name}\n{label}, {span} (server time)\n\n{}\n\nThe full report is attached.\n", lines.join("\n"));
        let html = format!(
            "<div style=\"font-family:Helvetica,Arial,sans-serif;color:#15151a\"><div style=\"height:3px;background:#ff5a1f\"></div><h2 style=\"margin:16px 0 4px\">{}</h2><p style=\"color:#6b6b76;margin:0 0 12px\">{} &middot; {} (server time)</p><table style=\"border-collapse:collapse;font-size:13px\">{}</table><p style=\"color:#6b6b76;font-size:12px\">The full report is attached ({}).</p></div>",
            html_escape(&name),
            html_escape(label),
            span,
            lines.iter().map(|l| { let (k, v) = l.split_once(": ").unwrap_or((l, "")); format!("<tr><td style=\"padding:3px 16px 3px 0;color:#6b6b76\">{}</td><td style=\"padding:3px 0;font-weight:600\">{}</td></tr>", html_escape(k), html_escape(v)) }).collect::<String>(),
            ext.to_uppercase()
        );
        match &app.mailer {
            Some(m) => {
                m.send(Mail { to: &recipients, subject: &format!("Kalks report: {name} ({span})"), text: &text, html: &html, attachment: Some((&file, ctype, bytes)) }).await?;
                Ok(("sent".to_string(), size))
            }
            None => {
                tracing::info!(schedule = id, recipients = ?recipients, file, size, "DEV report email (SMTP not configured)");
                Ok(("logged".to_string(), size))
            }
        }
    }
    .await;
    let (status, bytes, error) = match result {
        Ok((st, b)) => (st, b as i32, None),
        Err(e) => ("failed".to_string(), 0, Some(e.to_string().chars().take(500).collect::<String>())),
    };
    let r = sqlx::query("INSERT INTO schedule_runs (schedule_id, tenant, trigger, status, period_from, period_to, recipients, bytes, error) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id, at")
        .bind(id)
        .bind(&tenant)
        .bind(trigger)
        .bind(&status)
        .bind(from)
        .bind(to)
        .bind(&recipients)
        .bind(bytes)
        .bind(&error)
        .fetch_one(&app.pool)
        .await?;
    let next = if trigger == "schedule" { next_run(&freq, s.get("weekday"), s.get("month_day"), s.get("hour"), now) } else { s.get("next_run_at") };
    sqlx::query("UPDATE schedules SET last_run_at = now(), last_status = $2, next_run_at = $3 WHERE id = $1").bind(id).bind(&status).bind(next).execute(&app.pool).await?;
    db::audit(&app.pool, &tenant, actor, "schedule.run", Some(&id.to_string()), Some(json!({"status": status, "trigger": trigger, "from": from, "to": to, "error": error}))).await;
    Ok(json!({"id": r.get::<i64, _>("id"), "at": r.get::<DateTime<Utc>, _>("at"), "status": status, "bytes": bytes, "error": error, "from": from, "to": to}))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub async fn runner(app: App) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        let due: Vec<i64> = match sqlx::query_scalar("SELECT id FROM schedules WHERE enabled AND next_run_at <= now() ORDER BY next_run_at LIMIT 20").fetch_all(&app.pool).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "schedule poll failed");
                continue;
            }
        };
        for id in due {
            if let Err(e) = run_one(&app, id, "schedule", &Actor::system()).await {
                tracing::warn!(schedule = id, error = ?e, "scheduled report failed");
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn next_runs_and_periods() {
        // 2026-09-29 is a Tuesday; server time GMT+3
        let now = Utc.with_ymd_and_hms(2026, 9, 29, 10, 0, 0).unwrap(); // 13:00 server
        assert_eq!(next_run("daily", 1, 1, 7, now), Utc.with_ymd_and_hms(2026, 9, 30, 4, 0, 0).unwrap());
        assert_eq!(next_run("daily", 1, 1, 18, now), Utc.with_ymd_and_hms(2026, 9, 29, 15, 0, 0).unwrap());
        assert_eq!(next_run("weekly", 1, 1, 7, now), Utc.with_ymd_and_hms(2026, 10, 5, 4, 0, 0).unwrap());
        assert_eq!(next_run("monthly", 1, 1, 7, now), Utc.with_ymd_and_hms(2026, 10, 1, 4, 0, 0).unwrap());
        let (f, t) = period("monthly", Utc.with_ymd_and_hms(2026, 10, 1, 4, 0, 0).unwrap());
        assert_eq!((f, t), (Utc.with_ymd_and_hms(2026, 8, 31, 21, 0, 0).unwrap(), Utc.with_ymd_and_hms(2026, 9, 30, 21, 0, 0).unwrap()));
        let (f, t) = period("daily", now);
        assert_eq!(t - f, Duration::days(1));
    }

    #[test]
    fn validates_recipients() {
        let mut s = ScheduleIn { name: "Daily P&L".into(), report: "pnl".into(), format: None, frequency: "daily".into(), weekday: None, month_day: None, hour: Some(7), recipients: vec!["Ops@KalksTrade.com ".into()], enabled: None };
        s.validate().unwrap();
        assert_eq!(s.recipients, vec!["ops@kalkstrade.com"]);
        s.recipients = vec!["bad".into()];
        assert!(s.validate().is_err());
    }
}
