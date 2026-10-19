/**
 * Back Office Academy CMS: permissions and the server-only client for services/academy (127.0.0.1:8098).
 *
 * The gateway's role → permission matrix has no content permissions yet, so the admin BFF owns this map. When
 * the gateway starts returning "content.*" permissions for a role (`/v1/admin/auth/me` → permissions), that
 * list wins and this map is no longer consulted.
 *
 * | permission    | what it allows                                                                 | roles                                          |
 * |---------------|--------------------------------------------------------------------------------|------------------------------------------------|
 * | content.read  | course tree, chapters, quizzes, exams, glossary, learner stats, edit history   | every staff role except dealer, risk_manager   |
 * | content.write | edit / create chapters, quizzes and exams, publish / unpublish, reorder, reset | platform_owner, super_admin, admin, marketing  |
 * | content.publish | site content (services/content): publish / unpublish / roll back, delete slots and media, change live images | platform_owner, super_admin, admin, marketing |
 *
 * Edits are copy-on-write overrides for the staff member's tenant; the platform default comes from the
 * versioned files in content/academy (seeded by the service on start).
 */

export const CONTENT_PERMS = ["content.read", "content.write", "content.publish"] as const;
export type ContentPerm = (typeof CONTENT_PERMS)[number];

export const CONTENT_ROLE_MAP: Record<ContentPerm, readonly string[]> = {
  "content.read": ["platform_owner", "super_admin", "admin", "marketing", "support", "compliance", "finance", "partner_manager", "viewer"],
  "content.write": ["platform_owner", "super_admin", "admin", "marketing"],
  "content.publish": ["platform_owner", "super_admin", "admin", "marketing"],
};

export function contentAllows(staff: { role: string; permissions?: string[]; rbac?: boolean }, perm: ContentPerm): boolean {
  if (staff.rbac) return staff.permissions?.includes(perm) ?? false;
  if (staff.permissions?.some((p) => p.startsWith("content."))) return staff.permissions.includes(perm);
  return CONTENT_ROLE_MAP[perm].includes(staff.role);
}
