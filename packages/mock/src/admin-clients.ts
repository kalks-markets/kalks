/**
 * Back Office — clients, CRM, compliance mock data.
 * Deterministic (seeded) so server and client renders match.
 */
import { seeded, hashString } from "./rng";
import { PEOPLE } from "./people";
import { INSTRUMENTS } from "./symbols";

/** Fixed "now" for the back-office mocks: 24 Sep 2026, 18:40 server time (GMT+3). */
export const ADMIN_NOW = Date.parse("2026-09-24T15:40:00Z");
const MIN = 60_000;
const HOUR = 3_600_000;
const DAY = 86_400_000;

/* ------------------------------------------------------------------ */
/* Staff, desks                                                        */
/* ------------------------------------------------------------------ */

export interface StaffMember {
  id: string;
  name: string;
  role: string;
  desk: string;
  photo: string;
  email: string;
}

export const DESKS = ["Sales · EN", "Sales · MENA", "Sales · LATAM", "Sales · APAC", "Retention · A", "Retention · B"] as const;
export type Desk = (typeof DESKS)[number];

export const STAFF_MEMBERS: StaffMember[] = [
  { id: "ST-101", name: "Sara Whitfield", role: "Senior sales agent", desk: "Sales · EN", photo: "/assets/people/women-12.jpg", email: "sara.w@kalkstrade.com" },
  { id: "ST-102", name: "Karim Nasser", role: "Sales agent", desk: "Sales · MENA", photo: "/assets/people/men-52.jpg", email: "karim.n@kalkstrade.com" },
  { id: "ST-103", name: "Gabriela Ruiz", role: "Sales agent", desk: "Sales · LATAM", photo: "/assets/people/women-81.jpg", email: "gabriela.r@kalkstrade.com" },
  { id: "ST-104", name: "Wei Zhang", role: "Sales agent", desk: "Sales · APAC", photo: "/assets/people/men-29.jpg", email: "wei.z@kalkstrade.com" },
  { id: "ST-105", name: "Hannah Scott", role: "Retention manager", desk: "Retention · A", photo: "/assets/people/women-32.jpg", email: "hannah.s@kalkstrade.com" },
  { id: "ST-106", name: "Marcus Dlamini", role: "Retention agent", desk: "Retention · B", photo: "/assets/people/men-68.jpg", email: "marcus.d@kalkstrade.com" },
  { id: "ST-107", name: "Noura Khalid", role: "Compliance officer", desk: "Compliance", photo: "/assets/people/women-11.jpg", email: "noura.k@kalkstrade.com" },
  { id: "ST-108", name: "Ravi Menon", role: "AML analyst", desk: "Compliance", photo: "/assets/people/men-65.jpg", email: "ravi.m@kalkstrade.com" },
  { id: "ST-109", name: "Julia Novak", role: "Dealer", desk: "Dealing", photo: "/assets/people/women-29.jpg", email: "julia.n@kalkstrade.com" },
  { id: "ST-110", name: "Tom Becker", role: "Finance officer", desk: "Finance", photo: "/assets/people/men-12.jpg", email: "tom.b@kalkstrade.com" },
];

export const SALES_AGENTS = STAFF_MEMBERS.slice(0, 6);
export const COMPLIANCE_STAFF = STAFF_MEMBERS.filter((s) => s.desk === "Compliance");

export function staff(id: string): StaffMember {
  return STAFF_MEMBERS.find((s) => s.id === id) ?? STAFF_MEMBERS[0]!;
}

/* ------------------------------------------------------------------ */
/* Clients                                                             */
/* ------------------------------------------------------------------ */

export type KycStatus = "verified" | "pending" | "review" | "rejected" | "none";
export type ClientStatus = "active" | "inactive" | "blocked";

export interface AdminClient {
  id: string; // numeric string, e.g. 100482
  name: string;
  email: string;
  phone: string;
  country: string;
  countryName: string;
  photo: string;
  gender: "m" | "f";
  kyc: KycStatus;
  kycLevel: 0 | 1 | 2 | 3;
  accounts: number;
  logins: string[];
  equity: number;
  balance: number;
  credit: number;
  deposits: number;
  withdrawals: number;
  net: number;
  ib: { id: string; name: string } | null;
  desk: Desk;
  agentId: string;
  risk: number; // 1..10
  tags: string[];
  lastLogin: string; // ISO
  registered: string; // ISO
  ftd: string | null;
  status: ClientStatus;
  funded: boolean;
  group: string;
  source: string;
  wallet: string; // TRC20 address
  lifetimeLots: number;
  tradingDisabled: boolean;
}

