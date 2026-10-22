-- A broker's order of phases, sections and chapters, kept apart from the content rows: reordering is not a content
-- edit, so it must not copy a node into a tenant override (which would show it as "Edited" and freeze it on the
-- platform version of that moment). One order per tenant for every language; it wins over content_nodes.ord.

CREATE TABLE content_order (
    tenant      text        NOT NULL,
    kind        text        NOT NULL,
    slug        text        NOT NULL,
    ord         integer     NOT NULL,
    updated_by  text        NOT NULL DEFAULT '',
    updated_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant, kind, slug)
);

-- Earlier reorders wrote such overrides: a tenant row identical to the platform row except for its order. Keep the
-- order here and drop the row, so the node is the platform version again (what learners see doesn't change).
INSERT INTO content_order (tenant, kind, slug, ord, updated_by)
SELECT DISTINCT ON (t.tenant, t.kind, t.slug) t.tenant, t.kind, t.slug, t.ord, t.updated_by
FROM content_nodes t
JOIN content_nodes d ON d.tenant = '*' AND d.lang = t.lang AND d.kind = t.kind AND d.slug = t.slug
WHERE t.tenant <> '*' AND t.data = d.data AND t.parent = d.parent AND t.published = d.published AND t.ord <> d.ord
ORDER BY t.tenant, t.kind, t.slug, (t.lang <> 'en')
ON CONFLICT DO NOTHING;

DELETE FROM content_nodes t
USING content_nodes d
WHERE d.tenant = '*' AND d.lang = t.lang AND d.kind = t.kind AND d.slug = t.slug
  AND t.tenant <> '*' AND t.data = d.data AND t.parent = d.parent AND t.published = d.published;
