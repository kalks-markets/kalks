/**
 * Back Office mock data — Security & Audit (SEC_) and Organization (ORG_).
 * Server time GMT+3. "Now" is 24 Sep 2026 14:32 GMT+3.
 */
import { seeded, hashString } from "./rng";
import { PEOPLE, type Person } from "./people";

export const SEC_NOW = "2026-09-24T14:32:00+03:00";

/* ================================================================== */
/* Organization                                                        */
/* ================================================================== */

export type OrgRoleKey = "super" | "dealer" | "risk" | "finance" | "compliance" | "support" | "sales" | "ib" | "custom";
export type OrgDeskKey = "dealing" | "risk" | "finance" | "compliance" | "support-en" | "support-ar" | "sales-mena" | "sales-asia" | "ib";
export type OrgStatus = "active" | "invited" | "suspended";
export type OrgTwoFa = "hardware" | "totp" | "sms" | "none";

export const ORG_TENANTS = [
  { key: "kalks", name: "Kalks Markets", color: "#ff5a1f", short: "KM" },
  { key: "aurum", name: "Aurum FX", color: "#e9b949", short: "AU" },
  { key: "nova", name: "NovaTrade Asia", color: "#38bdf8", short: "NT" },
  { key: "dunes", name: "Dunes Capital", color: "#22c55e", short: "DC" },
] as const;
export type OrgTenantKey = (typeof ORG_TENANTS)[number]["key"];

export interface OrgEmployee {
  id: string;
  person: Person;
  name: string;
  email: string;
  title: string;
  role: OrgRoleKey;
  desk: OrgDeskKey | null;
  status: OrgStatus;
  twoFa: OrgTwoFa;
  lastActive: string; // ISO
  online: boolean;
  tenants: OrgTenantKey[];
  joined: string;
  location: string;
}

const staffEmail = (n: string) => `${n.toLowerCase().split(" ")[0]}.${n.toLowerCase().split(" ").slice(-1)[0]!.replace(/[^a-z]/g, "")}@kalkstrade.com`;

const EMP_RAW: [number, string, OrgRoleKey, OrgDeskKey | null, OrgStatus, OrgTwoFa, string, boolean, OrgTenantKey[], string, string][] = [
  [9, "Chief Operating Officer", "super", null, "active", "hardware", "2026-09-24T14:21:00+03:00", true, ["kalks", "aurum", "nova", "dunes"], "2023-02-01", "London"],
  [4, "Risk Manager", "risk", "risk", "active", "hardware", "2026-09-24T14:32:00+03:00", true, ["kalks", "aurum", "nova", "dunes"], "2023-06-12", "Dubai"],
  [19, "Head of Dealing", "dealer", "dealing", "active", "hardware", "2026-09-24T14:30:00+03:00", true, ["kalks", "aurum", "nova", "dunes"], "2023-04-03", "Limassol"],
  [15, "Senior Dealer", "dealer", "dealing", "active", "totp", "2026-09-24T14:28:00+03:00", true, ["kalks", "aurum", "dunes"], "2024-01-15", "Istanbul"],
  [14, "Dealer (Asia shift)", "dealer", "dealing", "active", "totp", "2026-09-24T09:02:00+03:00", false, ["kalks", "nova"], "2024-08-19", "Tokyo"],
  [17, "Risk Analyst", "risk", "risk", "active", "totp", "2026-09-24T14:12:00+03:00", true, ["kalks", "aurum", "nova", "dunes"], "2025-03-10", "Accra"],
  [12, "Finance Lead", "finance", "finance", "active", "hardware", "2026-09-24T14:26:00+03:00", true, ["kalks", "aurum", "nova", "dunes"], "2023-03-20", "Limassol"],
  [13, "Payments Officer", "finance", "finance", "active", "totp", "2026-09-24T13:58:00+03:00", true, ["kalks", "nova"], "2024-05-06", "Mumbai"],
  [6, "MLRO · Head of Compliance", "compliance", "compliance", "active", "hardware", "2026-09-24T14:05:00+03:00", true, ["kalks", "aurum", "nova", "dunes"], "2023-02-14", "Milan"],
  [10, "KYC Analyst", "compliance", "compliance", "active", "totp", "2026-09-24T14:31:00+03:00", true, ["kalks", "nova"], "2024-10-01", "Kuala Lumpur"],
  [22, "KYC Analyst", "compliance", "compliance", "invited", "none", "2026-09-22T10:15:00+03:00", false, ["kalks"], "2026-09-22", "Manila"],
  [8, "Support Lead · EN", "support", "support-en", "active", "totp", "2026-09-24T14:29:00+03:00", true, ["kalks", "aurum", "nova"], "2023-09-04", "Singapore"],
  [23, "Support Agent · EN", "support", "support-en", "active", "sms", "2026-09-24T14:17:00+03:00", true, ["kalks", "aurum"], "2025-01-20", "Cape Town"],
  [16, "Support Agent · EN/PT", "support", "support-en", "active", "totp", "2026-09-24T12:40:00+03:00", false, ["kalks", "aurum"], "2025-06-02", "São Paulo"],
  [20, "Support Lead · AR", "support", "support-ar", "active", "totp", "2026-09-24T14:24:00+03:00", true, ["kalks", "dunes"], "2024-02-12", "Cairo"],
  [1, "Support Agent · AR", "support", "support-ar", "active", "sms", "2026-09-24T14:02:00+03:00", true, ["kalks", "dunes"], "2025-04-28", "Dubai"],
  [5, "Sales Director · MENA", "sales", "sales-mena", "active", "hardware", "2026-09-24T14:19:00+03:00", true, ["kalks", "dunes"], "2023-05-08", "Riyadh"],
  [18, "Account Manager · MENA", "sales", "sales-mena", "active", "totp", "2026-09-24T13:47:00+03:00", true, ["kalks", "dunes"], "2025-02-17", "Karachi"],
  [3, "Sales Director · Asia", "sales", "sales-asia", "active", "totp", "2026-09-24T11:36:00+03:00", false, ["kalks", "nova"], "2023-11-13", "Ho Chi Minh City"],
  [21, "Account Manager · Asia", "sales", "sales-asia", "active", "sms", "2026-09-24T10:52:00+03:00", false, ["kalks", "nova"], "2025-07-07", "Bengaluru"],
  [7, "Head of Partnerships", "ib", "ib", "active", "hardware", "2026-09-24T14:08:00+03:00", true, ["kalks", "aurum", "nova", "dunes"], "2023-08-21", "Lagos"],
  [2, "IB Manager · LatAm", "ib", "ib", "active", "totp", "2026-09-24T08:30:00+03:00", false, ["kalks", "aurum"], "2024-09-16", "São Paulo"],
  [11, "IB Manager · LatAm", "ib", "ib", "suspended", "totp", "2026-09-11T17:44:00+03:00", false, ["kalks"], "2024-03-25", "Mexico City"],
];

export const ORG_EMPLOYEES: OrgEmployee[] = EMP_RAW.map(([pi, title, role, desk, status, twoFa, lastActive, online, tenants, joined, location], i) => {
  const p = PEOPLE[pi]!;
  return {
    id: `stf_${2100 + i}`,
    person: p,
    name: p.name,
    email: staffEmail(p.name),
    title,
    role,
    desk,
    status,
    twoFa,
    lastActive,
    online,
    tenants,
    joined,
    location,
  };
});