const COUNTRIES: { cc: string; name: string; m: string[]; f: string[]; last: string[]; phone: string; desk: Desk }[] = [
  { cc: "in", name: "India", m: ["Rohan", "Nikhil", "Aditya", "Karan", "Siddharth"], f: ["Ananya", "Divya", "Kavya", "Meera", "Riya"], last: ["Sharma", "Kapoor", "Reddy", "Gupta", "Joshi", "Bose"], phone: "+91 98", desk: "Sales · APAC" },
  { cc: "ae", name: "United Arab Emirates", m: ["Khalid", "Saeed", "Rashid", "Hamdan"], f: ["Mariam", "Noor", "Hessa", "Latifa"], last: ["Al-Mansoori", "Al-Nuaimi", "Al-Falasi", "Al-Suwaidi"], phone: "+971 50", desk: "Sales · MENA" },
  { cc: "sa", name: "Saudi Arabia", m: ["Faisal", "Abdullah", "Turki", "Majed"], f: ["Reem", "Lama", "Nouf", "Sara"], last: ["Al-Qahtani", "Al-Harbi", "Al-Otaibi", "Al-Ghamdi"], phone: "+966 55", desk: "Sales · MENA" },
  { cc: "br", name: "Brazil", m: ["Mateus", "Gabriel", "Rafael", "Thiago"], f: ["Beatriz", "Larissa", "Camila", "Juliana"], last: ["Oliveira", "Costa", "Almeida", "Ribeiro", "Carvalho"], phone: "+55 11", desk: "Sales · LATAM" },
  { cc: "mx", name: "Mexico", m: ["Diego", "Alejandro", "Emiliano"], f: ["Valeria", "Ximena", "Fernanda"], last: ["Hernández", "García", "Ramírez", "Torres"], phone: "+52 55", desk: "Sales · LATAM" },
  { cc: "vn", name: "Vietnam", m: ["Minh Quan", "Duc Anh", "Hoang Nam"], f: ["Thu Trang", "Lan Anh", "Bao Ngoc"], last: ["Nguyen", "Tran", "Pham", "Le"], phone: "+84 90", desk: "Sales · APAC" },
  { cc: "my", name: "Malaysia", m: ["Hafiz", "Irfan", "Danial"], f: ["Nurul", "Aina", "Syafiqah"], last: ["Ismail", "Rahman", "Hassan", "Yusof"], phone: "+60 12", desk: "Sales · APAC" },
  { cc: "ng", name: "Nigeria", m: ["Chinedu", "Tunde", "Emeka", "Seun"], f: ["Adaeze", "Funmi", "Ngozi", "Temi"], last: ["Adeyemi", "Okonkwo", "Balogun", "Eze"], phone: "+234 80", desk: "Sales · EN" },
  { cc: "za", name: "South Africa", m: ["Thabo", "Sipho", "Liam"], f: ["Naledi", "Lerato", "Zanele"], last: ["Nkosi", "Botha", "Mokoena", "van Wyk"], phone: "+27 82", desk: "Sales · EN" },
  { cc: "gb", name: "United Kingdom", m: ["Oliver", "Harry", "George"], f: ["Amelia", "Charlotte", "Isla"], last: ["Hughes", "Walker", "Bennett", "Clarke"], phone: "+44 77", desk: "Sales · EN" },
  { cc: "de", name: "Germany", m: ["Lukas", "Felix", "Jonas"], f: ["Lena", "Hannah", "Mia"], last: ["Schneider", "Fischer", "Weber", "Wagner"], phone: "+49 151", desk: "Sales · EN" },
  { cc: "tr", name: "Türkiye", m: ["Emre", "Burak", "Mert"], f: ["Elif", "Zeynep", "Ece"], last: ["Yılmaz", "Kaya", "Demir", "Şahin"], phone: "+90 532", desk: "Sales · MENA" },
  { cc: "eg", name: "Egypt", m: ["Mostafa", "Youssef", "Karim"], f: ["Salma", "Nour", "Habiba"], last: ["Mahmoud", "Ibrahim", "Fathy", "Soliman"], phone: "+20 100", desk: "Sales · MENA" },
  { cc: "ph", name: "Philippines", m: ["Paolo", "Miguel", "Joshua"], f: ["Andrea", "Bea", "Kristine"], last: ["Santos", "Reyes", "Bautista", "Garcia"], phone: "+63 917", desk: "Sales · APAC" },
  { cc: "pk", name: "Pakistan", m: ["Bilal", "Hamza", "Usman"], f: ["Hira", "Ayesha", "Mahnoor"], last: ["Khan", "Malik", "Qureshi", "Chaudhry"], phone: "+92 300", desk: "Sales · APAC" },
  { cc: "gh", name: "Ghana", m: ["Kofi", "Yaw", "Kwabena"], f: ["Akosua", "Ama", "Efua"], last: ["Boateng", "Owusu", "Asante", "Appiah"], phone: "+233 24", desk: "Sales · EN" },
  { cc: "cy", name: "Cyprus", m: ["Andreas", "Nikos", "Christos"], f: ["Eleni", "Maria", "Sofia"], last: ["Georgiou", "Christodoulou", "Charalambous"], phone: "+357 99", desk: "Sales · EN" },
  { cc: "it", name: "Italy", m: ["Lorenzo", "Matteo", "Alessandro"], f: ["Giulia", "Chiara", "Francesca"], last: ["Bianchi", "Romano", "Colombo", "Ricci"], phone: "+39 347", desk: "Sales · EN" },
];

const MEN = [11, 12, 22, 29, 32, 44, 52, 65, 68, 75, 81, 86].map((n) => `/assets/people/men-${n}.jpg`);
const WOMEN = [11, 12, 22, 29, 32, 44, 52, 65, 68, 75, 81, 86].map((n) => `/assets/people/women-${n}.jpg`);

export const CLIENT_TAGS = ["VIP", "High value", "Scalper", "Swing", "Copy master", "IB", "Bonus abuser?", "Churn risk", "Whale", "News trader", "EA user", "Prop"] as const;
export const CLIENT_GROUPS = ["Standard", "Pro", "ECN", "Cent", "VIP"] as const;
const SOURCES = ["Google Ads", "Meta · IG", "Organic", "IB referral", "YouTube", "TikTok", "Affiliate · CPA", "Telegram"];

export const IB_PARTNERS = [
  { id: "IB-2201", name: "Omar Haddad", photo: "/assets/people/men-52.jpg", country: "sa" },
  { id: "IB-2207", name: "Priya Nair", photo: "/assets/people/women-68.jpg", country: "in" },
  { id: "IB-2214", name: "Daniel Okafor", photo: "/assets/people/men-75.jpg", country: "ng" },
  { id: "IB-2230", name: "Mei Lin", photo: "/assets/people/women-29.jpg", country: "sg" },
  { id: "IB-2241", name: "Carlos Mendoza", photo: "/assets/people/men-44.jpg", country: "mx" },
];

const B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
export function trcAddress(seed: number) {
  const r = seeded(seed);
  let s = "T";
  for (let i = 0; i < 33; i++) s += B58[r.int(0, B58.length - 1)];
  return s;
}

function iso(t: number) {
  return new Date(t).toISOString();
}

