//! Database access: connect + migrate, the content seed, effective (tenant-resolved) content and the course tree.

use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool, Row};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use crate::content::{self, Bundle};

pub const DEFAULT_TENANT: &str = "*";
pub const BASE_LANG: &str = "en";

pub async fn connect(url: &str) -> anyhow::Result<PgPool> {
    let opts = PgConnectOptions::from_str(url)?;
    let db = opts.get_database().unwrap_or("kalks_academy").to_string();
    let admin = opts.clone().database("postgres");
    let mut conn = admin.connect().await?;
    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM pg_database WHERE datname = $1").bind(&db).fetch_optional(&mut conn).await?;
    if exists.is_none() {
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{}\"", db.replace('"', "")))).execute(&mut conn).await?;
        tracing::info!(%db, "created database");
    }
    drop(conn);
    let pool = PgPoolOptions::new().max_connections(16).connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/* ------------------------------------------------------------------ */
/* Seed                                                                */
/* ------------------------------------------------------------------ */

pub struct SeedRow {
    pub kind: &'static str,
    pub slug: String,
    pub parent: String,
    pub ord: i32,
    pub version: i32,
    pub data: Value,
}

/// Every node of a loaded language bundle as seed rows.
pub fn seed_rows(b: &Bundle) -> Vec<SeedRow> {
    let mut rows = vec![];
    for p in &b.phases {
        rows.push(SeedRow {
            kind: "phase",
            slug: p.slug.clone(),
            parent: String::new(),
            ord: p.order,
            version: p.version,
            data: json!({"title": p.title, "level": p.level, "summary": p.summary}),
        });
        for s in &p.sections {
            rows.push(SeedRow {
                kind: "section",
                slug: s.def.slug.clone(),
                parent: p.slug.clone(),
                ord: s.order,
                version: p.version,
                data: json!({"title": s.def.title, "summary": s.def.summary, "track": s.def.track}),
            });
            for c in &s.chapters {
                rows.push(SeedRow {
                    kind: "chapter",
                    slug: c.slug.clone(),
                    parent: s.def.slug.clone(),
                    ord: c.order,
                    version: c.version,
                    data: json!({
                        "title": c.title, "summary": c.summary, "body": c.body, "takeaways": c.takeaways,
                        "practice": c.practice, "quiz": c.quiz, "words": c.words, "minutes": c.minutes,
                    }),
                });
            }
        }
        if let Some(e) = &p.exam {
            rows.push(SeedRow { kind: "exam", slug: p.slug.clone(), parent: p.slug.clone(), ord: 0, version: e.version, data: json!({"pass_mark": e.pass_mark, "questions": e.questions}) });
        }
    }
    for (i, t) in b.glossary.iter().enumerate() {
        rows.push(SeedRow {
            kind: "term",
            slug: t.slug.clone(),
            parent: t.category.clone(),
            ord: i as i32,
            version: 1,
            data: json!({"term": t.term, "category": t.category, "definition": t.definition, "related": t.related}),
        });
    }
    rows
}

#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
pub struct SeedStats {
    pub inserted: u64,
    pub updated: u64,
    pub unchanged: u64,
    pub retired: u64,
}

