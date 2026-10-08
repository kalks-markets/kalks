"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowDownToLine, ArrowLeftRight, CandlestickChart, Check, Coins, KeyRound, Layers, Loader2, Lock, MoreHorizontal, PencilLine, RefreshCcw, Gauge as GaugeIcon, Star, StarOff, Trash2, Users, Wallet } from "lucide-react";
import { toast } from "sonner";
import { Button, Chip, CopyButton, Dialog, IconButton, Menu, Money, cn, type ButtonProps } from "@/components/kit";
import { useT } from "@kalks/i18n/react";
import { useReadOnly } from "@/components/session";
import { STATUS_LABEL, curOf, errorToast, fmtLevel, levelTone, openTerminal, serverOf, tradingApi, type EngineAccount } from "./api";
import { productOf } from "@/lib/products";
import { DeleteAccountDialog, FlavorChip, RenameDialog, accountFlavor, copyingName } from "./archive";
import { CloseAccountDialog } from "./closure";
import { ChangeTypeDialog, DefaultStar, DemoBalanceDialog, TransferBetweenDialog, setDefaultAccount } from "./extras";

/** Prop-challenge accounts live in engine groups named prop*: simulated capital that is never funded from the
 * wallet (the wallet refuses transfers to them) and never counted in the client's own live equity. */
export function isPropAccount(a: Pick<EngineAccount, "group">) {
  return a.group.toLowerCase().startsWith("prop");
}

export function KindBadge({ type, prop }: { type: "live" | "demo"; prop?: boolean }) {
  const t = useT();
  if (prop)
    return (
      <Chip tone="neutral" size="sm" className="font-semibold tracking-wider">
        {t("accounts.badge.prop")}
      </Chip>
    );
  return type === "live" ? (
    <Chip tone="ember" size="sm" className="font-semibold tracking-wider">
      {t("accounts.badge.live")}
    </Chip>
  ) : (
    <Chip tone="gold" size="sm" className="font-semibold tracking-wider">
      {t("accounts.badge.demo")}
    </Chip>
  );
}

export function StatusBadge({ a }: { a: Pick<EngineAccount, "status"> }) {
  const t = useT();
  if (a.status === "active") return null;
  const s = STATUS_LABEL[a.status];
  return (
    <Chip size="sm" tone={s.tone}>
      {t.dyn(`accounts.status.${a.status}`, s.label)}
    </Chip>
  );
}

/** Opens Kalks Trader signed in to this account (one-time SSO token). */
export function TradeButton({ a, size = "sm", label, ...rest }: { a: Pick<EngineAccount, "login" | "status"> } & Omit<ButtonProps, "onClick"> & { label?: string }) {
  const t = useT();
  const [busy, setBusy] = React.useState(false);
  const blocked = a.status === "disabled" || a.status === "expired" || a.status === "archived" || a.status === "closed";
  return (
    <Button
      size={size}
      variant="ember"
      disabled={busy || blocked}
      title={blocked ? t("accounts.row.cantOpenTrader") : undefined}
      onClick={async () => {
        setBusy(true);
        await openTerminal(a.login);
        setBusy(false);
      }}
      {...rest}
    >
      {busy ? <Loader2 className="animate-spin" /> : <CandlestickChart />} {label ?? t("accounts.row.trade")}
    </Button>
  );
}

/* ------------------------------------------------------------------ */
/* Funding (live accounts start at 0; deposits open with the wallet)    */
/* ------------------------------------------------------------------ */