export const CLIENTS: AdminClient[] = (() => {
  const r = seeded(41027);
  const out: AdminClient[] = [];
  for (let i = 0; i < 152; i++) {
    let name: string, cc: string, countryName: string, photo: string, gender: "m" | "f", phonePrefix: string, desk: Desk;
    if (i < PEOPLE.length) {
      const p = PEOPLE[i]!;
      name = p.name;
      cc = p.country;
      countryName = p.countryName;
      photo = p.photo;
      gender = p.photo.includes("women") ? "f" : "m";
      const c = COUNTRIES.find((x) => x.cc === cc);
      phonePrefix = c?.phone ?? "+65 81";
      desk = c?.desk ?? "Sales · APAC";
    } else {
      const c = COUNTRIES[r.int(0, COUNTRIES.length - 1)]!;
      gender = r.bool() ? "m" : "f";
      name = `${r.pick(gender === "m" ? c.m : c.f)} ${r.pick(c.last)}`;
      cc = c.cc;
      countryName = c.name;
      photo = r.pick(gender === "m" ? MEN : WOMEN);
      phonePrefix = c.phone;
      desk = c.desk;
    }
    if (r.bool(0.28)) desk = r.pick(["Retention · A", "Retention · B"] as const);
    const agent = SALES_AGENTS.find((a) => a.desk === desk) ?? SALES_AGENTS[0]!;
    const kycRoll = r.next();
    const kyc: KycStatus = i === 0 ? "verified" : kycRoll < 0.62 ? "verified" : kycRoll < 0.74 ? "pending" : kycRoll < 0.82 ? "review" : kycRoll < 0.87 ? "rejected" : "none";
    const funded = kyc === "verified" ? r.bool(0.86) : kyc === "pending" || kyc === "review" ? r.bool(0.35) : false;
    const deposits = i === 0 ? 62480 : funded ? Math.round(Math.exp(r.range(5.3, 11.2))) : 0;
    const withdrawals = funded ? Math.round(deposits * r.range(0, 0.72)) : 0;
    const net = deposits - withdrawals;
    const pnlFactor = r.range(0.35, 1.45);
    const equity = i === 0 ? 48915.6 : funded ? +(net * pnlFactor).toFixed(2) : 0;
    const accounts = funded ? r.int(1, 4) : r.int(0, 1);
    const logins = Array.from({ length: Math.max(accounts, funded ? 1 : 0) }, (_, k) => String(80410000 + ((hashString(`${i}-${k}`) % 89999) + 100)));
    if (i === 0) logins.splice(0, logins.length, "80412337", "80412512", "80413001");
    const risk = i === 0 ? 3 : Math.max(1, Math.min(10, Math.round(r.range(1, 7) + (r.bool(0.12) ? r.range(2, 4) : 0))));
    const tags: string[] = [];
    if (deposits > 25000) tags.push("VIP");
    if (deposits > 10000 && !tags.includes("VIP")) tags.push("High value");
    if (risk >= 8) tags.push("Scalper");
    if (r.bool(0.12)) tags.push(r.pick(["Swing", "News trader", "EA user", "Copy master", "Churn risk"]));
    if (r.bool(0.05)) tags.push("Bonus abuser?");
    const registered = ADMIN_NOW - r.int(1, 720) * DAY - r.int(0, 23) * HOUR;
    const lastLogin = ADMIN_NOW - (r.bool(0.55) ? r.int(1, 600) * MIN : r.int(1, 90) * DAY);
    const status: ClientStatus = r.bool(0.03) ? "blocked" : ADMIN_NOW - lastLogin > 30 * DAY ? "inactive" : "active";
    const ibRoll = r.next();
    const ib = ibRoll < 0.42 ? IB_PARTNERS[Math.floor(ibRoll * 100) % IB_PARTNERS.length]! : null;
    const first = name.split(" ")[0]!.toLowerCase().replace(/[^a-z]/g, "");
    const last = name.split(" ").slice(-1)[0]!.toLowerCase().replace(/[^a-z]/g, "");
    out.push({
      id: String(100400 + i * 7 + (i % 5)),
      name,
      email: i < PEOPLE.length ? PEOPLE[i]!.email : `${first}.${last}${r.bool(0.4) ? r.int(1, 99) : ""}@${r.pick(["gmail.com", "outlook.com", "yahoo.com", "icloud.com", "proton.me"])}`,
      phone: `${phonePrefix} ${r.int(100, 999)} ${r.int(1000, 9999)}`,
      country: cc,
      countryName,
      photo,
      gender,
      kyc,
      kycLevel: kyc === "verified" ? (deposits > 20000 ? 3 : 2) : kyc === "none" ? 0 : 1,
      accounts,
      logins,
      equity,
      balance: +(equity * r.range(0.92, 1.06)).toFixed(2),
      credit: r.bool(0.1) ? r.pick([100, 250, 500]) : 0,
      deposits,
      withdrawals,
      net,
      ib: ib ? { id: ib.id, name: ib.name } : null,
      desk,
      agentId: agent.id,
      risk,
      tags,
      lastLogin: iso(lastLogin),
      registered: iso(registered),
      ftd: funded ? iso(registered + r.int(0, 20) * DAY) : null,
      status,
      funded,
      group: i === 0 ? "Pro" : deposits > 25000 ? "VIP" : r.pick(CLIENT_GROUPS.slice(0, 4)),
      source: ib ? "IB referral" : r.pick(SOURCES),
      wallet: trcAddress(hashString("w" + i)),
      lifetimeLots: funded ? +r.range(2, 2400).toFixed(1) : 0,
      tradingDisabled: status === "blocked" || r.bool(0.02),
    });
  }
  return out;
})();

export function getClient(id: string): AdminClient {
  return CLIENTS.find((c) => c.id === id) ?? CLIENTS[0]!;
}

export function clientByLogin(login: string): AdminClient | undefined {
  return CLIENTS.find((c) => c.logins.includes(login));
}

/* ------------------------------------------------------------------ */
/* Client 360 detail                                                   */
/* ------------------------------------------------------------------ */

export interface ClientAccount {
  login: string;
  type: "live" | "demo";
  group: string;
  mode: "hedging" | "netting";
  leverage: number;
  currency: "USD" | "USC";
  balance: number;
  equity: number;
  credit: number;
  margin: number;
  server: string;
  route: "A" | "B";
  openPositions: number;
  created: string;
}

export function clientAccounts(c: AdminClient): ClientAccount[] {
  const r = seeded(hashString("acc" + c.id));
  const split = c.logins.map(() => r.range(0.2, 1));
  const sum = split.reduce((a, b) => a + b, 0) || 1;
  const live: ClientAccount[] = c.logins.map((login, k) => {
    const eq = +((c.equity * split[k]!) / sum).toFixed(2);
    const margin = eq > 0 ? +(eq * r.range(0.02, 0.4)).toFixed(2) : 0;
    const group = k === 0 ? c.group : r.pick(["Standard", "Pro", "ECN", "Cent"]);
    const cent = group === "Cent";
    return {
      login,
      type: "live",
      group,
      mode: r.bool(0.7) ? "hedging" : "netting",
      leverage: r.pick([100, 200, 500, 1000]),
      currency: cent ? "USC" : "USD",
      balance: +(eq * r.range(0.95, 1.05)).toFixed(2),
      equity: eq,
      credit: k === 0 ? c.credit : 0,
      margin,
      server: r.bool(0.7) ? "Kalks-Live01" : "Kalks-Live02",
      route: c.risk >= 8 ? "A" : "B",
      openPositions: eq > 0 ? r.int(0, 7) : 0,
      created: new Date(Date.parse(c.registered) + k * 9 * DAY).toISOString(),
    };
  });
  const demo: ClientAccount = {
    login: String(90020000 + (hashString("d" + c.id) % 9999)),
    type: "demo",
    group: "Pro",
    mode: "hedging",
    leverage: 500,
    currency: "USD",
    balance: 100000,
    equity: +(100000 * r.range(0.9, 1.08)).toFixed(2),
    credit: 0,
    margin: +r.range(200, 3000).toFixed(2),
    server: "Kalks-Demo",
    route: "B",
    openPositions: r.int(0, 3),
    created: c.registered,
  };
  return [...live, demo];
}

export interface ClientTrade {
  ticket: string;
  login: string;
  symbol: string;
  side: "buy" | "sell";
  volume: number;
  openPrice: number;
  closePrice: number;
  openTime: string;
  closeTime: string;
  profit: number;
  holdSec: number;
}

