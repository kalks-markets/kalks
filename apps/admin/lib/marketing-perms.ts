/**
 * Back Office Marketing permissions (services/growth: bonuses, promo codes, banners, contests, loyalty, cashback).
 *
 * The gateway RBAC (services/gateway/src/rbac.rs) defines marketing.read / write / approve and returns them in the
 * staff session (`rbac: true`); that list always wins, custom roles included. This map is the fallback for sessions
 * without RBAC. The BFF forwards the resolved permissions to the growth service in `X-Kalks-Staff-Perms`, which
 * checks them again (role lists below when the header is absent).
 *
 * | permission         | what it allows                                                                        | roles                                           |
 * |--------------------|---------------------------------------------------------------------------------------|-------------------------------------------------|
 * | marketing.read     | every GET: overview, campaigns, grants, promos, banners, contests, loyalty, cashback,  | platform_owner, super_admin, admin, marketing,  |
 * |                    | reports, audit                                                                        | finance, compliance, support, risk_manager,     |
 * |                    |                                                                                       | partner_manager, viewer                         |
 * | marketing.write    | campaigns, promos, banners, contests (create, edit, finalize, disqualify), rules,     | platform_owner, super_admin, admin, marketing   |
 * |                    | tiers, catalogue, cashback programmes, settings, jobs                                 |                                                 |
 * | marketing.approve  | money out: pay contest prizes, manual bonus grants and cancellations, points           | platform_owner, super_admin, admin, finance     |
 * |                    | adjustments, run cashback payouts, retry redemptions; approve / reject prizes,        |                                                 |
 * |                    | mission rewards, held cashback payouts and redemptions (four-eyes: at or above the    |                                                 |
 * |                    | growth `fourEyesThreshold` two different staff members, enforced by the service)       |                                                 |
 */

export const MARKETING_PERMS = ["marketing.read", "marketing.write", "marketing.approve"] as const;
export type MarketingPerm = (typeof MARKETING_PERMS)[number];

const READERS = ["platform_owner", "super_admin", "admin", "marketing", "finance", "compliance", "support", "risk_manager", "partner_manager", "viewer"];
const WRITERS = ["platform_owner", "super_admin", "admin", "marketing"];
const APPROVERS = ["platform_owner", "super_admin", "admin", "finance"];

export const MARKETING_ROLE_MAP: Record<MarketingPerm, readonly string[]> = {
  "marketing.read": READERS,
  "marketing.write": WRITERS,
  "marketing.approve": APPROVERS,
};

/** Whether a staff member (role + gateway permission list) holds a Marketing permission. */
export function marketingAllow(staff: { role: string; permissions?: string[]; rbac?: boolean }, perm: MarketingPerm): boolean {
  if (staff.rbac) return staff.permissions?.includes(perm) ?? false;
  const fromGateway = staff.permissions?.some((p) => p.startsWith("marketing."));
  if (fromGateway) return staff.permissions!.includes(perm);
  return MARKETING_ROLE_MAP[perm].includes(staff.role);
}