export function FundDialog({ a, open, onOpenChange }: { a: Pick<EngineAccount, "login" | "cent" | "groupName">; open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT();
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("accounts.fund.title", { login: a.login })}
      description={`${a.groupName}${a.cent ? ` · ${t("accounts.fund.centAccount")}` : ""}`}
      width={480}
      footer={
        <>
          <Link href="/wallet/deposit">
            <Button variant="surface">{t("accounts.fund.depositUsdt")}</Button>
          </Link>
          <Link href={`/wallet/transfer?to=${a.login}`}>
            <Button variant="ember">{t("accounts.fund.transferFromWallet")}</Button>
          </Link>
        </>
      }
    >
      <div className="space-y-3 text-[13.5px] text-fg-2">
        <div className="flex items-start gap-3 rounded-[14px] border border-line bg-surface-2 px-4 py-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-full border border-ember/30 bg-ember-soft text-ember">
            <Wallet className="size-4" />
          </span>
          <div>
            <div className="font-medium text-fg">{t("accounts.fund.fromWallet")}</div>
            <p className="mt-0.5 text-[12.5px] text-fg-3">
              {t(a.cent ? "accounts.fund.textCent" : "accounts.fund.text")}
            </p>
          </div>
        </div>
      </div>
    </Dialog>
  );
}

export function FundButton({ a, size = "sm", variant = "surface" }: { a: Pick<EngineAccount, "login" | "cent" | "groupName">; size?: ButtonProps["size"]; variant?: ButtonProps["variant"] }) {
  const t = useT();
  const [open, setOpen] = React.useState(false);
  return (
    <>
      <Button size={size} variant={variant} onClick={() => setOpen(true)}>
        <ArrowDownToLine /> {t("accounts.fund.button")}
      </Button>
      <FundDialog a={a} open={open} onOpenChange={setOpen} />
    </>
  );
}

/* ------------------------------------------------------------------ */
/* Demo refill (D8)                                                    */
/* ------------------------------------------------------------------ */

export function refillsLeft(a: Pick<EngineAccount, "demo">) {
  return a.demo ? Math.max(0, a.demo.refillsPerDay - a.demo.refillsUsedToday) : 0;
}

/** Demo starting balance in the account currency (the engine already reports it in cents for cent accounts). */
export function demoTarget(a: Pick<EngineAccount, "demo" | "cent">) {
  return a.demo ? a.demo.initialBalance : null;
}

export function useRefill(a: Pick<EngineAccount, "login" | "cent" | "currency" | "demo">, onDone?: () => void) {
  const t = useT();
  const [busy, setBusy] = React.useState(false);
  const run = React.useCallback(async () => {
    setBusy(true);
    try {
      const r = await tradingApi<{ amount: number; balance: number }>(`accounts/${a.login}/demo-refill`, { body: {} });
      const cur = curOf(a);
      toast.success(t("accounts.refill.done"), { description: t("accounts.refill.desc", { login: a.login, amount: `${cur}${r.balance.toLocaleString("en-US", { minimumFractionDigits: 2 })}`, count: Math.max(0, refillsLeft(a) - 1) }) });
      onDone?.();
    } catch (e) {
      errorToast(t("accounts.refill.error"), e);
    } finally {
      setBusy(false);
    }
  }, [a, onDone, t]);
  return { busy, run };
}

export function RefillButton({ a, onDone, size = "sm" }: { a: EngineAccount; onDone?: () => void; size?: ButtonProps["size"] }) {
  const t = useT();
  const { busy, run } = useRefill(a, onDone);
  const left = refillsLeft(a);
  const full = demoTarget(a) !== null && a.balance >= demoTarget(a)!;
  return (
    <Button size={size} variant="surface" disabled={busy || left === 0 || full || a.status === "expired"} onClick={run} title={left === 0 ? t("accounts.refill.noneLeft") : full ? t("accounts.refill.full") : undefined}>
      {busy ? <Loader2 className="animate-spin" /> : <RefreshCcw />} {t("accounts.row.refill")}
    </Button>
  );
}

/* ------------------------------------------------------------------ */
/* Account row (accounts list, dashboard)                              */
/* ------------------------------------------------------------------ */