export function clientTrades(c: AdminClient, n = 40): ClientTrade[] {
  const r = seeded(hashString("trd" + c.id));
  if (!c.funded) return [];
  const syms = ["XAUUSD", "EURUSD", "GBPUSD", "NAS100", "BTCUSD", "USDJPY", "US30", "ETHUSD", "USOIL"];
  let t = ADMIN_NOW - r.int(10, 200) * MIN;
  return Array.from({ length: n }, (_, k) => {
    const symbol = r.pick(syms);
    const inst = INSTRUMENTS.find((x) => x.symbol === symbol)!;
    const side = r.bool() ? "buy" : "sell";
    const volume = +(c.risk >= 8 ? r.range(2, 12) : r.range(0.05, 2.5)).toFixed(2);
    const holdSec = c.risk >= 8 ? r.int(8, 90) : r.int(120, 60 * 60 * 30);
    const openPrice = +(inst.price * (1 + r.normal() * 0.004)).toFixed(inst.digits);
    const move = r.normal() * inst.price * 0.0016 + (c.risk >= 8 ? inst.price * 0.0004 : 0);
    const closePrice = +(openPrice + (side === "buy" ? move : -move)).toFixed(inst.digits);
    let profit = (side === "buy" ? closePrice - openPrice : openPrice - closePrice) * volume * inst.contractSize;
    if (symbol.endsWith("JPY")) profit /= closePrice;
    const closeTime = t;
    t -= r.int(20, 900) * MIN;
    return {
      ticket: String(49430000 - k * 113 - (hashString(c.id) % 900)),
      login: r.pick(c.logins.length ? c.logins : ["80400000"]),
      symbol,
      side,
      volume,
      openPrice,
      closePrice,
      openTime: iso(closeTime - holdSec * 1000),
      closeTime: iso(closeTime),
      profit: +profit.toFixed(2),
      holdSec,
    };
  });
}

export type ClientTxType = "deposit" | "withdrawal" | "transfer" | "adjustment" | "credit" | "ib-commission" | "bonus";
export interface ClientTx {
  id: string;
  type: ClientTxType;
  amount: number;
  status: "completed" | "pending" | "processing" | "rejected";
  method: string;
  hash?: string;
  createdAt: string;
  by?: string;
  reason?: string;
}

export function clientTransactions(c: AdminClient, n = 24): ClientTx[] {
  const r = seeded(hashString("tx" + c.id));
  if (!c.funded) return [];
  let t = ADMIN_NOW - r.int(30, 300) * MIN;
  const types: ClientTxType[] = ["deposit", "deposit", "deposit", "withdrawal", "transfer", "transfer", "adjustment", "bonus", "ib-commission"];
  return Array.from({ length: n }, (_, k) => {
    const type = r.pick(types);
    const amount = +(type === "bonus" ? 100 : type === "adjustment" ? r.range(-150, 400) : r.range(80, Math.max(200, c.deposits / 6))).toFixed(2);
    const out: ClientTx = {
      id: `TX${904400 - k * 41 - (hashString(c.id) % 300)}`,
      type,
      amount,
      status: k === 0 && type === "withdrawal" ? "pending" : r.bool(0.05) ? "rejected" : "completed",
      method: type === "deposit" || type === "withdrawal" ? "USDT · TRC20" : type === "transfer" ? `Wallet → ${r.pick(c.logins.length ? c.logins : ["80400000"])}` : "Internal",
      hash: type === "deposit" || type === "withdrawal" ? `${hashString("h" + c.id + k).toString(16)}9f${(k * 7907).toString(16)}c3e1a7` : undefined,
      createdAt: iso(t),
    };
    if (type === "adjustment") {
      out.by = r.pick(["Tom Becker", "Julia Novak"]);
      out.reason = r.pick(["ADJ-02 · Slippage compensation", "ADJ-05 · Swap correction", "ADJ-01 · Goodwill"]);
    }
    t -= r.int(4, 90) * HOUR;
    return out;
  });
}

export interface LoginEvent {
  id: string;
  time: string;
  ip: string;
  city: string;
  country: string;
  device: string;
  os: string;
  fingerprint: string;
  result: "success" | "failed" | "2fa";
  flagged?: string;
}

export function clientLogins(c: AdminClient, n = 14): LoginEvent[] {
  const r = seeded(hashString("lg" + c.id));
  const devices = [
    ["iPhone 15 Pro", "iOS 19.2"],
    ["MacBook Pro", "macOS 16"],
    ["Pixel 9", "Android 16"],
    ["Windows PC", "Windows 11"],
    ["Galaxy S25", "Android 16"],
  ];
  const cities: Record<string, string[]> = { in: ["Mumbai", "Bengaluru", "Pune"], ae: ["Dubai", "Abu Dhabi"], br: ["São Paulo", "Rio de Janeiro"], ng: ["Lagos", "Abuja"], gb: ["London", "Manchester"] };
  const home = cities[c.country] ?? [c.countryName];
  const fp = hashString("fp" + c.id).toString(16).slice(0, 8);
  let t = Date.parse(c.lastLogin);
  return Array.from({ length: n }, (_, k) => {
    const [device, os] = r.pick(devices);
    const odd = k === 3 || k === 9;
    const e: LoginEvent = {
      id: `L${k}`,
      time: iso(t),
      ip: odd ? `185.220.${r.int(100, 103)}.${r.int(2, 250)}` : `103.${r.int(20, 60)}.${r.int(1, 250)}.${r.int(2, 250)}`,
      city: odd ? "Frankfurt" : r.pick(home),
      country: odd ? "de" : c.country,
      device: device!,
      os: os!,
      fingerprint: odd ? "a91fe0c4" : fp,
      result: k === 5 ? "failed" : k === 1 ? "2fa" : "success",
      flagged: odd ? (k === 3 ? "Shared device with #100561 (Hamza Khan)" : "Tor exit node / VPN") : undefined,
    };
    t -= r.int(3, 60) * HOUR;
    return e;
  });
}

export interface ClientNote {
  id: string;
  author: StaffMember;
  time: string;
  text: string;
  kind: "note" | "call" | "email" | "system";
}

export function clientNotes(c: AdminClient): ClientNote[] {
  const a = staff(c.agentId);
  const first = c.name.split(" ")[0];
  return [
    { id: "n1", author: a, time: iso(ADMIN_NOW - 2 * HOUR), kind: "call", text: `Called ${first} — interested in the Pro account and swap-free on gold. Follow up Thursday 14:00 GMT+3.` },
    { id: "n2", author: staff("ST-107"), time: iso(ADMIN_NOW - 26 * HOUR), kind: "note", text: "Proof of address re-checked (utility bill, 11 Sep). Name matches passport. Level 2 approved." },
    { id: "n3", author: a, time: iso(ADMIN_NOW - 3 * DAY), kind: "email", text: "Sent welcome pack + webinar invite (Gold scalping masterclass)." },
    { id: "n4", author: STAFF_MEMBERS[8]!, time: iso(ADMIN_NOW - 6 * DAY), kind: "system", text: "Auto-routing moved account to B-book (risk score dropped below 6)." },
    { id: "n5", author: staff("ST-105"), time: iso(ADMIN_NOW - 12 * DAY), kind: "note", text: "Client asked about IB programme — referred to partners desk." },
  ];
}