/// Idempotent upsert of the platform default (tenant '*') for one language: new slugs are inserted, a row is
/// rewritten when its content hash changed or the file version went up (the published flag is kept), and
/// default rows whose file was removed are unpublished. Tenant overrides are never touched.
pub async fn seed_lang(pool: &PgPool, b: &Bundle) -> anyhow::Result<SeedStats> {
    let rows = seed_rows(b);
    let mut st = SeedStats::default();
    let mut tx = pool.begin().await?;
    let existing: HashMap<(String, String), (String, i32)> =
        sqlx::query("SELECT kind, slug, source_hash, source_version FROM content_nodes WHERE tenant = '*' AND lang = $1")
            .bind(&b.lang)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|r| ((r.get::<String, _>(0), r.get::<String, _>(1)), (r.get::<String, _>(2), r.get::<i32, _>(3))))
            .collect();
    let mut seen = std::collections::HashSet::new();
    for r in &rows {
        let key = (r.kind.to_string(), r.slug.clone());
        seen.insert(key.clone());
        let hash = content::hash_json(&json!([r.parent, r.ord, r.data]));
        match existing.get(&key) {
            Some((h, v)) if *h == hash && *v >= r.version => {
                st.unchanged += 1;
                continue;
            }
            Some(_) => st.updated += 1,
            None => st.inserted += 1,
        }
        sqlx::query(
            "INSERT INTO content_nodes (tenant, lang, kind, slug, parent, ord, published, data, source_version, source_hash, updated_by)
             VALUES ('*', $1, $2, $3, $4, $5, true, $6, $7, $8, 'seed')
             ON CONFLICT (tenant, lang, kind, slug) DO UPDATE
               SET parent = EXCLUDED.parent, ord = EXCLUDED.ord, data = EXCLUDED.data, source_version = EXCLUDED.source_version,
                   source_hash = EXCLUDED.source_hash, updated_by = 'seed', updated_at = now()",
        )
        .bind(&b.lang)
        .bind(r.kind)
        .bind(&r.slug)
        .bind(&r.parent)
        .bind(r.ord)
        .bind(&r.data)
        .bind(r.version)
        .bind(&hash)
        .execute(&mut *tx)
        .await?;
    }
    for (kind, slug) in existing.keys() {
        if !seen.contains(&(kind.clone(), slug.clone())) {
            let n = sqlx::query("UPDATE content_nodes SET published = false, updated_by = 'seed:removed', updated_at = now() WHERE tenant = '*' AND lang = $1 AND kind = $2 AND slug = $3 AND published")
                .bind(&b.lang)
                .bind(kind)
                .bind(slug)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            st.retired += n;
        }
    }
    tx.commit().await?;
    Ok(st)
}

/// Loads every language under `root`, refuses languages with lint errors, and seeds the rest.
pub async fn seed_all(pool: &PgPool, root: &Path) -> anyhow::Result<Vec<(String, SeedStats)>> {
    let mut out = vec![];
    for lang in content::languages(root) {
        let b = content::load_lang(root, &lang);
        let (errors, warnings) = content::lint(&b);
        for w in warnings.iter().take(20) {
            tracing::warn!(%lang, warning = %w, "content lint");
        }
        if !errors.is_empty() {
            for e in errors.iter().take(50) {
                tracing::error!(%lang, error = %e, "content lint");
            }
            // seed what loaded anyway so a single bad file does not take the Academy down; the lint binary fails CI
            tracing::error!(%lang, errors = errors.len(), "content has lint errors; seeding the files that loaded");
        }
        let st = seed_lang(pool, &b).await?;
        tracing::info!(%lang, inserted = st.inserted, updated = st.updated, unchanged = st.unchanged, retired = st.retired, "academy content seeded");
        out.push((lang, st));
    }
    Ok(out)
}

/* ------------------------------------------------------------------ */
/* Effective content                                                   */
/* ------------------------------------------------------------------ */

#[derive(Debug, Clone)]
pub struct Node {
    pub tenant: String,
    pub lang: String,
    pub kind: String,
    pub slug: String,
    pub parent: String,
    pub ord: i32,
    pub published: bool,
    pub data: Value,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub updated_by: String,
    /// A platform default exists for this slug (so the tenant row is an override, not a custom node).
    pub has_default: bool,
}

impl Node {
    pub fn s(&self, k: &str) -> String {
        self.data.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
    }
    pub fn source(&self) -> &'static str {
        match (self.tenant.as_str(), self.has_default) {
            (DEFAULT_TENANT, _) => "default",
            (_, true) => "override",
            _ => "custom",
        }
    }
}

