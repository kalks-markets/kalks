# Content service and media library

Editable copy and pictures for the website, the Client Area, Kalks Trader and (later) emails, without a deploy.
Staff edit in the Back Office (Content → Pages, Content → Media library); the apps read what is published and keep
their built-in copy for every slot that has nothing published, so an empty content service changes nothing.

- Service: `services/content` (Rust, `127.0.0.1:8105`, database `kalks_content`). API contract in
  `services/content/src/api.rs`; value shapes in `src/content.rs`; media rules in `src/media.rs`.
- Back Office: `apps/admin/app/(app)/content/pages`, `apps/admin/app/(app)/content/media`, BFF
  `apps/admin/app/api/content/[...path]/route.ts`, slot catalogue for the editor `apps/admin/lib/content-catalogue.ts`.
- Apps: `@kalks/ui/content` (`ContentProvider`, `useContentText`, `useContentImage`, `useContentLink`, `<ContentText>`,
  `<ThemedImage>`, `<ContentImageView>`), `@kalks/ui/content/server` (`fetchContent`, cached per tenant + locale for
  60 s), `apps/crm/lib/content.ts`, `apps/terminal/lib/content.ts`. The website (separate repo) has its own
  `src/lib/content.ts`.

## Model

| Thing | Key | Notes |
|---|---|---|
| Slot | tenant, surface, page, slot | one `kind` for every locale; `label` for the editor |
| Entry | + locale | a `draft` (unpublished change) and a `published` value; `version` = the published version |
| Version | + version | every publish / rollback / unpublish; the last 50 per slot locale are kept |
| Media asset | id (`m` + 18 hex) | name, alt text per locale, a light file and an optional dark file |
| Media file | id (32 hex, sha256 of tenant + bytes) | the original + WebP variants; immutable, so URLs are cached for a year |

- **Surfaces:** `website`, `app` (Client Area), `trader` (Kalks Trader), `email` (reserved, no reader yet).
- **Kinds:** `text` `{text}` (2 000 chars), `placeholder` `{text}` (200), `rich_text` `{markdown}` (20 000), `link`
  `{label, href}` (`/path`, `#anchor`, `https://`, `http://`, `mailto:`), `image` `{media}`.
- **Locales:** en hi ar ur fa es pt fr de it ru tr id ms vi th zh ja ko bn ta sw. A locale without a published value
  reads English (`pt-BR` → `pt`, unknown → `en`). Image alt text follows the same rule.
- **Tenant:** every row is per broker (`X-Kalks-Tenant` from the BFF; `?tenant=` on the public read). A broker can
  only use its own images; the same bytes uploaded by two brokers are two files.
- **Drafts:** saving a value equal to the published one clears the draft. Publishing takes every draft in scope
  (one slot locale, one slot, or a whole page). Rollback publishes an older version's value again as a new version
  (the draft is kept). Unpublish records a version with no value (the app default shows again). Deleting a slot
  removes every locale and version.

## Media

- Upload: `POST /v1/admin/media` with the raw image as the body and `X-File-Name` (percent-encoded), max 10 MB
  (`CONTENT_MAX_UPLOAD_MB`). The type is sniffed from the bytes: PNG, JPEG, WebP, SVG. Anything else is 415.
- SVG: refused (not rewritten) when it contains scripts, event handlers (`on*`), `javascript:` / `vbscript:` links,
  `foreignObject` / `iframe` / `embed` / `object` / `audio` / `video`, a DOCTYPE or entities, external `href` / `src`
  (only `#fragment` and embedded PNG / JPEG / WebP / GIF data URIs), CSS `@import` or external `url()`. Served with
  `Content-Security-Policy: default-src 'none'; …; sandbox` as a second wall.
- Rasters: decoded with a 12 000 px / 60 MP guard, EXIF orientation applied, and resized to WebP `sm` 480, `md` 1024,
  `lg` 1920 px wide (quality 82) when the original is wider. The original is kept as `orig`.
- Public file: `GET /v1/public/media/{file id}/{orig|sm|md|lg}`: `Cache-Control: public, max-age=31536000, immutable`.
  A variant that doesn't exist serves the next larger one, then the original.
- An asset has a light file and an optional dark file (`POST /v1/admin/media/{id}/file?theme=dark`). Replacing a
  file gives new file ids (new URLs); content pointing at the asset shows the new file on its next read. Changing the
  file of an image used in published content needs `content.publish` and is audited.