export interface AuditEntry {
  id: string;
  time: string;
  actor: string;
  actorPhoto?: string;
  action: string;
  detail: string;
  reason?: string;
  ip: string;
}

export function clientAudit(c: AdminClient): AuditEntry[] {
  return [
    { id: "a1", time: iso(ADMIN_NOW - 40 * MIN), actor: "Julia Novak", actorPhoto: "/assets/people/women-29.jpg", action: "Viewed profile", detail: "Client 360", ip: "103.21.44.12" },
    { id: "a2", time: iso(ADMIN_NOW - 5 * HOUR), actor: "Tom Becker", actorPhoto: "/assets/people/men-12.jpg", action: "Balance adjustment", detail: `+$120.00 on ${c.logins[0] ?? "—"}`, reason: "ADJ-02 · Slippage compensation", ip: "103.21.44.19" },
    { id: "a3", time: iso(ADMIN_NOW - 26 * HOUR), actor: "Noura Khalid", actorPhoto: "/assets/people/women-11.jpg", action: "KYC approved", detail: "Level 2 · Passport + utility bill", reason: "KYC-OK · Documents consistent", ip: "103.21.44.31" },
    { id: "a4", time: iso(ADMIN_NOW - 2 * DAY), actor: "Aisha Rahman", actorPhoto: "/assets/people/women-22.jpg", action: "Impersonation (read-only)", detail: "Session 6m 12s", reason: "SUP-04 · Reproduce support ticket #88213", ip: "103.21.44.12" },
    { id: "a5", time: iso(ADMIN_NOW - 4 * DAY), actor: "Hannah Scott", actorPhoto: "/assets/people/women-32.jpg", action: "Group changed", detail: "Standard → Pro", reason: "GRP-01 · Client request", ip: "103.21.44.40" },
    { id: "a6", time: iso(ADMIN_NOW - 9 * DAY), actor: "System", action: "Tag added", detail: "High value", ip: "—" },
    { id: "a7", time: iso(ADMIN_NOW - 14 * DAY), actor: "Karim Nasser", actorPhoto: "/assets/people/men-52.jpg", action: "Agent assigned", detail: `→ ${staff(c.agentId).name}`, ip: "103.21.44.22" },
  ];
}

export interface IbNode {
  id: string;
  name: string;
  photo: string;
  country: string;
  level: number;
  clients: number;
  volume: number;
  commission: number;
  children?: IbNode[];
}

export function clientIbTree(c: AdminClient): IbNode {
  const r = seeded(hashString("ib" + c.id));
  const sub = CLIENTS.filter((x) => x.id !== c.id).slice(r.int(20, 60), r.int(70, 90));
  const pick = (k: number) => sub[(k * 7) % sub.length]!;
  const node = (k: number, level: number, kids?: IbNode[]): IbNode => {
    const p = pick(k);
    return { id: p.id, name: p.name, photo: p.photo, country: p.country, level, clients: r.int(0, 40), volume: +r.range(10, 1800).toFixed(1), commission: +r.range(20, 6400).toFixed(2), children: kids };
  };
  return {
    id: c.id,
    name: c.name,
    photo: c.photo,
    country: c.country,
    level: 0,
    clients: 18,
    volume: 2418.6,
    commission: 14208.4,
    children: [node(1, 1, [node(5, 2), node(6, 2), node(7, 2)]), node(2, 1, [node(8, 2)]), node(3, 1), node(4, 1, [node(9, 2), node(10, 2)])],
  };
}

/* ------------------------------------------------------------------ */
/* Leads pipeline                                                      */
/* ------------------------------------------------------------------ */

export const LEAD_STAGES = ["new", "contacted", "qualified", "deposit_pending", "ftd", "lost"] as const;
export type LeadStage = (typeof LEAD_STAGES)[number];
export const LEAD_STAGE_LABEL: Record<LeadStage, string> = {
  new: "New",
  contacted: "Contacted",
  qualified: "Qualified",
  deposit_pending: "Deposit pending",
  ftd: "FTD",
  lost: "Lost",
};

export interface Lead {
  id: string;
  name: string;
  photo: string;
  country: string;
  email: string;
  phone: string;
  stage: LeadStage;
  source: string;
  utm: string;
  value: number;
  agentId: string;
  desk: Desk;
  nextFollowUp: string;
  created: string;
  score: number; // 0..100
  lostReason?: string;
}

export const LEADS: Lead[] = (() => {
  const r = seeded(9301);
  const pool = CLIENTS.slice(24, 120);
  const utms = ["gold_scalp_sep", "brand_search", "ib_omar_mena", "yt_webinar_0918", "tt_btc_rally", "meta_lookalike_3", "cpa_fxnet", "tg_signals_ae"];
  const weights: LeadStage[] = ["new", "new", "new", "new", "contacted", "contacted", "contacted", "qualified", "qualified", "deposit_pending", "deposit_pending", "ftd", "ftd", "lost"];
  return Array.from({ length: 46 }, (_, i) => {
    const p = pool[(i * 11) % pool.length]!;
    const stage = i < 6 ? LEAD_STAGES[i]! : r.pick(weights);
    const agent = SALES_AGENTS.find((a) => a.desk === p.desk) ?? r.pick(SALES_AGENTS);
    return {
      id: `LD-${58200 + i * 3}`,
      name: p.name,
      photo: p.photo,
      country: p.country,
      email: p.email,
      phone: p.phone,
      stage,
      source: p.source,
      utm: r.pick(utms),
      value: Math.round(r.pick([250, 500, 1000, 2000, 5000, 10000]) * r.range(0.8, 1.4)),
      agentId: agent.id,
      desk: agent.desk as Desk,
      nextFollowUp: iso(ADMIN_NOW + r.int(-20, 72) * HOUR),
      created: iso(ADMIN_NOW - r.int(1, 30) * DAY),
      score: r.int(18, 96),
      lostReason: stage === "lost" ? r.pick(["No answer ×5", "Went with competitor", "Not eligible (US)", "Low budget"]) : undefined,
    };
  });
})();

/* ------------------------------------------------------------------ */
/* KYC queue                                                           */
/* ------------------------------------------------------------------ */

export interface KycCheck {
  label: string;
  result: "pass" | "warn" | "fail";
  detail: string;
}