export const ORG_ME = ORG_EMPLOYEES[1]!; // Priya Nair
export const orgEmployee = (personIdx: number) => ORG_EMPLOYEES.find((e) => e.person === PEOPLE[personIdx]) ?? ORG_EMPLOYEES[0]!;

/* ---------------------------- Roles -------------------------------- */

export const ORG_MODULES = [
  { key: "clients", label: "Clients", hint: "Users, leads, segments" },
  { key: "kyc", label: "KYC", hint: "Queue, documents, AML" },
  { key: "trading", label: "Trading", hint: "Positions, orders, dealer desk" },
  { key: "config", label: "Config", hint: "Groups, symbols, spreads, swaps" },
  { key: "finance", label: "Finance", hint: "Deposits, wallets, adjustments" },
  { key: "withdrawals", label: "Withdrawals", hint: "Payout approvals" },
  { key: "partners", label: "Partners", hint: "IBs, plans, rebates" },
  { key: "social", label: "Social", hint: "Copy, PAMM, API keys" },
  { key: "prop", label: "Prop", hint: "Challenges, funded, payouts" },
  { key: "marketing", label: "Marketing", hint: "Bonuses, contests, promo" },
  { key: "support", label: "Support", hint: "Inbox, canned replies" },
  { key: "content", label: "Content", hint: "News, legal, emails" },
  { key: "analytics", label: "Analytics", hint: "P&L, cohorts, reports" },
  { key: "security", label: "Security", hint: "Audit log, sessions, IPs" },
  { key: "org", label: "Organization", hint: "Staff, roles, desks" },
  { key: "brokers", label: "Brokers", hint: "Tenants, billing, flags" },
  { key: "settings", label: "Settings", hint: "Branding, integrations" },
] as const;
export type OrgModuleKey = (typeof ORG_MODULES)[number]["key"];
export const ORG_ACTIONS = ["view", "create", "edit", "approve", "export"] as const;
export type OrgAction = (typeof ORG_ACTIONS)[number];
export type OrgPermMatrix = Record<OrgModuleKey, Record<OrgAction, boolean>>;

/** Compact preset spec: module -> letters of v/c/e/a/x. */
type Spec = Partial<Record<OrgModuleKey, string>>;
const L: Record<string, OrgAction> = { v: "view", c: "create", e: "edit", a: "approve", x: "export" };
export function orgMatrix(spec: Spec | "all"): OrgPermMatrix {
  const out = {} as OrgPermMatrix;
  for (const m of ORG_MODULES) {
    const s = spec === "all" ? "vceax" : (spec[m.key] ?? "");
    out[m.key] = { view: false, create: false, edit: false, approve: false, export: false };
    for (const ch of s) if (L[ch]) out[m.key][L[ch]!] = true;
  }
  return out;
}

const PRESET_SPECS: Record<Exclude<OrgRoleKey, "custom">, Spec | "all"> = {
  super: "all",
  dealer: { clients: "v", trading: "vceax", config: "vce", finance: "v", partners: "v", social: "v", prop: "va", analytics: "vx", security: "v" },
  risk: { clients: "vx", kyc: "v", trading: "veax", config: "vea", finance: "v", withdrawals: "va", partners: "v", social: "vea", prop: "veax", analytics: "vx", security: "vx" },
  finance: { clients: "v", kyc: "v", trading: "v", finance: "vceax", withdrawals: "vceax", partners: "vax", prop: "va", marketing: "v", analytics: "vx", security: "v" },
  compliance: { clients: "vcex", kyc: "vceax", trading: "v", finance: "v", withdrawals: "va", partners: "va", social: "va", prop: "v", content: "ve", analytics: "vx", security: "vx" },
  support: { clients: "ve", kyc: "vc", trading: "v", finance: "v", withdrawals: "v", partners: "v", social: "v", prop: "v", support: "vceax", content: "v" },
  sales: { clients: "vcex", kyc: "v", finance: "v", partners: "vc", marketing: "vc", support: "vc", analytics: "v" },
  ib: { clients: "v", kyc: "v", finance: "v", partners: "vceax", marketing: "vce", analytics: "vx", support: "v" },
};

export interface OrgRole {
  key: string;
  name: string;
  description: string;
  preset: boolean;
  tone: "ember" | "gold" | "up" | "down" | "info" | "warn" | "neutral";
  members: number;
  perms: OrgPermMatrix;
  fourEyes: boolean;
  fourEyesAbove: number;
  maskPii: boolean;
  exportReason: boolean;
  updated: string;
  updatedBy: number; // PEOPLE index
}

const countRole = (k: OrgRoleKey) => ORG_EMPLOYEES.filter((e) => e.role === k).length;

export const ORG_ROLE_META: Record<OrgRoleKey, { name: string; tone: OrgRole["tone"]; description: string }> = {
  super: { name: "Super Admin", tone: "ember", description: "Unrestricted access to every module, tenant and setting." },
  dealer: { name: "Dealer", tone: "gold", description: "Dealing desk: positions, orders, routing, quotes and spreads." },
  risk: { name: "Risk", tone: "down", description: "Exposure, A/B book, stop-outs, abuse and toxic-flow controls." },
  finance: { name: "Finance", tone: "up", description: "Deposits, withdrawals, reconciliation and adjustments." },
  compliance: { name: "Compliance / KYC", tone: "info", description: "KYC queue, AML cases, sanctions, regulatory reporting." },
  support: { name: "Support", tone: "neutral", description: "Client inbox, tickets, canned replies, read-only accounts." },
  sales: { name: "Sales", tone: "warn", description: "Leads, conversion, retention and client follow-ups." },
  ib: { name: "IB manager", tone: "gold", description: "Partner onboarding, commission plans and rebate payouts." },
  custom: { name: "Custom", tone: "neutral", description: "Hand-tuned permission set." },
};

export const ORG_ROLES: OrgRole[] = [
  ...(Object.keys(PRESET_SPECS) as Exclude<OrgRoleKey, "custom">[]).map((k, i) => ({
    key: k,
    name: ORG_ROLE_META[k].name,
    description: ORG_ROLE_META[k].description,
    preset: true,
    tone: ORG_ROLE_META[k].tone,
    members: countRole(k),
    perms: orgMatrix(PRESET_SPECS[k]),
    fourEyes: k === "finance" || k === "risk" || k === "super",
    fourEyesAbove: k === "finance" ? 10000 : 25000,
    maskPii: k === "support" || k === "sales" || k === "ib",
    exportReason: k !== "super",
    updated: ["2026-09-02", "2026-08-28", "2026-09-19", "2026-09-11", "2026-09-15", "2026-07-30", "2026-09-05", "2026-08-14"][i]!,
    updatedBy: [9, 19, 4, 12, 6, 8, 5, 7][i]!,
  })),
  {
    key: "custom-night-desk",
    name: "Night desk (custom)",
    description: "Asia-hours dealer with limited finance approvals up to $5k.",
    preset: false,
    tone: "neutral",
    members: 2,
    perms: orgMatrix({ clients: "v", trading: "vcea", config: "v", finance: "v", withdrawals: "va", support: "v", analytics: "v" }),
    fourEyes: true,
    fourEyesAbove: 5000,
    maskPii: true,
    exportReason: true,
    updated: "2026-09-21",
    updatedBy: 4,
  },
  {
    key: "custom-auditor",
    name: "External auditor (custom)",
    description: "Read-only access with exports for the annual CySEC audit.",
    preset: false,
    tone: "neutral",
    members: 1,
    perms: orgMatrix({ clients: "vx", kyc: "vx", trading: "vx", finance: "vx", withdrawals: "vx", partners: "vx", analytics: "vx", security: "vx" }),
    fourEyes: false,
    fourEyesAbove: 0,
    maskPii: true,
    exportReason: true,
    updated: "2026-09-18",
    updatedBy: 6,
  },
];

