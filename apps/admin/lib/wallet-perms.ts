/**
 * Back Office finance permissions for the wallet (deposits, withdrawals, client wallets, reconciliation,
 * wallet settings).
 *
 * The gateway's role → permission matrix (services/gateway/src/admin.rs) has no finance permissions yet, so the
 * admin BFF owns this map, like lib/trading-perms.ts does for trading. It mirrors the role checks the wallet
 * service falls back to (services/wallet/src/state.rs: ROLES_READ / ROLES_WRITE / ROLES_APPROVE /
 * ROLES_SETTINGS); the BFF checks first and the UI hides what the role can't use. The service checks the exact
 * keys forwarded in `x-kalks-staff-perms` (financePerms below).
 *
 * When the gateway starts returning these for a role (any "finance.read" entry in `/v1/admin/auth/me` →
 * permissions), that list wins and this map is no longer consulted. To move them into the gateway, add
 * FinanceRead / FinanceWrite / FinanceApprove / FinanceSettings to `Perm` with the rows below.
 *
 * | permission        | what it allows                                                           | roles                                                  |
 * |-------------------|--------------------------------------------------------------------------|--------------------------------------------------------|
 * | finance.read      | deposits, withdrawals (with risk checklist), wallets, reconciliation,    | platform_owner, super_admin, admin, finance,           |
 * |                   | wallet settings (read), wallet audit                                     | compliance, risk_manager                               |
 * | finance.write     | assign / reject / re-check deposits, manual wallet adjustments,          | platform_owner, super_admin, admin, finance            |
 * |                   | mark withdrawals paid (payout hash)                                      |                                                        |
 * | finance.approve   | approve / reject withdrawals                                             | platform_owner, super_admin, admin, finance            |
 * | finance.settings  | receiving / payout addresses, confirmations, limits, fees, the           | platform_owner, super_admin, admin                     |
 * |                   | four-eyes threshold of manual adjustments                                |                                                        |
 *
 * Balance & credit (manual adjustments, services/wallet/src/ops/adjustments.rs):
 *
 * | permission             | what it allows                                                      | roles                                          |
 * |------------------------|---------------------------------------------------------------------|------------------------------------------------|
 * | finance.adjust         | add / deduct funds on wallets and trading accounts                  | platform_owner, super_admin, admin, finance    |
 * | finance.credit         | give / take credit on trading accounts                              | platform_owner, super_admin, admin, finance    |
 * | finance.adjust_approve | approve / reject adjustments above the four-eyes threshold          | platform_owner, super_admin, admin, finance    |
 * | finance.adjust_force   | force a trading-account deduction past the free margin              | platform_owner, super_admin                    |
 */

export const WALLET_PERMS = ["finance.read", "finance.write", "finance.approve", "finance.settings", "finance.credit", "finance.adjust_approve", "finance.adjust_force"] as const;
export type WalletPerm = (typeof WALLET_PERMS)[number];

const READERS = ["platform_owner", "super_admin", "admin", "finance", "compliance", "risk_manager"];
const FINANCE = ["platform_owner", "super_admin", "admin", "finance"];
const CONFIG = ["platform_owner", "super_admin", "admin"];
const SUPER = ["platform_owner", "super_admin"];

export const WALLET_ROLE_MAP: Record<WalletPerm, readonly string[]> = {
  "finance.read": READERS,
  "finance.write": FINANCE,
  "finance.approve": FINANCE,
  "finance.settings": CONFIG,
  "finance.credit": FINANCE,
  "finance.adjust_approve": FINANCE,
  "finance.adjust_force": SUPER,
};

export const isWalletPerm = (p: string): p is WalletPerm => (WALLET_PERMS as readonly string[]).includes(p);

/** Whether a staff member (role + gateway permission list) holds a wallet permission. */
export function walletAllows(staff: { role: string; permissions?: string[]; rbac?: boolean }, perm: WalletPerm): boolean {
  if (staff.rbac) return staff.permissions?.includes(perm) ?? false;
  if (staff.permissions?.includes("finance.read")) return staff.permissions.includes(perm);
  return WALLET_ROLE_MAP[perm].includes(staff.role);
}

/** Finance permission keys forwarded to the wallet service as `x-kalks-staff-perms`, so it can enforce the exact key
 *  (the gateway's list when authoritative, else the role map above plus finance.adjust from the trading map). */
export function financePerms(staff: { role: string; permissions?: string[]; rbac?: boolean }): string[] {
  if (staff.rbac || staff.permissions?.includes("finance.read")) return (staff.permissions ?? []).filter((p) => p.startsWith("finance."));
  const out = WALLET_PERMS.filter((p) => WALLET_ROLE_MAP[p].includes(staff.role)) as string[];
  if (["platform_owner", "super_admin", "admin", "finance"].includes(staff.role)) out.push("finance.adjust");
  return out;
}