export interface KycApplication {
  id: string;
  clientId: string;
  level: 1 | 2 | 3;
  docType: "Passport" | "National ID" | "Driving licence";
  poa: "Utility bill" | "Bank statement" | "Tenancy agreement" | null;
  providerScore: number; // 0..100 (Sumsub-style)
  faceMatch: number;
  docAuth: number;
  amlHit: "clear" | "pep" | "sanctions" | "adverse";
  submitted: string;
  slaMins: number; // minutes remaining (negative = breached)
  status: "pending" | "review" | "resubmit" | "approved" | "rejected";
  checks: KycCheck[];
  extracted: { field: string; doc: string; profile: string }[];
  corporate?: boolean;
}

export const KYC_QUEUE: KycApplication[] = (() => {
  const r = seeded(5521);
  const candidates = CLIENTS.filter((c) => c.kyc === "pending" || c.kyc === "review");
  const list = [CLIENTS[3]!, CLIENTS[0]!, ...candidates].slice(0, 16);
  return list.map((c, i) => {
    const score = i === 1 ? 48 : Math.round(r.range(52, 99));
    const faceMatch = i === 1 ? 61 : Math.round(r.range(78, 99.5));
    const docAuth = i === 1 ? 44 : Math.round(r.range(70, 99));
    const amlHit = i === 4 ? "pep" : i === 9 ? "adverse" : "clear";
    const docType = r.pick(["Passport", "National ID", "Driving licence"] as const);
    const dob = `${r.int(1972, 2003)}-${String(r.int(1, 12)).padStart(2, "0")}-${String(r.int(1, 28)).padStart(2, "0")}`;
    const mismatch = i === 1 || i === 6;
    const docName = mismatch ? c.name.replace(/(\w+)$/, (m) => m.slice(0, -1) + "a") : c.name.toUpperCase();
    return {
      id: `KYC-${73100 + i * 17}`,
      clientId: c.id,
      level: (i % 4 === 0 ? 3 : 2) as 1 | 2 | 3,
      docType: i === 0 ? "National ID" : i === 1 ? "Passport" : docType,
      poa: r.pick(["Utility bill", "Bank statement", "Tenancy agreement", null] as const),
      providerScore: score,
      faceMatch,
      docAuth,
      amlHit,
      submitted: iso(ADMIN_NOW - r.int(4, 600) * MIN),
      slaMins: i === 2 ? -18 : r.int(-5, 240),
      status: i === 0 || i === 1 ? "review" : score < 60 ? "review" : "pending",
      checks: [
        { label: "Document authenticity", result: docAuth >= 85 ? "pass" : docAuth >= 60 ? "warn" : "fail", detail: docAuth >= 85 ? "MRZ valid · security features detected" : "Font inconsistency near DOB field" },
        { label: "Face match", result: faceMatch >= 85 ? "pass" : faceMatch >= 70 ? "warn" : "fail", detail: `${faceMatch}% similarity · liveness passed` },
        { label: "Liveness", result: i === 1 ? "warn" : "pass", detail: i === 1 ? "Low light, 2 retries" : "Passive liveness · 0.98" },
        { label: "AML / PEP / sanctions", result: amlHit === "clear" ? "pass" : "warn", detail: amlHit === "clear" ? "No hits across 1,400 lists" : amlHit === "pep" ? "PEP — Tier 3 (relative of local official)" : "1 adverse media article (2021)" },
        { label: "Document expiry", result: i === 1 ? "fail" : "pass", detail: i === 1 ? "Expired 02 Aug 2026" : `Valid until ${r.int(2027, 2034)}` },
        { label: "Duplicate check", result: i === 6 ? "warn" : "pass", detail: i === 6 ? "Doc number seen on #100561" : "Unique document number" },
      ],
      extracted: [
        { field: "Full name", doc: docName, profile: c.name },
        { field: "Date of birth", doc: dob, profile: mismatch && i === 6 ? dob.replace(/-(\d\d)$/, "-0" + r.int(1, 9)) : dob },
        { field: "Nationality", doc: c.countryName, profile: c.countryName },
        { field: "Document no.", doc: `${c.country.toUpperCase()}${r.int(1000000, 9999999)}`, profile: "—" },
        { field: "Address", doc: i === 1 ? "Flat 4B, Andheri West, Mumbai" : `${r.int(2, 180)} ${r.pick(["Palm", "Harbour", "Cedar", "Marina"])} Road`, profile: i === 1 ? "12 Linking Rd, Bandra, Mumbai" : "Same" },
      ],
    };
  });
})();

export const CORPORATE_KYC = {
  id: "KYB-2044",
  company: "Meridian Capital Holdings Ltd",
  regNo: "C 184 229",
  jurisdiction: "Cyprus",
  country: "cy",
  incorporated: "2019-03-14",
  type: "Private limited company",
  address: "Arch. Makariou III 155, Limassol 3026",
  providerScore: 81,
  status: "review" as const,
  documents: [
    { name: "Certificate of incorporation", status: "verified" },
    { name: "Memorandum & articles", status: "verified" },
    { name: "Register of directors", status: "verified" },
    { name: "Register of shareholders", status: "pending" },
    { name: "Proof of registered address", status: "verified" },
    { name: "Board resolution (trading)", status: "pending" },
  ],
  directors: [
    { name: "Elena Petrova", role: "Director", photo: "/assets/people/women-32.jpg", country: "cy", kyc: "verified" as KycStatus, pep: false },
    { name: "Andreas Georgiou", role: "Director · Secretary", photo: "/assets/people/men-11.jpg", country: "cy", kyc: "verified" as KycStatus, pep: false },
  ],
  ubos: [
    { name: "Elena Petrova", share: 52, photo: "/assets/people/women-32.jpg", country: "cy", kyc: "verified" as KycStatus, pep: false },
    { name: "Hassan Karimi", share: 30, photo: "/assets/people/men-81.jpg", country: "tr", kyc: "pending" as KycStatus, pep: true },
    { name: "Treasury shares", share: 18, photo: "", country: "cy", kyc: "none" as KycStatus, pep: false },
  ],
};

/* ------------------------------------------------------------------ */
/* AML                                                                 */
/* ------------------------------------------------------------------ */

export interface AmlCase {
  id: string;
  clientId: string;
  rule: string;
  ruleId: string;
  amount: number;
  risk: "low" | "medium" | "high" | "critical";
  status: "open" | "investigating" | "escalated" | "sar_filed" | "closed";
  assigneeId: string;
  opened: string;
  sar: "not_required" | "drafting" | "filed" | "pending";
  timeline: { time: string; who: string; text: string }[];
  txs: { id: string; type: string; amount: number; time: string; hash: string; counterparty: string }[];
  notes: string[];
}