export function AccountActions({ a, onChanged }: { a: EngineAccount; onChanged?: () => void }) {
  const t = useT();
  const [renaming, setRenaming] = React.useState(false);
  const [deleting, setDeleting] = React.useState(false);
  const [closing, setClosing] = React.useState(false);
  const [typing, setTyping] = React.useState(false);
  const [moving, setMoving] = React.useState(false);
  const [demoBal, setDemoBal] = React.useState(false);
  // prop-challenge accounts are opened and closed by the prop service, not by the client
  const prop = isPropAccount(a);
  const live = a.type === "live";
  const special = prop || accountFlavor(a) !== null;
  const star = async () => {
    try {
      await setDefaultAccount(a.isDefault ? null : a.login);
      toast.success(a.isDefault ? t("accounts.default.removed") : t("accounts.default.set"), { description: `#${a.login}` });
      onChanged?.();
    } catch (e) {
      errorToast(t("accounts.default.failed"), e);
    }
  };
  return (
    <>
      <Menu
        trigger={
          <IconButton size="sm" aria-label={t("accounts.menu.actions")}>
            <MoreHorizontal />
          </IconButton>
        }
        items={[
          { label: t("accounts.menu.details"), icon: <GaugeIcon />, href: `/accounts/${a.login}` },
          { label: a.isDefault ? t("accounts.default.unset") : t("accounts.default.makeDefault"), icon: a.isDefault ? <StarOff /> : <Star />, onSelect: () => void star() },
          { label: t("accounts.menu.changeLeverage"), icon: <GaugeIcon />, href: `/accounts/${a.login}?tab=settings` },
          ...(special ? [] : [{ label: t("accounts.type.menu"), icon: <Layers />, onSelect: () => setTyping(true) }]),
          ...(live && !prop ? [{ label: t("accounts.between.menu"), icon: <ArrowLeftRight />, onSelect: () => setMoving(true) }] : []),
          ...(!live ? [{ label: t("accounts.demoBalance.menu"), icon: <Coins />, onSelect: () => setDemoBal(true) }] : []),
          { label: t("accounts.menu.passwords"), icon: <KeyRound />, href: `/accounts/${a.login}?tab=credentials` },
          { label: t("accounts.menu.statements"), icon: <ArrowDownToLine />, href: `/accounts/${a.login}?tab=history` },
          { label: t("accounts.history.zip"), icon: <ArrowDownToLine />, onSelect: () => window.location.assign(`/api/trading/accounts/${a.login}/history-zip`) },
          { label: t("accounts.menu.rename"), icon: <PencilLine />, onSelect: () => setRenaming(true) },
          ...(prop
            ? []
            : ([
                "sep",
                { label: t("accounts.menu.delete"), icon: <Trash2 />, danger: true, onSelect: () => setDeleting(true) },
                ...(live ? [{ label: t("accounts.close.menu"), icon: <Lock />, danger: true, onSelect: () => setClosing(true) }] : []),
              ] as const)),
        ]}
      />
      {renaming && <RenameDialog a={a} open={renaming} onOpenChange={setRenaming} onDone={onChanged} />}
      {deleting && <DeleteAccountDialog a={a} open={deleting} onOpenChange={setDeleting} onDone={onChanged} />}
      {closing && <CloseAccountDialog a={a} open={closing} onOpenChange={setClosing} onDone={onChanged} />}
      {typing && <ChangeTypeDialog a={a} open={typing} onOpenChange={setTyping} onDone={onChanged} />}
      {moving && <TransferBetweenDialog from={a} open={moving} onOpenChange={setMoving} onDone={onChanged} />}
      {demoBal && <DemoBalanceDialog a={a} open={demoBal} onOpenChange={setDemoBal} onDone={onChanged} />}
    </>
  );
}

/** Kalks Trader for a copy-trading account: the copy service trades it, the client watches P&L and manages the copy. */
function CopyActions({ a }: { a: EngineAccount }) {
  const t = useT();
  return (
    <>
      <Link href="/social/copy">
        <Button size="sm" variant="surface">
          <Users /> {t("accounts.copy.manage")}
        </Button>
      </Link>
      <TradeButton a={a} variant="surface" label={t("accounts.copy.watchPnl")} />
    </>
  );
}

