-- Content service (services/content): media library + editable content slots with draft / publish versions.
-- Every row is scoped by tenant (the broker slug the BFF sends in X-Kalks-Tenant).

-- One stored image (an upload). The id is content-hashed (sha256 of tenant + bytes), so its public URLs never
-- change meaning and are served with immutable cache headers. `variants` maps a variant name (orig, sm, md, lg)
-- to {key, mime, width, height, bytes}; `key` is the storage key (storage.rs).
CREATE TABLE media_files (
    id          text        PRIMARY KEY,
    tenant      text        NOT NULL,
    mime        text        NOT NULL,
    bytes       bigint      NOT NULL,
    width       integer,
    height      integer,
    variants    jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_by  text        NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX media_files_tenant ON media_files (tenant);

-- A library entry: the light (default) file, an optional dark-theme file and alt text per locale ({"en": "..."}).
CREATE TABLE media_assets (
    id          text        PRIMARY KEY,
    tenant      text        NOT NULL,
    name        text        NOT NULL,
    alt         jsonb       NOT NULL DEFAULT '{}'::jsonb,
    light_file  text        NOT NULL REFERENCES media_files (id),
    dark_file   text        REFERENCES media_files (id),
    created_by  text        NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_by  text        NOT NULL,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    deleted_at  timestamptz
);
CREATE INDEX media_assets_tenant ON media_assets (tenant, updated_at DESC) WHERE deleted_at IS NULL;

-- A content slot: (surface, page, slot) with one kind for every locale. `label` / `help` describe it in the editor.
CREATE TABLE content_slots (
    tenant      text        NOT NULL,
    surface     text        NOT NULL,
    page        text        NOT NULL,
    slot        text        NOT NULL,
    kind        text        NOT NULL,
    label       text        NOT NULL DEFAULT '',
    created_by  text        NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant, surface, page, slot)
);

-- One locale of a slot: the working draft (NULL = no unpublished change) and the published value (NULL = not
-- published: the app shows its built-in default). `version` = the content_versions row now published.
CREATE TABLE content_entries (
    tenant        text        NOT NULL,
    surface       text        NOT NULL,
    page          text        NOT NULL,
    slot          text        NOT NULL,
    locale        text        NOT NULL,
    draft         jsonb,
    draft_by      text,
    draft_at      timestamptz,
    published     jsonb,
    version       integer     NOT NULL DEFAULT 0,
    published_by  text,
    published_at  timestamptz,
    PRIMARY KEY (tenant, surface, page, slot, locale),
    FOREIGN KEY (tenant, surface, page, slot) REFERENCES content_slots (tenant, surface, page, slot) ON DELETE CASCADE
);
CREATE INDEX content_entries_page ON content_entries (tenant, surface, page);

-- Every publish / rollback / unpublish of a slot locale (value NULL = unpublished). The last 50 per slot locale are kept.
CREATE TABLE content_versions (
    id            bigserial   PRIMARY KEY,
    tenant        text        NOT NULL,
    surface       text        NOT NULL,
    page          text        NOT NULL,
    slot          text        NOT NULL,
    locale        text        NOT NULL,
    version       integer     NOT NULL,
    action        text        NOT NULL,
    from_version  integer,
    value         jsonb,
    by_staff      text        NOT NULL,
    at            timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant, surface, page, slot, locale, version)
);

-- Staff changes that reach clients (publish, rollback, unpublish, delete, media replace / delete).
CREATE TABLE content_audit (
    id          bigserial   PRIMARY KEY,
    tenant      text        NOT NULL,
    staff       text        NOT NULL,
    action      text        NOT NULL,
    target      text        NOT NULL,
    detail      jsonb       NOT NULL DEFAULT '{}'::jsonb,
    at          timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX content_audit_tenant ON content_audit (tenant, at DESC);