export const AML_RULES = [
  { id: "R-101", name: "Rapid in/out", desc: "Deposit followed by withdrawal ≥ 80% within 24h with < 1 lot traded", severity: "high", enabled: true, hits: 14 },
  { id: "R-102", name: "Structuring", desc: "≥ 3 deposits between $9,000 and $9,999 within 7 days", severity: "high", enabled: true, hits: 3 },
  { id: "R-103", name: "Large single deposit", desc: "Single deposit ≥ $50,000 or ≥ 10× historical average", severity: "medium", enabled: true, hits: 6 },
  { id: "R-104", name: "Third-party wallet", desc: "Withdrawal address differs from any deposit source address", severity: "medium", enabled: true, hits: 22 },
  { id: "R-105", name: "High-risk jurisdiction IP", desc: "Login or funding from FATF grey/black-listed jurisdiction", severity: "high", enabled: true, hits: 2 },
  { id: "R-106", name: "Mixer / sanctioned exposure", desc: "On-chain risk score ≥ 70 (Chainalysis-style) on source funds", severity: "critical", enabled: true, hits: 1 },
  { id: "R-107", name: "Dormant then active", desc: "Account dormant > 180 days then deposit ≥ $20,000", severity: "low", enabled: false, hits: 0 },
  { id: "R-108", name: "PEP re-screening", desc: "Nightly re-screen of all verified clients against PEP & sanctions lists", severity: "medium", enabled: true, hits: 4 },
] as const;

export const AML_CASES: AmlCase[] = (() => {
  const r = seeded(7719);
  const specs: [number, number, AmlCase["risk"], AmlCase["status"], AmlCase["sar"]][] = [
    [0, 48000, "high", "investigating", "drafting"],
    [5, 29640, "high", "open", "pending"],
    [3, 9800, "critical", "escalated", "drafting"],
    [2, 61200, "medium", "open", "pending"],
    [1, 12400, "medium", "investigating", "not_required"],
    [4, 7300, "medium", "open", "pending"],
    [6, 18900, "high", "sar_filed", "filed"],
    [7, 4100, "low", "closed", "not_required"],
    [0, 22500, "medium", "closed", "not_required"],
    [3, 9950, "high", "investigating", "pending"],
  ];
  const pool = CLIENTS.filter((c) => c.funded).slice(10, 60);
  return specs.map(([ri, amount, risk, status, sar], i) => {
    const rule = AML_RULES[ri]!;
    const c = pool[(i * 7) % pool.length]!;
    const assignee = COMPLIANCE_STAFF[i % 2]!;
    const opened = ADMIN_NOW - r.int(1, 120) * HOUR;
    return {
      id: `AML-${3300 + i * 9}`,
      clientId: c.id,
      rule: rule.name,
      ruleId: rule.id,
      amount,
      risk,
      status,
      assigneeId: assignee.id,
      opened: iso(opened),
      sar,
      timeline: [
        { time: iso(opened), who: "System", text: `Rule ${rule.id} “${rule.name}” triggered on ${c.name}` },
        { time: iso(opened + 14 * MIN), who: "System", text: "Withdrawals auto-held pending review" },
        { time: iso(opened + 52 * MIN), who: assignee.name, text: "Case picked up; requested source-of-funds documents" },
        ...(status !== "open" ? [{ time: iso(opened + 5 * HOUR), who: assignee.name, text: "Client uploaded bank statement + employment letter" }] : []),
        ...(status === "escalated" || status === "sar_filed" ? [{ time: iso(opened + 9 * HOUR), who: "Noura Khalid", text: "Escalated to MLRO — inconsistent source of funds" }] : []),
        ...(status === "sar_filed" ? [{ time: iso(opened + 30 * HOUR), who: "Noura Khalid", text: "SAR filed with FIU (ref FIU-26-08841)" }] : []),
        ...(status === "closed" ? [{ time: iso(opened + 20 * HOUR), who: assignee.name, text: "Closed — explained, documentation satisfactory" }] : []),
      ],
      txs: Array.from({ length: 4 }, (_, k) => ({
        id: `TX${903800 - i * 90 - k * 13}`,
        type: k % 2 === 0 ? "Deposit" : "Withdrawal",
        amount: +(amount / (k + 1.6)).toFixed(2),
        time: iso(opened - k * 7 * HOUR),
        hash: `${hashString(`aml${i}${k}`).toString(16)}e7${(k * 3313).toString(16)}b90d`,
        counterparty: k % 2 === 0 ? trcAddress(hashString(`cp${i}${k}`)) : c.wallet,
      })),
      notes: ["Client is a salaried engineer; declared income $6k/month.", "Funds arrived from Binance hot wallet — exchange KYC likely."],
    };
  });
})();

/* ------------------------------------------------------------------ */
/* Duplicate detection                                                 */
/* ------------------------------------------------------------------ */

export type DuplicateKind = "device" | "ip" | "wallet" | "kyc_doc";

export interface DuplicateCluster {
  id: string;
  kind: DuplicateKind;
  value: string;
  confidence: number;
  clientIds: string[];
  detected: string;
  note: string;
  bonusClaimed: boolean;
  status: "open" | "confirmed" | "false_positive";
}

export const DUPLICATE_CLUSTERS: DuplicateCluster[] = (() => {
  const pool = CLIENTS.slice(20, 150);
  const g = (k: number[]) => k.map((x) => pool[x % pool.length]!.id);
  return [
    { id: "DUP-901", kind: "device", value: "fp:a91fe0c4 · Chrome 131 · macOS", confidence: 97, clientIds: g([3, 41, 88]), detected: iso(ADMIN_NOW - 35 * MIN), note: "3 accounts, same canvas + WebGL hash; each claimed the $100 welcome bonus", bonusClaimed: true, status: "open" },
    { id: "DUP-902", kind: "wallet", value: "TN4bXq8…u8Qa (TRC20)", confidence: 94, clientIds: g([12, 57]), detected: iso(ADMIN_NOW - 3 * HOUR), note: "Same withdrawal address used by two unrelated KYC identities", bonusClaimed: false, status: "open" },
    { id: "DUP-903", kind: "kyc_doc", value: "Passport PK8822914", confidence: 99, clientIds: g([7, 93]), detected: iso(ADMIN_NOW - 5 * HOUR), note: "Identical document number submitted under different names", bonusClaimed: false, status: "open" },
    { id: "DUP-904", kind: "ip", value: "185.220.101.44 (Tor exit)", confidence: 62, clientIds: g([19, 25, 66, 104]), detected: iso(ADMIN_NOW - 8 * HOUR), note: "Shared Tor exit node — likely coincidental", bonusClaimed: false, status: "open" },
    { id: "DUP-905", kind: "device", value: "fp:3cc81d02 · iPhone 15 · iOS 19", confidence: 88, clientIds: g([30, 31]), detected: iso(ADMIN_NOW - 26 * HOUR), note: "Household? Same surname, same address", bonusClaimed: true, status: "open" },
    { id: "DUP-906", kind: "ip", value: "41.58.22.190 (Lagos, static)", confidence: 71, clientIds: g([44, 45, 46]), detected: iso(ADMIN_NOW - 2 * DAY), note: "Same office IP — IB sub-network under IB-2214", bonusClaimed: false, status: "confirmed" },
    { id: "DUP-907", kind: "wallet", value: "TXk9pLm…3rWe (TRC20)", confidence: 55, clientIds: g([70, 115]), detected: iso(ADMIN_NOW - 3 * DAY), note: "Exchange deposit address (Binance) — shared omnibus", bonusClaimed: false, status: "false_positive" },
  ];
})();