export const ORG_PRESET_TEMPLATES = (Object.keys(PRESET_SPECS) as Exclude<OrgRoleKey, "custom">[]).map((k) => ({ key: k, name: ORG_ROLE_META[k].name, perms: orgMatrix(PRESET_SPECS[k]) }));

/* ---------------------------- Desks -------------------------------- */

export interface OrgDesk {
  key: OrgDeskKey;
  name: string;
  icon: string; // Icon3D name
  lead: number; // PEOPLE index
  members: number[]; // PEOPLE indices
  headcount: number; // incl. members not on staff list (contractors)
  queue: number; // open items
  queueLabel: string;
  load: number; // 0-100
  shift: string; // GMT+3
  onShift: boolean;
  slaTarget: string;
  slaPct: number;
  languages: string[];
  region: string;
  flag: string;
  tenants: OrgTenantKey[];
  trend: number[];
}

const deskMembers = (k: OrgDeskKey) => ORG_EMPLOYEES.filter((e) => e.desk === k).map((e) => PEOPLE.indexOf(e.person));
const trend = (seed: number, base: number) => {
  const r = seeded(seed);
  return Array.from({ length: 14 }, (_, i) => Math.max(1, Math.round(base + Math.sin(i / 2) * base * 0.25 + r.range(-0.2, 0.2) * base)));
};

export const ORG_DESKS: OrgDesk[] = [
  { key: "dealing", name: "Dealing", icon: "chart_increasing", lead: 19, members: deskMembers("dealing"), headcount: 6, queue: 14, queueLabel: "requotes & manual fills", load: 62, shift: "24/5 · 3 shifts", onShift: true, slaTarget: "Fill < 150 ms", slaPct: 99.2, languages: ["EN", "TR", "JA"], region: "Global", flag: "cy", tenants: ["kalks", "aurum", "nova", "dunes"], trend: trend(11, 18) },
  { key: "risk", name: "Risk", icon: "shield", lead: 4, members: deskMembers("risk"), headcount: 4, queue: 9, queueLabel: "exposure & abuse alerts", load: 48, shift: "07:00 – 23:00", onShift: true, slaTarget: "Alert ack < 5 min", slaPct: 97.8, languages: ["EN", "HI"], region: "Global", flag: "ae", tenants: ["kalks", "aurum", "nova", "dunes"], trend: trend(12, 10) },
  { key: "finance", name: "Finance", icon: "bank", lead: 12, members: deskMembers("finance"), headcount: 5, queue: 23, queueLabel: "withdrawals pending", load: 81, shift: "08:00 – 20:00", onShift: true, slaTarget: "Payout < 4 h", slaPct: 93.4, languages: ["EN", "EL"], region: "Global", flag: "cy", tenants: ["kalks", "aurum", "nova", "dunes"], trend: trend(13, 26) },
  { key: "compliance", name: "Compliance", icon: "identification_card", lead: 6, members: deskMembers("compliance"), headcount: 7, queue: 38, queueLabel: "KYC files in queue", load: 74, shift: "08:00 – 22:00", onShift: true, slaTarget: "KYC decision < 2 h", slaPct: 91.6, languages: ["EN", "IT", "MS", "TL"], region: "Global", flag: "it", tenants: ["kalks", "aurum", "nova", "dunes"], trend: trend(14, 40) },
  { key: "support-en", name: "Support EN", icon: "speech_balloon", lead: 8, members: deskMembers("support-en"), headcount: 9, queue: 27, queueLabel: "open tickets", load: 58, shift: "24/7 · 3 shifts", onShift: true, slaTarget: "First reply < 10 min", slaPct: 96.1, languages: ["EN", "PT"], region: "Global", flag: "gb", tenants: ["kalks", "aurum", "nova"], trend: trend(15, 34) },
  { key: "support-ar", name: "Support AR", icon: "speech_balloon", lead: 20, members: deskMembers("support-ar"), headcount: 5, queue: 11, queueLabel: "open tickets", load: 44, shift: "09:00 – 01:00", onShift: true, slaTarget: "First reply < 10 min", slaPct: 98.3, languages: ["AR", "EN"], region: "MENA", flag: "ae", tenants: ["kalks", "dunes"], trend: trend(16, 14) },
  { key: "sales-mena", name: "Sales MENA", icon: "handshake", lead: 5, members: deskMembers("sales-mena"), headcount: 8, queue: 146, queueLabel: "leads assigned", load: 69, shift: "10:00 – 19:00", onShift: true, slaTarget: "Lead call < 15 min", slaPct: 88.7, languages: ["AR", "EN", "UR"], region: "MENA", flag: "sa", tenants: ["kalks", "dunes"], trend: trend(17, 120) },
  { key: "sales-asia", name: "Sales Asia", icon: "globe_with_meridians", lead: 3, members: deskMembers("sales-asia"), headcount: 6, queue: 92, queueLabel: "leads assigned", load: 37, shift: "04:00 – 13:00", onShift: false, slaTarget: "Lead call < 15 min", slaPct: 90.2, languages: ["VI", "EN", "HI"], region: "APAC", flag: "vn", tenants: ["kalks", "nova"], trend: trend(18, 80) },
  { key: "ib", name: "IB Management", icon: "busts_in_silhouette", lead: 7, members: deskMembers("ib"), headcount: 4, queue: 17, queueLabel: "partner applications", load: 52, shift: "09:00 – 18:00", onShift: true, slaTarget: "Application < 24 h", slaPct: 94.5, languages: ["EN", "PT", "ES"], region: "Global", flag: "ng", tenants: ["kalks", "aurum", "nova", "dunes"], trend: trend(19, 16) },
];

/* ----------------------------- KPIs -------------------------------- */

export interface OrgKpiRow {
  id: string;
  person: Person;
  desk: OrgDeskKey;
  role: OrgRoleKey;
  tickets: number | null;
  ftds: number | null;
  kyc: number | null;
  sla: number;
  csat: number | null;
  commission: number;
  target: number; // % of monthly target
  trend: number[];
}

const kr = seeded(4242);
export const ORG_KPIS: OrgKpiRow[] = ORG_EMPLOYEES.filter((e) => e.desk && e.status === "active" && e.role !== "super")
  .map((e, i) => {
    const d = e.desk!;
    const isSupport = d === "support-en" || d === "support-ar";
    const isSales = d === "sales-mena" || d === "sales-asia";
    const isKyc = d === "compliance";
    const isIb = d === "ib";
    const lead = ORG_DESKS.find((x) => x.key === d)?.lead === PEOPLE.indexOf(e.person);
    const tickets = isSupport ? kr.int(280, 520) - (lead ? 120 : 0) : null;
    const ftds = isSales ? kr.int(38, 96) : isIb ? kr.int(14, 40) : null;
    const kyc = isKyc ? kr.int(410, 760) - (lead ? 200 : 0) : null;
    const sla = +(isSales ? kr.range(84, 96) : kr.range(90, 99.6)).toFixed(1);
    const csat = isSupport || isSales ? +kr.range(4.35, 4.93).toFixed(2) : null;
    const commission = +(isSales ? (ftds ?? 0) * 40 + kr.range(900, 2600) : isIb ? kr.range(1800, 4200) : isSupport ? kr.range(320, 780) : isKyc ? kr.range(260, 640) : kr.range(600, 1900)).toFixed(2);
    const r = seeded(900 + i);
    return {
      id: e.id,
      person: e.person,
      desk: d,
      role: e.role,
      tickets,
      ftds,
      kyc,
      sla,
      csat,
      commission,
      target: Math.round(kr.range(64, 128)),
      trend: Array.from({ length: 12 }, (_, j) => Math.round(40 + j * 4 + r.range(-10, 12))),
    };
  })
  .sort((a, b) => b.commission - a.commission);