/// Effective order of node `n` for tenant `$1`: the tenant's own order (content_order) wins over the row's.
const ORD: &str = "COALESCE((SELECT o.ord FROM content_order o WHERE o.tenant = $1 AND o.kind = n.kind AND o.slug = n.slug), n.ord) AS ord";

/// Tenant-resolved nodes: tenant override in `lang` > default in `lang` > tenant override in English > default
/// in English. `with_body` = false strips chapter bodies (catalogue / tree queries).
pub async fn nodes(pool: &PgPool, tenant: &str, lang: &str, kinds: &[&str], with_body: bool) -> anyhow::Result<Vec<Node>> {
    let kinds: Vec<String> = kinds.iter().map(|s| s.to_string()).collect();
    let data = if with_body { "data" } else { "data - 'body' AS data" };
    let sql = format!(
        "SELECT DISTINCT ON (kind, slug) tenant, lang, kind, slug, parent, {ORD}, published, {data}, updated_at, updated_by,
                EXISTS (SELECT 1 FROM content_nodes d WHERE d.tenant = '*' AND d.kind = n.kind AND d.slug = n.slug) AS has_default
         FROM content_nodes n WHERE tenant IN ($1, '*') AND lang IN ($2, 'en') AND kind = ANY($3)
         ORDER BY kind, slug, (lang <> $2), (tenant = '*')"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(tenant).bind(lang).bind(&kinds).fetch_all(pool).await?;
    Ok(rows.into_iter().map(row_node).collect())
}

fn row_node(r: sqlx::postgres::PgRow) -> Node {
    Node {
        tenant: r.get("tenant"),
        lang: r.get("lang"),
        kind: r.get("kind"),
        slug: r.get("slug"),
        parent: r.get("parent"),
        ord: r.get("ord"),
        published: r.get("published"),
        data: r.get("data"),
        updated_at: r.get("updated_at"),
        updated_by: r.get("updated_by"),
        has_default: r.get("has_default"),
    }
}

pub async fn node(pool: &PgPool, tenant: &str, lang: &str, kind: &str, slug: &str) -> anyhow::Result<Option<Node>> {
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT tenant, lang, kind, slug, parent, {ORD}, published, data, updated_at, updated_by,
                EXISTS (SELECT 1 FROM content_nodes d WHERE d.tenant = '*' AND d.kind = n.kind AND d.slug = n.slug) AS has_default
         FROM content_nodes n WHERE tenant IN ($1, '*') AND lang IN ($2, 'en') AND kind = $3 AND slug = $4
         ORDER BY (lang <> $2), (tenant = '*') LIMIT 1",
    )))
    .bind(tenant)
    .bind(lang)
    .bind(kind)
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    Ok(r.map(row_node))
}

/* ------------------------------------------------------------------ */
/* Course tree                                                         */
/* ------------------------------------------------------------------ */

#[derive(Debug, Clone)]
pub struct TreeSection {
    pub node: Node,
    pub chapters: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct TreePhase {
    pub node: Node,
    pub sections: Vec<TreeSection>,
    pub exam: Option<Node>,
}

impl TreePhase {
    pub fn chapters(&self) -> impl Iterator<Item = &Node> {
        self.sections.iter().flat_map(|s| s.chapters.iter())
    }