/* ------------------------------------------------------------------ */
/* Segments                                                            */
/* ------------------------------------------------------------------ */

export interface SegmentCondition {
  field: string;
  op: string;
  value: string;
}

export interface Segment {
  id: string;
  name: string;
  description: string;
  conditions: SegmentCondition[];
  match: "all" | "any";
  count: number;
  trend: number;
  owner: string;
  updated: string;
  auto: boolean;
  color: "ember" | "gold" | "up" | "down" | "info" | "warn";
}

export const SEGMENT_FIELDS = ["Country", "KYC status", "Net deposits", "Equity", "Last login", "Risk score", "Group", "Tag", "Desk", "Source", "Lifetime lots", "Registered"] as const;

export const SEGMENTS: Segment[] = [
  { id: "SEG-1", name: "VIP whales", description: "High-value active clients for the VIP desk", conditions: [{ field: "Net deposits", op: "≥", value: "$25,000" }, { field: "Last login", op: "within", value: "14 days" }], match: "all", count: 38, trend: 4.2, owner: "Hannah Scott", updated: iso(ADMIN_NOW - 2 * HOUR), auto: true, color: "gold" },
  { id: "SEG-2", name: "Registered, no FTD (7d)", description: "Sales follow-up for new sign-ups", conditions: [{ field: "Registered", op: "within", value: "7 days" }, { field: "Net deposits", op: "=", value: "$0" }, { field: "KYC status", op: "is not", value: "Rejected" }], match: "all", count: 214, trend: 12.8, owner: "Sara Whitfield", updated: iso(ADMIN_NOW - 30 * MIN), auto: true, color: "ember" },
  { id: "SEG-3", name: "Churn risk — dormant funded", description: "Funded clients with no login in 30 days", conditions: [{ field: "Equity", op: "≥", value: "$500" }, { field: "Last login", op: "older than", value: "30 days" }], match: "all", count: 96, trend: -3.1, owner: "Marcus Dlamini", updated: iso(ADMIN_NOW - 1 * DAY), auto: true, color: "warn" },
  { id: "SEG-4", name: "Toxic flow candidates", description: "Route review for the dealing desk", conditions: [{ field: "Risk score", op: "≥", value: "8" }, { field: "Tag", op: "has", value: "Scalper" }], match: "any", count: 17, trend: 2.0, owner: "Julia Novak", updated: iso(ADMIN_NOW - 4 * HOUR), auto: true, color: "down" },
  { id: "SEG-5", name: "MENA gold traders", description: "Swap-free campaign audience", conditions: [{ field: "Country", op: "in", value: "AE, SA, EG, TR" }, { field: "Lifetime lots", op: "≥", value: "10" }, { field: "Group", op: "is", value: "Standard" }], match: "all", count: 142, trend: 6.5, owner: "Karim Nasser", updated: iso(ADMIN_NOW - 3 * DAY), auto: false, color: "info" },
  { id: "SEG-6", name: "KYC stuck > 48h", description: "Pending verification, nudge by email", conditions: [{ field: "KYC status", op: "is", value: "Pending" }, { field: "Registered", op: "older than", value: "2 days" }], match: "all", count: 61, trend: -8.4, owner: "Noura Khalid", updated: iso(ADMIN_NOW - 6 * HOUR), auto: true, color: "up" },
];

/* ------------------------------------------------------------------ */
/* Helpers                                                             */
/* ------------------------------------------------------------------ */

export const KYC_LABEL: Record<KycStatus, string> = { verified: "Verified", pending: "Pending", review: "In review", rejected: "Rejected", none: "Not started" };

export const REASON_CODES = {
  adjustment: ["ADJ-01 · Goodwill", "ADJ-02 · Slippage compensation", "ADJ-03 · Bonus credit", "ADJ-04 · Chargeback", "ADJ-05 · Swap correction", "ADJ-06 · Error correction"],
  trade: ["TRD-01 · Client request (phone, recorded)", "TRD-02 · Stop-out failure", "TRD-03 · Off-market price", "TRD-04 · Abusive trading", "TRD-05 · Risk limit breach"],
  impersonate: ["SUP-01 · Reproduce UI issue", "SUP-04 · Reproduce support ticket", "CMP-02 · Compliance review", "RSK-03 · Risk investigation"],
  reject: ["REJ-01 · Document unreadable", "REJ-02 · Document expired", "REJ-03 · Name mismatch", "REJ-04 · Suspected forgery", "REJ-05 · Selfie mismatch", "REJ-06 · Sanctions / PEP hit"],
  withdrawal: ["WDR-01 · Third-party wallet", "WDR-02 · KYC incomplete", "WDR-03 · Bonus terms not met", "WDR-04 · AML hold", "WDR-05 · Insufficient free margin"],
  block: ["BLK-01 · AML hold", "BLK-02 · Abusive trading", "BLK-03 · Duplicate account", "BLK-04 · Client request", "BLK-05 · Chargeback"],
  group: ["GRP-01 · Client request", "GRP-02 · Volume upgrade", "GRP-03 · Risk downgrade"],
  routing: ["RTE-01 · Toxic flow", "RTE-02 · Exposure hedge", "RTE-03 · Manual review"],
} as const;

export function timeAgo(isoStr: string, now = ADMIN_NOW): string {
  const d = now - Date.parse(isoStr);
  const future = d < 0;
  const a = Math.abs(d);
  const s = a < HOUR ? `${Math.max(1, Math.round(a / MIN))}m` : a < DAY ? `${Math.round(a / HOUR)}h` : a < 30 * DAY ? `${Math.round(a / DAY)}d` : `${Math.round(a / (30 * DAY))}mo`;
  return future ? `in ${s}` : `${s} ago`;
}

/** Formats an ISO timestamp in server time (GMT+3). */
export function serverTime(isoStr: string, withDate = true): string {
  const d = new Date(Date.parse(isoStr) + 3 * HOUR);
  const hh = String(d.getUTCHours()).padStart(2, "0");
  const mm = String(d.getUTCMinutes()).padStart(2, "0");
  if (!withDate) return `${hh}:${mm}`;
  const mon = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][d.getUTCMonth()];
  return `${String(d.getUTCDate()).padStart(2, "0")} ${mon} ${hh}:${mm}`;
}