export interface OrgCommissionRule {
  id: string;
  desk: string;
  name: string;
  trigger: string;
  payout: string;
  cap: string;
  tone: "ember" | "gold" | "up" | "info" | "warn";
  active: boolean;
  earnedMtd: number;
}

export const ORG_COMMISSION_RULES: OrgCommissionRule[] = [
  { id: "cr_01", desk: "Sales", name: "FTD bounty", trigger: "First deposit ≥ $250 within 30 days of assignment", payout: "$40 per FTD", cap: "No cap", tone: "ember", active: true, earnedMtd: 14840 },
  { id: "cr_02", desk: "Sales", name: "Net deposit share", trigger: "Net deposits of assigned book, month-to-date", payout: "1.5% of NDA above $50k", cap: "$6,000 / month", tone: "ember", active: true, earnedMtd: 9265.5 },
  { id: "cr_03", desk: "IB Management", name: "Partner growth", trigger: "Rebate volume growth vs. prior month", payout: "5% of incremental rebates", cap: "$4,500 / month", tone: "gold", active: true, earnedMtd: 7420 },
  { id: "cr_04", desk: "Support", name: "CSAT excellence", trigger: "CSAT ≥ 4.6 with ≥ 250 rated tickets", payout: "$300 flat + $1 per ticket over 400", cap: "$900 / month", tone: "up", active: true, earnedMtd: 3970 },
  { id: "cr_05", desk: "Compliance", name: "KYC throughput", trigger: "Approved files after the first 400, zero QA reversals", payout: "$0.80 per file", cap: "$700 / month", tone: "info", active: true, earnedMtd: 2310.4 },
  { id: "cr_06", desk: "Dealing", name: "B-book desk share", trigger: "Quarterly net B-book P&L, after hedging costs", payout: "2% pooled, split by hours", cap: "Clawback on negative quarter", tone: "warn", active: false, earnedMtd: 0 },
];

export interface OrgMtdPoint {
  day: number;
  sales: number;
  ib: number;
  support: number;
  compliance: number;
}
const mr = seeded(77);
export const ORG_MTD: OrgMtdPoint[] = Array.from({ length: 24 }, (_, i) => {
  const weekend = [5, 6, 12, 13, 19, 20].includes(i);
  const f = weekend ? 0.35 : 1;
  return {
    day: i + 1,
    sales: Math.round(mr.range(700, 1400) * f),
    ib: Math.round(mr.range(220, 460) * f),
    support: Math.round(mr.range(120, 230) * (weekend ? 0.8 : 1)),
    compliance: Math.round(mr.range(70, 140) * f),
  };
});

/* ================================================================== */
/* Security                                                            */
/* ================================================================== */

function hex(seed: number, len = 64) {
  const r = seeded(seed);
  let s = "";
  while (s.length < len) s += Math.floor(r.next() * 16).toString(16);
  return s;
}

export type SecModule = "Finance" | "Trading" | "Config" | "Clients" | "KYC" | "Marketing" | "Partners" | "Prop" | "Security" | "Organization" | "Settings";
export type SecActionType = "approve" | "reject" | "edit" | "create" | "delete" | "credit" | "export" | "access";

export interface SecAuditEntry {
  id: string;
  seq: number;
  staff: number; // PEOPLE index
  module: SecModule;
  type: SecActionType;
  action: string;
  target: string;
  targetId: string;
  reason: string;
  ip: string;
  time: string;
  tenant: OrgTenantKey;
  before: Record<string, unknown> | null;
  after: Record<string, unknown> | null;
  note?: string;
  hash: string;
  prevHash: string;
}

type RawAudit = Omit<SecAuditEntry, "id" | "seq" | "hash" | "prevHash">;