    /// A product phase (e.g. phase 9, Kalks FX Options: a single `options` section) rather than a core
    /// fundamental + technical phase. Shown to clients as an elective; it never depends on earlier phases.
    pub fn is_elective(&self) -> bool {
        let tracks: Vec<String> = self.sections.iter().map(|s| s.node.s("track")).collect();
        content::is_product_phase(tracks.iter().map(String::as_str))
    }
}

/// Builds phase > section > chapter from effective nodes. `published_only` hides unpublished nodes and
/// everything below an unpublished parent (the learner view).
pub fn tree(nodes: &[Node], published_only: bool) -> Vec<TreePhase> {
    let keep = |n: &Node| !published_only || n.published;
    let mut phases: Vec<TreePhase> = nodes
        .iter()
        .filter(|n| n.kind == "phase" && keep(n))
        .map(|p| TreePhase {
            node: p.clone(),
            sections: vec![],
            exam: nodes.iter().find(|e| e.kind == "exam" && e.slug == p.slug && keep(e)).cloned(),
        })
        .collect();
    phases.sort_by(|a, b| a.node.ord.cmp(&b.node.ord).then_with(|| a.node.slug.cmp(&b.node.slug)));
    for p in &mut phases {
        let mut secs: Vec<TreeSection> = nodes
            .iter()
            .filter(|s| s.kind == "section" && s.parent == p.node.slug && keep(s))
            .map(|s| {
                let mut chapters: Vec<Node> = nodes.iter().filter(|c| c.kind == "chapter" && c.parent == s.slug && keep(c)).cloned().collect();
                chapters.sort_by(|a, b| a.ord.cmp(&b.ord).then_with(|| a.slug.cmp(&b.slug)));
                TreeSection { node: s.clone(), chapters }
            })
            .collect();
        secs.sort_by(|a, b| a.node.ord.cmp(&b.node.ord).then_with(|| a.node.slug.cmp(&b.node.slug)));
        p.sections = secs;
    }
    phases
}

pub async fn course(pool: &PgPool, tenant: &str, lang: &str, published_only: bool) -> anyhow::Result<Vec<TreePhase>> {
    let ns = nodes(pool, tenant, lang, &["phase", "section", "chapter", "exam"], false).await?;
    Ok(tree(&ns, published_only))
}

/// Copy-on-write write of one node for a tenant (the tenant's override row).
#[allow(clippy::too_many_arguments)]
pub async fn put_override(pool: &PgPool, tenant: &str, lang: &str, kind: &str, slug: &str, parent: &str, ord: i32, published: bool, data: &Value, staff: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO content_nodes (tenant, lang, kind, slug, parent, ord, published, data, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (tenant, lang, kind, slug) DO UPDATE
           SET parent = EXCLUDED.parent, ord = EXCLUDED.ord, published = EXCLUDED.published, data = EXCLUDED.data,
               updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(tenant)
    .bind(lang)
    .bind(kind)
    .bind(slug)
    .bind(parent)
    .bind(ord)
    .bind(published)
    .bind(data)
    .bind(staff)
    .execute(pool)
    .await?;
    Ok(())
}

const SET_ORDER: &str = "INSERT INTO content_order (tenant, kind, slug, ord, updated_by) VALUES ($1, $2, $3, $4, $5)
     ON CONFLICT (tenant, kind, slug) DO UPDATE SET ord = EXCLUDED.ord, updated_by = EXCLUDED.updated_by, updated_at = now()";

/// Sets a tenant's order of nodes (`slugs[i]` gets order i + 1) without touching their content.
pub async fn put_order(pool: &PgPool, tenant: &str, kind: &str, slugs: &[String], staff: &str) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for (i, slug) in slugs.iter().enumerate() {
        sqlx::query(SET_ORDER).bind(tenant).bind(kind).bind(slug).bind(i as i32 + 1).bind(staff).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Sets one node's order for a tenant.
pub async fn set_order(pool: &PgPool, tenant: &str, kind: &str, slug: &str, ord: i32, staff: &str) -> anyhow::Result<()> {
    sqlx::query(SET_ORDER).bind(tenant).bind(kind).bind(slug).bind(ord).bind(staff).execute(pool).await?;
    Ok(())
}

pub async fn audit(pool: &PgPool, tenant: &str, staff: &str, action: &str, kind: &str, slug: &str, lang: &str, detail: Value) {
    let r = sqlx::query("INSERT INTO content_audit (tenant, staff, action, kind, slug, lang, detail) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(tenant)
        .bind(staff)
        .bind(action)
        .bind(kind)
        .bind(slug)
        .bind(lang)
        .bind(detail)
        .execute(pool)
        .await;
    if let Err(e) = r {
        tracing::error!(error = %e, "content audit write failed");
    }
}