/** OPTIONS chip on an Options account (CFD / Options account split); nothing on a CFD account. */
export function ProductChip({ a, size = "sm" }: { a: { product?: string | null }; size?: "sm" | "md" }) {
  const t = useT();
  if (productOf(a) !== "options") return null;
  return (
    <Chip size={size} tone="ember" className="font-semibold tracking-wider" data-testid="product-chip-options">
      {t("accounts.product.chipOptions")}
    </Chip>
  );
}

export function LiveAccountRow({ a, onChanged, compact }: { a: EngineAccount; onChanged?: () => void; compact?: boolean }) {
  const t = useT();
  const cur = curOf(a);
  const tone = levelTone(a.marginLevel);
  const readOnly = useReadOnly();
  const flavor = accountFlavor(a);
  const copying = flavor === "copy" ? copyingName(a) : null;
  return (
    <div className="k-row group relative overflow-hidden p-4 transition-colors hover:border-[var(--k-border-top)] sm:p-5">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <KindBadge type={a.type} prop={isPropAccount(a)} />
        <FlavorChip a={a} />
        <ProductChip a={a} />
        <Link href={`/accounts/${a.login}`} className="text-[15px] font-medium text-fg hover:text-ember">
          {productOf(a) === "options" ? a.groupName : `${a.groupName} · ${t.dyn(`accounts.mode.${a.mode}`, a.mode)}`}
        </Link>
        <span className="inline-flex items-center gap-1 font-mono text-[13px] text-fg-2">
          #{a.login}
          <CopyButton value={String(a.login)} label={t("accounts.label.login")} />
        </span>
        {a.name && <span className="truncate text-[13px] text-fg-3">“{a.name}”</span>}
        {a.cent && (
          <Chip size="sm" tone="gold">
            USC
          </Chip>
        )}
        <DefaultStar a={a} />
        <StatusBadge a={a} />
        {a.closureRequest?.status === "pending" && (
          <Chip size="sm" tone="info">
            {t("accounts.close.pendingChip")}
          </Chip>
        )}
        {a.dormantSince && (
          <Chip size="sm" tone="warn">
            {t("accounts.dormant.chip")}
          </Chip>
        )}
        <div className="ms-auto flex items-center gap-2 text-xs text-fg-3">
          <span className="hidden font-mono sm:inline">{serverOf(a)}</span>
          {productOf(a) !== "options" && <Chip size="sm">1:{a.leverage.toLocaleString("en-US")}</Chip>}
        </div>
      </div>
      <div className={cn("mt-4 grid items-end gap-4", compact ? "grid-cols-2 sm:grid-cols-3 xl:grid-cols-[1fr_1fr_1fr_auto]" : "grid-cols-2 sm:grid-cols-4 xl:grid-cols-[1fr_1fr_1fr_1fr_auto]")}>
        <div className="min-w-0">
          <div className="text-[12px] text-fg-3">{t("common.balance")}</div>
          <Money value={a.balance} currency={cur} countUp={false} className="mt-1 block truncate text-[16px] font-semibold sm:text-[19px]" />
        </div>
        <div className="min-w-0">
          <div className="text-[12px] text-fg-3">{t("common.equity")}</div>
          <Money value={a.equity} currency={cur} countUp={false} className="mt-1 block truncate text-[16px] font-semibold sm:text-[19px]" />
        </div>
        {!compact && (
          <div className="hidden min-w-0 sm:block">
            <div className="text-[12px] text-fg-3">{t("accounts.label.freeMargin")}</div>
            <Money value={a.freeMargin} currency={cur} countUp={false} className="mt-1 block text-[15px] font-medium text-fg-2" />
          </div>
        )}
        <div>
          <div className="whitespace-nowrap text-[12px] text-fg-3">{t("accounts.label.marginLevel")}</div>
          <div className={cn("k-num mt-1 text-[17px] font-semibold", tone === "up" && "text-up", tone === "warn" && "text-warn", tone === "down" && "text-down")}>{fmtLevel(a.marginLevel)}</div>
        </div>
        {!readOnly && (
          <div className="col-span-full flex flex-wrap items-center justify-end gap-2 xl:col-span-1">
            <AccountActions a={a} onChanged={onChanged} />
            {a.type === "live" ? !isPropAccount(a) && <FundButton a={a} /> : <RefillButton a={a} onDone={onChanged} />}
            {flavor === "copy" ? <CopyActions a={a} /> : <TradeButton a={a} />}
          </div>
        )}
      </div>
      <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-[11.5px] text-fg-3">
        {copying && <span className="font-medium text-fg-2">{t("accounts.copy.copying", { name: copying })}</span>}
        {a.positions > 0 || a.orders > 0 ? (
          <span>
            {t("accounts.row.openPositions", { count: a.positions })} · {t("accounts.row.pendingOrders", { count: a.orders })} · {t("accounts.row.floating")}{" "}
            <Money value={a.profit} currency={cur} signed tone="auto" countUp={false} />
          </span>
        ) : (
          <span>{t("accounts.row.noPositions")}</span>
        )}
        {a.type === "live" && a.balance === 0 && a.equity === 0 && <span className="text-warn">{t("accounts.row.notFunded")}</span>}
        {a.type === "demo" && a.demo && (
          <span>
            {t("accounts.row.demoRefills", { left: refillsLeft(a), total: a.demo.refillsPerDay, days: a.demo.expiryDays })}
          </span>
        )}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Credentials (shown once after opening / changing)                   */
/* ------------------------------------------------------------------ */

export function SecretField({ label, value, hint, secret }: { label: string; value: string; hint?: React.ReactNode; secret?: boolean }) {
  const t = useT();
  const [show, setShow] = React.useState(!secret);
  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between text-[12px] font-medium text-fg-2">
        {label}
        {hint && <span className="font-normal text-fg-3">{hint}</span>}
      </div>
      <div className="flex h-11 items-center gap-2 rounded-[14px] border border-line bg-surface-2 px-3.5">
        <span dir="ltr" className="min-w-0 flex-1 truncate font-mono text-[14px] text-fg" data-secret={secret ? label : undefined}>
          {show ? value : "•".repeat(Math.min(12, value.length))}
        </span>
        {secret && (
          <button type="button" onClick={() => setShow((s) => !s)} className="text-[11.5px] font-medium text-fg-3 hover:text-fg">
            {show ? t("accounts.secret.hide") : t("accounts.secret.show")}
          </button>
        )}
        <CopyButton value={value} label={label} />
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Password rules (engine: 8–64 characters, letters and digits)        */
/* ------------------------------------------------------------------ */

export const LIVE_PASSWORD_RULES = [
  { key: "len", labelKey: "accounts.password.len" as const, label: "8–64 characters", test: (p: string) => p.length >= 8 && p.length <= 64 },
  { key: "letter", labelKey: "accounts.password.letter" as const, label: "At least one letter", test: (p: string) => /\p{L}/u.test(p) },
  { key: "digit", labelKey: "accounts.password.digit" as const, label: "At least one digit", test: (p: string) => /\d/.test(p) },
];

export const livePasswordOk = (p: string) => LIVE_PASSWORD_RULES.every((r) => r.test(p));

export function PasswordRules({ password }: { password: string }) {
  const t = useT();
  return (
    <ul className="grid grid-cols-1 gap-1.5 sm:grid-cols-3">
      {LIVE_PASSWORD_RULES.map((r) => {
        const ok = r.test(password);
        return (
          <li key={r.key} className={cn("flex items-center gap-2 text-[12.5px] transition-colors", ok ? "text-up" : "text-fg-3")}>
            <span className={cn("grid size-4 place-items-center rounded-full border", ok ? "border-up/40 bg-up-soft" : "border-line")}>{ok && <Check className="size-2.5" />}</span>
            {t(r.labelKey)}
          </li>
        );
      })}
    </ul>
  );
}