const AUDIT_RAW: RawAudit[] = [
  { staff: 4, module: "Trading", type: "edit", action: "Changed leverage", target: "Account", targetId: "80412337", reason: "RISK-EXPO", ip: "94.200.18.41", time: "2026-09-24T14:27:08+03:00", tenant: "kalks", before: { leverage: "1:500", group: "real/pro-hedge", marginCallLevel: 100, stopOutLevel: 50 }, after: { leverage: "1:200", group: "real/pro-hedge", marginCallLevel: 100, stopOutLevel: 50 }, note: "Net XAUUSD exposure 42 lots vs. equity $18.4k" },
  { staff: 12, module: "Finance", type: "approve", action: "Approved withdrawal", target: "Withdrawal", targetId: "WD-771204", reason: "FIN-STD", ip: "185.44.76.12", time: "2026-09-24T14:19:52+03:00", tenant: "kalks", before: { status: "pending_review", amount: 12500, currency: "USDT", network: "TRC20", approvals: 1 }, after: { status: "approved", amount: 12500, currency: "USDT", network: "TRC20", approvals: 2, txid: "c41e9f…07ab" } },
  { staff: 19, module: "Config", type: "edit", action: "Edited spread", target: "Symbol", targetId: "XAUUSD · Pro", reason: "MKT-VOL", ip: "185.44.76.9", time: "2026-09-24T14:04:31+03:00", tenant: "aurum", before: { symbol: "XAUUSD", group: "pro", spreadMode: "floating", markupPoints: 8, minSpread: 12, swapLong: -38.4 }, after: { symbol: "XAUUSD", group: "pro", spreadMode: "floating", markupPoints: 14, minSpread: 18, swapLong: -38.4 }, note: "FOMC minutes 21:00 — widen until 22:30" },
  { staff: 5, module: "Marketing", type: "credit", action: "Credited bonus", target: "Account", targetId: "80519042", reason: "MKT-WELCOME", ip: "212.71.33.190", time: "2026-09-24T13:52:14+03:00", tenant: "dunes", before: { credit: 0, bonusCampaign: null, lotsToRelease: 0 }, after: { credit: 500, bonusCampaign: "DUNES-WELCOME-50", lotsToRelease: 10 } },
  { staff: 6, module: "KYC", type: "reject", action: "Rejected KYC document", target: "Client", targetId: "CL-448120", reason: "KYC-POA-OLD", ip: "93.43.201.7", time: "2026-09-24T13:41:03+03:00", tenant: "kalks", before: { kycLevel: 1, poaStatus: "in_review", poaDocument: "utility_bill.pdf" }, after: { kycLevel: 1, poaStatus: "rejected", poaDocument: "utility_bill.pdf", rejectionNote: "Document older than 90 days" } },
  { staff: 15, module: "Trading", type: "edit", action: "Closed position (manual)", target: "Position", targetId: "#48816203", reason: "DLR-CLIENT-REQ", ip: "88.255.102.4", time: "2026-09-24T13:30:47+03:00", tenant: "kalks", before: { ticket: 48816203, symbol: "NAS100", volume: 2.5, side: "buy", status: "open", profit: 1184.2 }, after: { ticket: 48816203, symbol: "NAS100", volume: 2.5, side: "buy", status: "closed", closePrice: 20118.4, profit: 1206.75 } },
  { staff: 9, module: "Organization", type: "edit", action: "Changed staff role", target: "Staff", targetId: "stf_2121", reason: "ORG-REORG", ip: "81.2.69.160", time: "2026-09-24T13:12:22+03:00", tenant: "kalks", before: { role: "Sales", desk: "sales-mena", tenants: ["kalks", "dunes"] }, after: { role: "IB manager", desk: "ib", tenants: ["kalks", "aurum"] } },
  { staff: 13, module: "Finance", type: "edit", action: "Balance adjustment", target: "Account", targetId: "80377215", reason: "FIN-PSP-FEE", ip: "103.21.58.77", time: "2026-09-24T12:58:09+03:00", tenant: "nova", before: { balance: 4210.55, lastAdjustment: null }, after: { balance: 4235.55, lastAdjustment: { amount: 25, comment: "Refund PSP fee — ticket T-99214" } } },
  { staff: 4, module: "Security", type: "create", action: "Added IP whitelist entry", target: "IP rule", targetId: "ipr_014", reason: "SEC-NEW-OFFICE", ip: "94.200.18.41", time: "2026-09-24T12:40:17+03:00", tenant: "kalks", before: null, after: { cidr: "94.200.18.0/24", label: "Dubai office — DIFC", scope: "all_staff", enforce: true } },
  { staff: 10, module: "KYC", type: "approve", action: "Approved KYC", target: "Client", targetId: "CL-448391", reason: "KYC-OK", ip: "175.139.4.22", time: "2026-09-24T12:31:45+03:00", tenant: "nova", before: { kycLevel: 1, status: "in_review", sanctionsHit: false, pep: false }, after: { kycLevel: 2, status: "verified", sanctionsHit: false, pep: false, depositLimit: "unlimited" } },
  { staff: 7, module: "Partners", type: "edit", action: "Changed IB commission plan", target: "Partner", targetId: "IB-20417", reason: "IB-RENEGOTIATE", ip: "105.112.9.61", time: "2026-09-24T12:05:33+03:00", tenant: "kalks", before: { plan: "Standard IB", forexPerLot: 6, metalsPerLot: 8, cpa: 0, tiers: 2 }, after: { plan: "Gold IB", forexPerLot: 8, metalsPerLot: 10, cpa: 150, tiers: 3 } },
  { staff: 19, module: "Config", type: "edit", action: "Changed swap rates", target: "Symbol", targetId: "EURUSD · all groups", reason: "CFG-LP-UPDATE", ip: "185.44.76.9", time: "2026-09-24T11:48:02+03:00", tenant: "kalks", before: { swapLong: -7.12, swapShort: 2.41, tripleSwapDay: "wednesday" }, after: { swapLong: -7.48, swapShort: 2.66, tripleSwapDay: "wednesday" } },
  { staff: 12, module: "Finance", type: "reject", action: "Rejected withdrawal", target: "Withdrawal", targetId: "WD-771188", reason: "FIN-3RD-PARTY", ip: "185.44.76.12", time: "2026-09-24T11:30:26+03:00", tenant: "aurum", before: { status: "pending_review", amount: 3800, method: "Bank wire", beneficiary: "Mohammed K." }, after: { status: "rejected", amount: 3800, method: "Bank wire", beneficiary: "Mohammed K.", rejection: "Beneficiary name mismatch" } },
  { staff: 6, module: "Clients", type: "edit", action: "Blocked client", target: "Client", targetId: "CL-441907", reason: "AML-SAR", ip: "93.43.201.7", time: "2026-09-24T11:12:40+03:00", tenant: "kalks", before: { status: "active", tradingAllowed: true, withdrawalsAllowed: true }, after: { status: "blocked", tradingAllowed: false, withdrawalsAllowed: false, amlCase: "AML-2026-0183" } },
  { staff: 17, module: "Prop", type: "edit", action: "Reset challenge", target: "Challenge", targetId: "PC-100K-38821", reason: "PROP-SERVER-ISSUE", ip: "154.160.2.11", time: "2026-09-24T10:54:18+03:00", tenant: "kalks", before: { phase: 1, status: "failed", breach: "daily_drawdown", equity: 94880 }, after: { phase: 1, status: "active", breach: null, equity: 100000, resets: 1 } },
  { staff: 8, module: "Clients", type: "edit", action: "Updated client email", target: "Client", targetId: "CL-445210", reason: "SUP-VERIFIED-REQ", ip: "116.12.48.201", time: "2026-09-24T10:31:09+03:00", tenant: "nova", before: { email: "l.nguyen@****.vn", emailVerified: true }, after: { email: "lan.nguyen@****.com", emailVerified: false } },
  { staff: 4, module: "Trading", type: "edit", action: "Moved account to A-book", target: "Account", targetId: "80298814", reason: "RISK-TOXIC", ip: "94.200.18.41", time: "2026-09-24T10:02:55+03:00", tenant: "kalks", before: { book: "B", routing: "internal", lp: null }, after: { book: "A", routing: "LP", lp: "LMAX Prime" }, note: "Latency-arb pattern: 41 trades < 2s hold" },
  { staff: 9, module: "Settings", type: "edit", action: "Changed payment method limits", target: "PSP", targetId: "USDT-TRC20", reason: "SET-LIMITS", ip: "81.2.69.160", time: "2026-09-24T09:40:13+03:00", tenant: "kalks", before: { minDeposit: 10, maxDeposit: 50000, withdrawalFee: 1, autoApproveBelow: 500 }, after: { minDeposit: 10, maxDeposit: 100000, withdrawalFee: 1, autoApproveBelow: 1000 } },
  { staff: 5, module: "Clients", type: "export", action: "Exported client list", target: "Report", targetId: "RPT-CLIENTS-MENA", reason: "EXP-CAMPAIGN", ip: "212.71.33.190", time: "2026-09-24T09:21:37+03:00", tenant: "dunes", before: null, after: { rows: 1842, columns: ["login", "country", "segment", "ftdDate"], piiMasked: true, format: "csv" } },
  { staff: 13, module: "Finance", type: "approve", action: "Approved withdrawal", target: "Withdrawal", targetId: "WD-771142", reason: "FIN-STD", ip: "103.21.58.77", time: "2026-09-24T09:05:48+03:00", tenant: "nova", before: { status: "pending_review", amount: 860, currency: "USDT", network: "TRC20" }, after: { status: "approved", amount: 860, currency: "USDT", network: "TRC20", txid: "8b20d1…c3f9" } },
  { staff: 14, module: "Trading", type: "edit", action: "Modified pending order", target: "Order", targetId: "#48811957", reason: "DLR-CLIENT-REQ", ip: "126.78.201.33", time: "2026-09-24T08:44:10+03:00", tenant: "nova", before: { type: "buy_limit", symbol: "USDJPY", price: 142.1, sl: 141.6, tp: 143.2 }, after: { type: "buy_limit", symbol: "USDJPY", price: 142.25, sl: 141.6, tp: 143.2 } },
  { staff: 20, module: "Marketing", type: "credit", action: "Credited bonus", target: "Account", targetId: "80533418", reason: "MKT-RETENTION", ip: "41.33.120.5", time: "2026-09-24T08:20:31+03:00", tenant: "dunes", before: { credit: 150, bonusCampaign: "RAMADAN-2026" }, after: { credit: 400, bonusCampaign: "RETAIN-Q3-250" } },
  { staff: 6, module: "Security", type: "edit", action: "Enabled 4-eyes approval", target: "Role", targetId: "Finance", reason: "SEC-POLICY", ip: "93.43.201.7", time: "2026-09-24T08:02:12+03:00", tenant: "kalks", before: { fourEyes: false, threshold: null }, after: { fourEyes: true, threshold: 10000 } },
  { staff: 19, module: "Config", type: "create", action: "Created symbol", target: "Symbol", targetId: "SOLUSD", reason: "CFG-NEW-PRODUCT", ip: "185.44.76.9", time: "2026-09-23T22:15:44+03:00", tenant: "kalks", before: null, after: { symbol: "SOLUSD", digits: 2, contractSize: 1, leverageCap: "1:20", sessions: "24/7", markupPoints: 45 } },
  { staff: 9, module: "Security", type: "access", action: "Force-logged out session", target: "Session", targetId: "ses_8f21a4", reason: "SEC-LOST-DEVICE", ip: "81.2.69.160", time: "2026-09-23T20:48:03+03:00", tenant: "kalks", before: { staff: "Carlos Mendoza", device: "iPhone 15 · Safari", status: "active" }, after: { staff: "Carlos Mendoza", device: "iPhone 15 · Safari", status: "revoked" } },
  { staff: 7, module: "Partners", type: "delete", action: "Removed sub-IB link", target: "Partner", targetId: "IB-20933", reason: "IB-FRAUD", ip: "105.112.9.61", time: "2026-09-23T18:32:57+03:00", tenant: "aurum", before: { parent: "IB-20417", level: 2, clients: 14 }, after: null },
  { staff: 12, module: "Finance", type: "edit", action: "Reconciled PSP batch", target: "Batch", targetId: "REC-2026-09-23", reason: "FIN-RECON", ip: "185.44.76.12", time: "2026-09-23T17:10:26+03:00", tenant: "kalks", before: { matched: 1204, unmatched: 7, variance: -312.4 }, after: { matched: 1211, unmatched: 0, variance: 0 } },
];