- Deleting an asset is refused (409 with the usage list) while any draft or published entry uses it. Deleted assets
  disappear from the library and from content; their files stay in storage (old URLs keep working).

### Storage

`Storage` trait (`services/content/src/storage.rs`): put / get / delete by key `<tenant>/<file id>/<variant>.<ext>`.

- `CONTENT_STORAGE=disk` (default): files under `CONTENT_STORAGE_DIR` (default `~/.kalks-data/content`). Public
  content: not encrypted. Back it up with the database.
- `CONTENT_STORAGE=r2`: Cloudflare R2 or any S3-compatible bucket, path-style requests signed with AWS SigV4
  (signature checked against AWS's published example in the unit tests). Settings: `R2_ENDPOINT`
  (`https://<account id>.r2.cloudflarestorage.com`), `R2_BUCKET`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`,
  `R2_REGION` (default `auto`). The service still streams the files (`/v1/public/media/*`); serving straight from
  an R2 public bucket / CDN host is a later step. Not yet exercised against a real bucket; copy existing files with
  the same keys before switching.

## Public read

`GET /v1/public/content?surface=app&locale=de[&page=auth|page=home,global][&tenant=kalks][&preview=<token>]`
(no token; on api.kalkstrade.com through Caddy, GET only):

```json
{"tenant": "kalks", "surface": "app", "locale": "de", "preview": false,
 "pages": {"auth": {
   "side.headline": {"kind": "text", "locale": "de", "value": {"text": "…"}},
   "side.image": {"kind": "image", "locale": "en", "value": {
     "media": "m…", "alt": "…", "url": "https://api.kalkstrade.com/v1/public/media/<id>/orig",
     "srcset": "…/sm 480w, …/md 1024w, …/orig 2400w", "width": 2400, "height": 1350, "mime": "image/jpeg",
     "variants": {"sm": {"url": "…", "width": 480, "height": 270}},
     "dark": {"url": "…", "srcset": "…", "width": 2400, "height": 1350, "variants": {}}}}}}}
```

`Cache-Control: public, max-age=60, stale-while-revalidate=300` and a weak `ETag` (304 on `If-None-Match`).
`page` is one page, up to 10 comma-separated, or absent (every page of the surface). `locale` in the body is the
locale asked for; each slot's `locale` is the one actually served.

**Preview.** `POST /v1/admin/content/preview-token {surface}` (Back Office "Preview" button) returns a token signed
with `CONTENT_PREVIEW_SECRET` (default: derived from `CONTENT_INTERNAL_TOKEN`) for that tenant + surface, valid 30
minutes. The Back Office opens `<app>/api/content-preview?token=…&path=/login`; that route (Client Area, Kalks Trader,
website) stores the token in the `kalks_content_preview` cookie (HttpOnly, 30 min) and redirects to the path. While
the cookie is there the app asks for drafts (never cached) and shows a "Preview" badge with an Exit link
(`?exit=1`). An expired or foreign token gets 403 from the service and the app falls back to the published copy.

## Staff API and permissions

All under `/v1/admin/*`, `X-Kalks-Internal` required, plus `X-Kalks-Staff` (email) and `X-Kalks-Staff-Perms` from
the BFF (the service checks the permission again when the header is present). Full table in `src/api.rs`.

| Permission | Allows | Roles (gateway RBAC defaults) |
|---|---|---|
| `content.read` | tree, slots, versions, media list / detail, audit, preview links | every role except dealer, risk manager, options risk |
| `content.write` | save / discard drafts, upload images, edit names and alt text, replace files of images not used in published content, import as drafts | Owner, Super admin, Admin, Marketing |
| `content.publish` | publish, unpublish, roll back, delete slots, delete images, replace files of images in published content, import with publish | Owner, Super admin, Admin, Marketing |

Audited (`content_audit`, `GET /v1/admin/content/audit`): `content.publish`, `content.unpublish`, `content.rollback`,
`content.delete`, `content.import` (with publish), `media.replace`, `media.remove_dark`, `media.delete`.

## Slot catalogue

The keys the apps read today. Empty = the built-in default in the right-hand column (nothing changes until something
is published). The Back Office editor lists them from `apps/admin/lib/content-catalogue.ts` (keep both in sync), and
also shows any other slot that exists in the service.

### Client Area (`app`)

| Page | Slot | Kind | Where | Default |
|---|---|---|---|---|
| dashboard | hero.image | image | dashboard hero photo (and the blurred page backdrop) | `/heroes/dashboard.jpg` |
| dashboard | hero.title | text | headline over the hero photo | none (today's dashboard has no headline) |
| dashboard | hero.lead | text | line under the headline | none |
| dashboard | banner.title | text | banner card under the balance strip (shows when a title or image is published) | none |
| dashboard | banner.text | text | banner text | none |
| dashboard | banner.image | image | banner picture | none |
| dashboard | banner.cta | link | banner button | none |
| accounts, wallet, portfolio, copy, prop, markets, options, partner, rewards, academy, profile, support, news | hero.image | image | each main page's hero photo (copy = /social) | `/heroes/<page>.jpg` |
| auth | side.image | image | sign-in / sign-up / reset side photo (also shown for white-label brokers once published) | `/heroes/dashboard.jpg` (none for white-label brokers) |
| auth | side.headline | text | headline on the side photo | i18n `auth.brand.headline` |
| auth | side.body | text | line under it | i18n `auth.brand.body` |
| empty | accounts.image | image | no trading accounts (Accounts) | illustration `welcome` |
| empty | positions.image | image | no open positions (Portfolio) | illustration `emptyPosition` |
| empty | history.image | image | no wallet history | illustration `emptyHistory` |
| empty | referrals.image | image | no referred clients (Partner → Clients) | illustration `partnerIb` |
| empty | notifications.image | image | empty notification centre | icon `bell` |

The Client Area is dark only: images use their dark file when one exists.

### Kalks Trader (`trader`)

| Page | Slot | Kind | Where | Default |
|---|---|---|---|---|
| login | side.image | image | sign-in side photo (light / dark follow the Trader theme) | `/assets/photos/trading-screen.jpg` |
| login | side.eyebrow | text | small heading | i18n `trader.login.brandEyebrow` |
| login | side.title | text | headline | i18n `trader.login.brandTitle` |
| login | side.text | text | line under it | i18n `trader.login.brandTextLive` (demo build: `brandTextDemo`) |

### Website (`website`)

WEBSITE_CATALOGUE_PLACEHOLDER

## Adding a slot

1. Read it where it shows: `useContentText(page, slot, <today's string>)`, `useContentImage(page, slot)` +
   `<ThemedImage>` (or `<ContentImageView fallback=…>`), `useContentLink`; `<EmptyState contentSlot={{page, slot}}>`
   for empty-state pictures. Always pass the current string as the default.
2. Add it to `apps/admin/lib/content-catalogue.ts` (label, kind, default, preview path) and to the table above.
3. Keys: page `^[a-z0-9][a-z0-9_-]{0,63}$`, slot `^[a-z0-9][a-z0-9_.-]{0,95}$` (dotted: `hero.title`).

## Settings

| Variable | Default | |
|---|---|---|
| `CONTENT_BIND` | `127.0.0.1:8105` | |
| `CONTENT_DATABASE_URL` | the gateway's server, database `kalks_content` | created + migrated on start |
| `CONTENT_INTERNAL_TOKEN` | empty (dev only; required in production) | Back Office BFF + seed script |
| `CONTENT_ENV` | `development` | `production` in the systemd unit |
| `CONTENT_PUBLIC_URL` | `http://<bind>` | base of media URLs browsers load (`https://api.kalkstrade.com`) |
| `CONTENT_PREVIEW_SECRET` | derived from the internal token | signs preview tokens |
| `CONTENT_STORAGE` | `disk` | or `r2` |
| `CONTENT_STORAGE_DIR` | `~/.kalks-data/content` | disk storage root |
| `CONTENT_MAX_UPLOAD_MB` | `10` | |
| `R2_ENDPOINT`, `R2_BUCKET`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`, `R2_REGION` | – / `auto` | `CONTENT_STORAGE=r2` |
| `CONTENT_URL` (apps, website) | `http://127.0.0.1:8105` | where the apps' servers read content |
| `CONTENT_PREVIEW_WEBSITE_URL` / `_APP_URL` / `_TRADER_URL` (Back Office) | `http://localhost:3010` / `:3000` / `:3002` | where preview links open |