const sortedAudit = [...AUDIT_RAW].sort((a, b) => a.time.localeCompare(b.time));
let prev = "0000000000000000000000000000000000000000000000000000000000000000";
const chained: SecAuditEntry[] = sortedAudit.map((e, i) => {
  const seq = 1_284_310 + i;
  const h = hex(hashString(prev + e.time + e.action + e.targetId));
  const out: SecAuditEntry = { ...e, id: `aud_${seq}`, seq, hash: h, prevHash: prev };
  prev = h;
  return out;
});
export const SEC_AUDIT: SecAuditEntry[] = chained.reverse();
export const SEC_CHAIN_HEAD = SEC_AUDIT[0]!.hash;
export const SEC_CHAIN_VERIFIED_AT = "2026-09-24T14:30:00+03:00";

export const SEC_REASON_LABELS: Record<string, string> = {
  "RISK-EXPO": "Exposure limit",
  "FIN-STD": "Standard payout",
  "MKT-VOL": "Market volatility",
  "MKT-WELCOME": "Welcome campaign",
  "KYC-POA-OLD": "PoA expired",
  "DLR-CLIENT-REQ": "Client request",
  "ORG-REORG": "Team restructure",
  "FIN-PSP-FEE": "PSP fee refund",
  "SEC-NEW-OFFICE": "New office",
  "KYC-OK": "Checks passed",
  "IB-RENEGOTIATE": "Plan renegotiated",
  "CFG-LP-UPDATE": "LP rate update",
  "FIN-3RD-PARTY": "3rd-party payment",
  "AML-SAR": "SAR filed",
  "PROP-SERVER-ISSUE": "Server incident",
  "SUP-VERIFIED-REQ": "Verified request",
  "RISK-TOXIC": "Toxic flow",
  "SET-LIMITS": "Limit review",
  "EXP-CAMPAIGN": "Campaign export",
  "MKT-RETENTION": "Retention offer",
  "SEC-POLICY": "Policy change",
  "CFG-NEW-PRODUCT": "New product",
  "SEC-LOST-DEVICE": "Lost device",
  "IB-FRAUD": "Fraud suspected",
  "FIN-RECON": "Reconciliation",
};

/* ------------------------- User activity --------------------------- */

export type SecRiskFlag = "new_device" | "vpn" | "impossible_travel" | "tor" | "failed_burst" | "geo_mismatch";
export type SecUserEventType = "login" | "login_failed" | "password_reset" | "2fa_enabled" | "withdrawal_request" | "api_key_created" | "device_added";

export interface SecUserEvent {
  id: string;
  person: Person;
  login: string;
  type: SecUserEventType;
  device: "desktop" | "mobile" | "tablet";
  deviceName: string;
  browser: string;
  country: string;
  city: string;
  ip: string;
  flags: SecRiskFlag[];
  twoFa: boolean;
  time: string;
  tenant: OrgTenantKey;
  risk: number; // 0-100
}

const CITIES: [string, string, string][] = [
  ["ae", "Dubai", "94.206."], ["sa", "Riyadh", "212.26."], ["in", "Mumbai", "49.36."], ["vn", "Hanoi", "113.190."], ["my", "Kuala Lumpur", "175.143."],
  ["ng", "Lagos", "105.112."], ["br", "São Paulo", "177.92."], ["gb", "London", "86.12."], ["tr", "Istanbul", "88.230."], ["eg", "Cairo", "41.44."],
  ["pk", "Karachi", "39.40."], ["ph", "Manila", "112.198."], ["za", "Johannesburg", "102.65."], ["de", "Frankfurt", "85.214."], ["sg", "Singapore", "116.14."],
];
const DEVICES: [SecUserEvent["device"], string, string][] = [
  ["desktop", "Windows 11", "Chrome 129"], ["desktop", "macOS 15", "Safari 18"], ["mobile", "iPhone 16", "Kalks iOS 4.12"], ["mobile", "Galaxy S24", "Kalks Android 4.12"],
  ["desktop", "Windows 10", "Edge 129"], ["mobile", "Pixel 9", "Chrome 129"], ["tablet", "iPad Air", "Safari 18"], ["desktop", "Ubuntu 24.04", "Firefox 131"], ["desktop", "MetaTrader 5", "MT5 build 4620"],
];
const EVENT_W: SecUserEventType[] = ["login", "login", "login", "login", "login", "login", "login_failed", "login_failed", "password_reset", "2fa_enabled", "withdrawal_request", "withdrawal_request", "api_key_created", "device_added"];
const ur = seeded(3131);
export const SEC_USER_EVENTS: SecUserEvent[] = Array.from({ length: 64 }, (_, i) => {
  let p = PEOPLE[ur.int(0, PEOPLE.length - 1)]!;
  if (p === PEOPLE[4]) p = PEOPLE[0]!;
  const home = CITIES.find((c) => c[0] === p.country) ?? ur.pick(CITIES);
  const away = ur.bool(0.18);
  const [cc, city, pre] = away ? ur.pick(CITIES) : home;
  const [device, deviceName, browser] = ur.pick(DEVICES);
  const type = ur.pick(EVENT_W);
  const flags: SecRiskFlag[] = [];
  if (ur.bool(0.16) || type === "device_added") flags.push("new_device");
  if (ur.bool(0.12)) flags.push("vpn");
  if (away && ur.bool(0.45)) flags.push("impossible_travel");
  if (type === "login_failed" && ur.bool(0.5)) flags.push("failed_burst");
  if (away && !flags.includes("impossible_travel") && ur.bool(0.4)) flags.push("geo_mismatch");
  if (i === 7) flags.push("tor");
  const mins = Math.round(i * 11.5 + ur.range(0, 9));
  const t = new Date(Date.parse(SEC_NOW) - mins * 60000);
  const risk = Math.min(98, flags.length * 28 + (type === "login_failed" ? 12 : 0) + ur.int(2, 14));
  return {
    id: `ev_${90412 - i}`,
    person: p,
    login: String(80200000 + ur.int(10000, 399999)),
    type,
    device,
    deviceName,
    browser,
    country: cc,
    city,
    ip: `${pre}${ur.int(1, 254)}.${ur.int(1, 254)}`,
    flags,
    twoFa: ur.bool(0.72),
    time: t.toISOString(),
    tenant: ur.pick(["kalks", "kalks", "aurum", "nova", "dunes"] as const),
    risk,
  };
});

const hr = seeded(515);
/** Hourly activity for the last 24h (GMT+3), oldest first. */
export const SEC_ACTIVITY_HOURLY = Array.from({ length: 24 }, (_, i) => {
  const hour = (15 + i) % 24; // ends at 14:00
  const curve = 0.45 + 0.55 * Math.sin(((hour - 4) / 24) * Math.PI * 2 - Math.PI / 2 + Math.PI) ** 2;
  const logins = Math.round(820 + curve * 1650 + hr.range(-120, 120));
  return { hour: `${String(hour).padStart(2, "0")}:00`, logins, failed: Math.round(logins * hr.range(0.018, 0.045)), flagged: Math.round(hr.range(3, 22) * curve + 2) };
});

export const SEC_USER_STATS = {
  logins24h: SEC_ACTIVITY_HOURLY.reduce((s, h) => s + h.logins, 0),
  failed24h: SEC_ACTIVITY_HOURLY.reduce((s, h) => s + h.failed, 0),
  flagged24h: SEC_ACTIVITY_HOURLY.reduce((s, h) => s + h.flagged, 0),
  twoFaAdoption: 71.4,
  uniqueDevices: 18_422,
  flagMix: [
    { flag: "new_device" as SecRiskFlag, count: 142 },
    { flag: "vpn" as SecRiskFlag, count: 88 },
    { flag: "geo_mismatch" as SecRiskFlag, count: 51 },
    { flag: "failed_burst" as SecRiskFlag, count: 37 },
    { flag: "impossible_travel" as SecRiskFlag, count: 14 },
    { flag: "tor" as SecRiskFlag, count: 3 },
  ],
  geo: [
    { country: "ae", count: 4210, label: "UAE" },
    { country: "in", count: 3890, label: "India" },
    { country: "sa", count: 2740, label: "Saudi Arabia" },
    { country: "vn", count: 2310, label: "Vietnam" },
    { country: "my", count: 1620, label: "Malaysia" },
    { country: "ng", count: 1480, label: "Nigeria" },
    { country: "br", count: 1210, label: "Brazil" },
    { country: "eg", count: 980, label: "Egypt" },
    { country: "gb", count: 760, label: "United Kingdom" },
    { country: "tr", count: 690, label: "Türkiye" },
  ],
};

/* ---------------------------- Sessions ----------------------------- */

export interface SecSession {
  id: string;
  staff: number; // PEOPLE index
  device: "desktop" | "mobile" | "tablet";
  deviceName: string;
  browser: string;
  ip: string;
  city: string;
  country: string;
  started: string;
  lastSeen: string;
  current?: boolean;
  mfa: OrgTwoFa;
  risk: "ok" | "watch" | "high";
  riskNote?: string;
  tenant: OrgTenantKey;
}

export const SEC_SESSIONS: SecSession[] = [
  { id: "ses_9a41c2", staff: 4, device: "desktop", deviceName: "MacBook Pro 16", browser: "Chrome 129", ip: "94.200.18.41", city: "Dubai", country: "ae", started: "2026-09-24T08:02:11+03:00", lastSeen: "2026-09-24T14:32:00+03:00", current: true, mfa: "hardware", risk: "ok", tenant: "kalks" },
  { id: "ses_9a3f07", staff: 4, device: "mobile", deviceName: "iPhone 16 Pro", browser: "Kalks Admin iOS 2.4", ip: "5.195.64.12", city: "Dubai", country: "ae", started: "2026-09-23T21:40:55+03:00", lastSeen: "2026-09-24T07:18:02+03:00", mfa: "hardware", risk: "ok", tenant: "kalks" },
  { id: "ses_9a40d8", staff: 19, device: "desktop", deviceName: "Windows 11 workstation", browser: "Edge 129", ip: "185.44.76.9", city: "Limassol", country: "cy", started: "2026-09-24T06:55:40+03:00", lastSeen: "2026-09-24T14:30:48+03:00", mfa: "hardware", risk: "ok", tenant: "kalks" },
  { id: "ses_9a40e1", staff: 12, device: "desktop", deviceName: "MacBook Air 13", browser: "Safari 18", ip: "185.44.76.12", city: "Limassol", country: "cy", started: "2026-09-24T07:58:03+03:00", lastSeen: "2026-09-24T14:26:10+03:00", mfa: "hardware", risk: "ok", tenant: "kalks" },
  { id: "ses_9a4102", staff: 15, device: "desktop", deviceName: "Windows 11 workstation", browser: "Chrome 129", ip: "88.255.102.4", city: "Istanbul", country: "tr", started: "2026-09-24T08:30:17+03:00", lastSeen: "2026-09-24T14:28:33+03:00", mfa: "totp", risk: "ok", tenant: "aurum" },
  { id: "ses_9a4118", staff: 10, device: "desktop", deviceName: "ThinkPad X1", browser: "Chrome 129", ip: "175.139.4.22", city: "Kuala Lumpur", country: "my", started: "2026-09-24T05:02:44+03:00", lastSeen: "2026-09-24T14:31:12+03:00", mfa: "totp", risk: "ok", tenant: "nova" },
  { id: "ses_9a4120", staff: 8, device: "desktop", deviceName: "MacBook Pro 14", browser: "Arc 1.62", ip: "116.12.48.201", city: "Singapore", country: "sg", started: "2026-09-24T04:48:20+03:00", lastSeen: "2026-09-24T14:29:05+03:00", mfa: "totp", risk: "ok", tenant: "nova" },
  { id: "ses_9a412d", staff: 5, device: "tablet", deviceName: "iPad Pro 13", browser: "Safari 18", ip: "212.71.33.190", city: "Riyadh", country: "sa", started: "2026-09-24T10:14:52+03:00", lastSeen: "2026-09-24T14:19:40+03:00", mfa: "hardware", risk: "ok", tenant: "dunes" },
  { id: "ses_9a4133", staff: 23, device: "desktop", deviceName: "Windows 10 laptop", browser: "Chrome 128", ip: "102.65.141.9", city: "Johannesburg", country: "za", started: "2026-09-24T09:06:31+03:00", lastSeen: "2026-09-24T14:17:22+03:00", mfa: "sms", risk: "watch", riskNote: "Outdated browser · SMS 2FA", tenant: "kalks" },
  { id: "ses_9a4140", staff: 7, device: "desktop", deviceName: "MacBook Pro 16", browser: "Chrome 129", ip: "105.112.9.61", city: "Lagos", country: "ng", started: "2026-09-24T09:30:05+03:00", lastSeen: "2026-09-24T14:08:51+03:00", mfa: "hardware", risk: "ok", tenant: "kalks" },
  { id: "ses_9a4152", staff: 13, device: "desktop", deviceName: "Windows 11 laptop", browser: "Chrome 129", ip: "103.21.58.77", city: "Mumbai", country: "in", started: "2026-09-24T06:12:48+03:00", lastSeen: "2026-09-24T13:58:30+03:00", mfa: "totp", risk: "ok", tenant: "nova" },
  { id: "ses_9a4161", staff: 1, device: "mobile", deviceName: "Galaxy S24", browser: "Chrome 129", ip: "185.220.101.44", city: "Frankfurt", country: "de", started: "2026-09-24T13:40:09+03:00", lastSeen: "2026-09-24T14:02:15+03:00", mfa: "sms", risk: "high", riskNote: "VPN exit node · 3,100 km from last login", tenant: "dunes" },
  { id: "ses_9a4170", staff: 20, device: "desktop", deviceName: "Dell OptiPlex", browser: "Firefox 131", ip: "41.33.120.5", city: "Cairo", country: "eg", started: "2026-09-24T09:01:27+03:00", lastSeen: "2026-09-24T14:24:44+03:00", mfa: "totp", risk: "ok", tenant: "dunes" },
  { id: "ses_9a4185", staff: 9, device: "desktop", deviceName: "MacBook Pro 14", browser: "Safari 18", ip: "81.2.69.160", city: "London", country: "gb", started: "2026-09-24T09:47:02+03:00", lastSeen: "2026-09-24T14:21:37+03:00", mfa: "hardware", risk: "ok", tenant: "kalks" },
  { id: "ses_9a4193", staff: 17, device: "desktop", deviceName: "ThinkPad T14", browser: "Chrome 129", ip: "154.160.2.11", city: "Accra", country: "gh", started: "2026-09-24T08:15:59+03:00", lastSeen: "2026-09-24T14:12:04+03:00", mfa: "totp", risk: "watch", riskNote: "IP outside whitelist (grace mode)", tenant: "kalks" },
];

/* ---------------------------- IP rules ----------------------------- */

export interface SecIpRule {
  id: string;
  cidr: string;
  label: string;
  scope: "all" | "role" | "user";
  scopeValue?: string;
  addedBy: number; // PEOPLE index
  added: string;
  lastHit: string | null;
  hits24h: number;
  enabled: boolean;
  country: string;
}

export const SEC_CURRENT_IP = "94.200.18.41";

export const SEC_IP_RULES: SecIpRule[] = [
  { id: "ipr_001", cidr: "185.44.76.0/24", label: "Limassol HQ — main floor", scope: "all", addedBy: 9, added: "2025-11-03", lastHit: "2026-09-24T14:30:48+03:00", hits24h: 18422, enabled: true, country: "cy" },
  { id: "ipr_014", cidr: "94.200.18.0/24", label: "Dubai office — DIFC", scope: "all", addedBy: 4, added: "2026-09-24", lastHit: "2026-09-24T14:32:00+03:00", hits24h: 2310, enabled: true, country: "ae" },
  { id: "ipr_002", cidr: "81.2.69.160/28", label: "London exec office", scope: "role", scopeValue: "Super Admin", addedBy: 9, added: "2025-11-03", lastHit: "2026-09-24T14:21:37+03:00", hits24h: 1204, enabled: true, country: "gb" },
  { id: "ipr_003", cidr: "88.255.102.0/26", label: "Istanbul dealing room", scope: "role", scopeValue: "Dealer", addedBy: 19, added: "2026-01-19", lastHit: "2026-09-24T14:28:33+03:00", hits24h: 6120, enabled: true, country: "tr" },
  { id: "ipr_004", cidr: "116.12.48.0/24", label: "Singapore support hub", scope: "role", scopeValue: "Support", addedBy: 8, added: "2026-02-07", lastHit: "2026-09-24T14:29:05+03:00", hits24h: 4410, enabled: true, country: "sg" },
  { id: "ipr_005", cidr: "175.139.4.0/24", label: "KL compliance centre", scope: "role", scopeValue: "Compliance / KYC", addedBy: 6, added: "2026-03-12", lastHit: "2026-09-24T14:31:12+03:00", hits24h: 3890, enabled: true, country: "my" },
  { id: "ipr_006", cidr: "41.33.120.0/25", label: "Cairo Arabic support", scope: "role", scopeValue: "Support", addedBy: 20, added: "2026-04-22", lastHit: "2026-09-24T14:24:44+03:00", hits24h: 1740, enabled: true, country: "eg" },
  { id: "ipr_007", cidr: "212.71.33.190/32", label: "Omar Haddad — home fibre", scope: "user", scopeValue: "Omar Haddad", addedBy: 9, added: "2026-05-30", lastHit: "2026-09-24T14:19:40+03:00", hits24h: 612, enabled: true, country: "sa" },
  { id: "ipr_008", cidr: "103.21.58.77/32", label: "Rahul Verma — remote", scope: "user", scopeValue: "Rahul Verma", addedBy: 12, added: "2026-06-18", lastHit: "2026-09-24T13:58:30+03:00", hits24h: 488, enabled: true, country: "in" },
  { id: "ipr_009", cidr: "10.40.0.0/16", label: "Corporate VPN (WireGuard)", scope: "all", addedBy: 9, added: "2025-11-03", lastHit: "2026-09-24T14:27:12+03:00", hits24h: 9204, enabled: true, country: "cy" },
  { id: "ipr_010", cidr: "105.112.9.0/24", label: "Lagos partnerships office", scope: "role", scopeValue: "IB manager", addedBy: 7, added: "2026-07-01", lastHit: "2026-09-24T14:08:51+03:00", hits24h: 902, enabled: true, country: "ng" },
  { id: "ipr_011", cidr: "126.78.201.0/27", label: "Tokyo night desk", scope: "user", scopeValue: "Yuki Tanaka", addedBy: 19, added: "2026-08-08", lastHit: "2026-09-24T09:02:00+03:00", hits24h: 1316, enabled: true, country: "jp" },
  { id: "ipr_012", cidr: "177.92.14.0/24", label: "São Paulo co-working (trial)", scope: "role", scopeValue: "IB manager", addedBy: 7, added: "2026-09-02", lastHit: "2026-09-19T16:40:22+03:00", hits24h: 0, enabled: false, country: "br" },
  { id: "ipr_013", cidr: "2a02:c7c:4410::/48", label: "London exec office (IPv6)", scope: "role", scopeValue: "Super Admin", addedBy: 9, added: "2026-02-15", lastHit: null, hits24h: 0, enabled: true, country: "gb" },
];

export const SEC_BLOCKED_ATTEMPTS = [
  { ip: "154.160.2.11", staff: 17, time: "2026-09-24T08:15:59+03:00", result: "grace" as const, city: "Accra", country: "gh" },
  { ip: "185.220.101.44", staff: 1, time: "2026-09-24T13:40:09+03:00", result: "grace" as const, city: "Frankfurt", country: "de" },
  { ip: "45.83.64.201", staff: 11, time: "2026-09-24T03:12:40+03:00", result: "blocked" as const, city: "Amsterdam", country: "nl" },
  { ip: "91.219.236.18", staff: 11, time: "2026-09-23T23:58:04+03:00", result: "blocked" as const, city: "Kyiv", country: "ua" },
  { ip: "197.210.53.7", staff: 2, time: "2026-09-23T19:21:36+03:00", result: "blocked" as const, city: "Lagos", country: "ng" },
];
