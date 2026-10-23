"use client";

import * as React from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { AnimatePresence, motion } from "motion/react";
import { ArrowLeft, ArrowRight, Check, ChevronDown, Eye, Info, KeyRound, Lock, RotateCw, ScrollText, SlidersHorizontal, Wallet } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, EmptyState, Field, Input, KeyValue, PageHeader, Reveal, Segmented, Skeleton, Stepper, cn } from "@/components/kit";
import { useT } from "@kalks/i18n/react";
import { ApiError, modeLabel, serverOf, tradingApi, useAccounts, useGroups, type AccountKind, type EngineAccount, type EngineGroup, type OpenResult } from "./api";
import { CardFace, EngineGroupCard, commissionText, faceOf, spreadType } from "./group-card";
import { FundButton, SecretField, TradeButton } from "./ui";
import { ChangePasswordDialog } from "./manage";
import { useFeatures } from "@/components/tenant-config";
import { useReadOnly } from "@/components/session";
import { SuitabilityError, suitabilityApi, useSuitability, type Suitability } from "@/components/options/api";
import { IdeaCards, TermsDialog } from "@/components/options/ui";
import { defaultGroup, offeredGroups, optionsModuleOn, parseProduct, productOf, usedFor, type Product } from "@/lib/products";

// Quick open (founder 2026-10-23: "no ID / password required"). The wizard is the product (CFD account | Options
// account, only while the broker has the Options module on; the Options path shows the options intro first while the
// client hasn't accepted it), then one Account step: live / demo, the account type (Standard preselected), "More
// options" (leverage, demo balance, nickname; defaults set), what happens, the risk acknowledgement and Open. No
// password: the client trades through Kalks (SSO) and can set a trading or investor password later, from the Done
// screen or the account's Credentials tab. The engine still generates its own; they never reach the browser.
type Step = "product" | "account" | "done";
const STEP_LABEL = { product: "accounts.wizard.step.product", account: "accounts.wizard.step.account", done: "accounts.wizard.step.done" } as const satisfies Record<Step, string>;
const DEMO_BALANCES = [1000, 5000, 10000, 25000, 50000, 100000];

interface Cfg {
  product: Product;
  kind: AccountKind;
  group: string;
  leverage: number;
  nickname: string;
  demoBalance: number;
  agree: boolean;
}

// prop* groups are for prop-challenge accounts only (bought under Prop challenges), never opened here; a group offers
// accounts of its own product only
const offers = (g: EngineGroup, kind: AccountKind, product: Product) => offeredGroups([g], kind, product).length > 0;
const money = (v: number, cent: boolean) => (cent ? `USC ${(v * 100).toLocaleString("en-US")}` : `$${v.toLocaleString("en-US")}`);

function ProductCard({ product, selected, onSelect }: { product: Product; selected: boolean; onSelect: () => void }) {
  const t = useT();
  const options = product === "options";
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      data-testid={`wizard-product-${product}`}
      className={cn(
        "k-card relative flex h-full flex-col overflow-hidden rounded-[24px] p-3 pb-5 text-start transition-[transform,box-shadow,border-color] duration-200 hover:-translate-y-0.5",
        selected && "!border-ember/70 shadow-[0_0_0_4px_color-mix(in_oklab,var(--k-ember)_18%,transparent)]",
      )}
    >
      <CardFace
        face={options ? "k-face-options" : "k-face-standard"}
        name={t(options ? "accounts.product.options" : "accounts.product.cfd")}
        badge={options ? t("accounts.product.chipOptions") : "CFD"}
        left={{ label: t.dyn("accounts.product.markets", "Markets"), value: options ? t.dyn("accounts.product.optionsMarkets", "FX · Gold · Oil") : t.dyn("accounts.product.cfdMarkets", "FX · Metals · Crypto") }}
        selected={selected}
      />
      <p className="mt-4 px-3 text-[13.5px] text-fg-2">{t(options ? "accounts.product.optionsText" : "accounts.product.cfdText")}</p>
    </button>
  );
}

/** The options intro on the wizard's Options path (once per client): the three ideas, "I understand", the terms. */
function OptionsIntro({ data, understood, onUnderstood, readOnly }: { data: Suitability; understood: boolean; onUnderstood: (v: boolean) => void; readOnly: boolean }) {
  const t = useT();
  const [terms, setTerms] = React.useState(false);
  const d = data.disclosure;
  return (
    <div className="mx-4 mb-6 rounded-[18px] border border-line bg-surface-2/50 px-4 py-4 sm:mx-6 sm:px-5" data-testid="wizard-options-intro">
      <div className="mb-3 text-[14px] font-medium text-fg">{t("accounts.wizard.introFirst")}</div>
      <IdeaCards />
      {readOnly && (
        <div className="mt-4 flex items-start gap-2.5 rounded-[14px] border border-warn/30 bg-warn-soft px-4 py-3 text-[13px] text-fg-2">
          <Lock className="mt-0.5 size-4 shrink-0 text-warn" />
          {t("options.readOnly")}
        </div>
      )}
      <label className={cn("mt-4 flex items-center gap-3 text-[15px] font-medium text-fg", readOnly || !d ? "cursor-not-allowed opacity-60" : "cursor-pointer")}>
        <input type="checkbox" className="size-5 shrink-0 accent-[var(--k-ember)]" checked={understood} disabled={readOnly || !d} onChange={(e) => onUnderstood(e.target.checked)} data-testid="wizard-options-understand" />
        {t("options.intro.confirm")}
      </label>
      <p className="mt-1.5 ps-8 text-[12.5px] leading-snug text-fg-3">
        {d ? (
          <>
            {t("options.intro.consent")}{" "}
            <button type="button" onClick={() => setTerms(true)} className="inline-flex items-center gap-1 font-medium text-ember underline-offset-4 hover:underline">
              <ScrollText className="size-3.5" /> {t("options.intro.terms")}
            </button>
          </>
        ) : (
          t("options.terms.unavailable")
        )}
      </p>
      {d && <TermsDialog data={data} open={terms} onOpenChange={setTerms} />}
    </div>
  );
}

function Summary({ cfg, g }: { cfg: Cfg; g: EngineGroup }) {
  const t = useT();
  const mode = t.dyn(`accounts.mode.${g.mode}`, modeLabel(g.mode));
  return (
    <Card className="overflow-hidden">
      {/* the card being opened, as it will look */}
      <div className="p-3">
        <CardFace
          face={faceOf(g.code, cfg.product === "options")}
          name={g.name}
          badge={`${cfg.kind === "live" ? t("accounts.badge.live") : t("accounts.badge.demo")} · ${cfg.product === "options" ? t("accounts.product.chipOptions") : mode}`}
          left={cfg.kind === "demo" ? { label: t("accounts.label.startBalance"), value: money(cfg.demoBalance, g.cent) } : { label: t("accounts.label.minFirstDeposit"), value: g.minDeposit > 0 ? `$${g.minDeposit.toLocaleString("en-US")}` : t("common.none") }}
          right={cfg.product === "options" ? undefined : { label: t("accounts.label.maxLeverage"), value: `1:${cfg.leverage}` }}
          selected
        />
      </div>
      <div className="px-6 pb-5">
        <KeyValue
          rows={[
            [t("accounts.wizard.step.product"), t(cfg.product === "options" ? "accounts.product.options" : "accounts.product.cfd")],
            [t("accounts.label.server"), <span key="s" className="font-mono">{serverOf({ type: cfg.kind })}</span>],
            [t("common.currency"), g.cent ? t("accounts.currency.uscUsCents") : "USD"],
            // leverage, position mode and per-lot pricing are CFD terms; options are priced per contract
            ...(cfg.product === "options"
              ? []
              : ([
                  [t("accounts.label.positionMode"), mode],
                  [t("accounts.label.leverage"), `1:${cfg.leverage.toLocaleString("en-US")}`],
                  [t("accounts.label.pricing"), spreadType(g, t)],
                  [t("accounts.label.commission"), commissionText(g, t)],
                ] as [string, React.ReactNode][])),
            [t("accounts.label.marginCallStopOut"), `${g.marginCallPct}% / ${g.stopOutPct}%`],
            cfg.kind === "demo" ? [t("accounts.label.startBalance"), money(cfg.demoBalance, g.cent)] : [t("accounts.label.minFirstDeposit"), g.minDeposit > 0 ? `$${g.minDeposit.toLocaleString("en-US")}` : t("common.none")],
            [t("accounts.label.nickname"), cfg.nickname || <span key="n" className="text-fg-3">—</span>],
          ]}
        />
        <div className="mt-3 flex items-center gap-2 text-[12px] text-fg-3">
          <Lock className="size-3.5 shrink-0" /> {t("accounts.summary.afterOpening")}
        </div>
      </div>
    </Card>
  );
}

/** "More options": leverage (CFD), the demo starting balance, nickname and currency, folded away with their defaults. */
function MoreOptions({ cfg, g, set }: { cfg: Cfg; g: EngineGroup; set: <K extends keyof Cfg>(k: K, v: Cfg[K]) => void }) {
  const t = useT();
  const [open, setOpen] = React.useState(false);
  const options = productOf(g) === "options";
  const balances = [...new Set([...DEMO_BALANCES, g.demoInitialBalance])].sort((x, y) => x - y);
  const chosen = [!options && `1:${cfg.leverage.toLocaleString("en-US")}`, cfg.kind === "demo" && money(cfg.demoBalance, g.cent), g.cent ? "USC" : "USD", cfg.nickname.trim()].filter(Boolean).join(" · ");
  return (
    <div className="k-row overflow-hidden p-0">
      <button type="button" aria-expanded={open} onClick={() => setOpen((o) => !o)} className="flex w-full items-center gap-3 px-4 py-3 text-start" data-testid="wizard-more-options">
        <span className="grid size-9 shrink-0 place-items-center rounded-full border border-line bg-surface-3 text-fg-2">
          <SlidersHorizontal className="size-4" />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-[14px] font-medium text-fg">{t("accounts.wizard.moreOptions")}</span>
          <span className="block truncate text-[12.5px] text-fg-3">{t("accounts.wizard.moreOptionsHint")}</span>
        </span>
        <span className="k-num hidden shrink-0 text-[12.5px] font-semibold text-fg-2 sm:inline">{chosen}</span>
        <ChevronDown className={cn("size-4 shrink-0 text-fg-3 transition-transform", open && "rotate-180")} />
      </button>
      {open && (
        <div className="space-y-6 border-t border-line px-4 pb-5 pt-4">
          {/* leverage is a CFD term: an Options account keeps its group's default */}
          {!options && (
            <div>
              <div className="mb-2 flex flex-wrap items-center justify-between gap-2 text-[12.5px] font-medium text-fg-2">
                {t("accounts.label.leverage")}
                <span className="font-normal text-fg-3">{t("accounts.wizard.leverageHint")}</span>
              </div>
              <div className="flex flex-wrap gap-2">
                {g.leverages.map((l) => (
                  <button
                    key={l}
                    type="button"
                    onClick={() => set("leverage", l)}
                    aria-pressed={cfg.leverage === l}
                    className={cn("k-num h-10 min-w-20 rounded-full border px-4 text-[13.5px] font-semibold transition-colors", cfg.leverage === l ? "border-ember/60 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:text-fg")}
                  >
                    1:{l.toLocaleString("en-US")}
                  </button>
                ))}
              </div>
              {cfg.leverage >= 1000 && (
                <div className="mt-2 flex items-center gap-2 text-[12px] text-warn">
                  <Info className="size-3.5" /> {t("accounts.wizard.highLeverage")}
                </div>
              )}
            </div>
          )}

          {cfg.kind === "demo" && (
            <div>
              <div className="mb-2 text-[12.5px] font-medium text-fg-2">{t("accounts.wizard.startingBalance")}</div>
              <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                {balances.map((b) => (
                  <button
                    key={b}
                    type="button"
                    onClick={() => set("demoBalance", b)}
                    aria-pressed={cfg.demoBalance === b}
                    className={cn("k-row flex flex-col items-start px-4 py-3 text-start transition-colors", cfg.demoBalance === b ? "border-gold/60 bg-gold-soft" : "hover:border-[var(--k-border-top)]")}
                  >
                    <span className={cn("k-num text-[17px] font-semibold", cfg.demoBalance === b ? "text-gold" : "text-fg")}>{money(b, g.cent)}</span>
                    <span className="text-[11.5px] text-fg-3">{t("accounts.wizard.virtualFunds")}</span>
                  </button>
                ))}
              </div>
            </div>
          )}

          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <Field label={t("accounts.label.nickname")} hint={t("accounts.wizard.nicknameHint")}>
              <Input value={cfg.nickname} maxLength={32} onChange={(e) => set("nickname", e.target.value)} placeholder={t("accounts.wizard.nicknamePlaceholder")} />
            </Field>
            <Field label={t("accounts.label.accountCurrency")}>
              <Input value={g.cent ? t("accounts.currency.uscLong") : t("accounts.currency.usdLong")} readOnly leading={<Wallet />} className="opacity-80" />
            </Field>
          </div>
        </div>
      )}
    </div>
  );
}

/** The Done screen: account number and server, Trade now, Fund (live), and the optional passwords. */
function Created({ res, g }: { res: OpenResult; g: EngineGroup }) {
  const t = useT();
  const a: EngineAccount = res.account;
  const [pw, setPw] = React.useState<"trading" | "investor" | null>(null);
  return (
    <Card className="overflow-hidden">
      <div className="grid grid-cols-1 gap-8 p-6 sm:p-8 lg:grid-cols-2 lg:items-center">
        <div>
          <span className="grid size-14 place-items-center rounded-full border border-up/30 bg-up-soft text-up">
            <Check className="size-7" />
          </span>
          <h2 className="mt-5 text-[26px] font-semibold tracking-tight">{t("accounts.created.title")}</h2>
          <p className="mt-2 text-[14px] text-fg-2">
            {a.type === "live"
              ? t("accounts.created.liveTextReady")
              : `${t("accounts.created.demoText", { amount: `${a.cent ? "USC " : "$"}${a.balance.toLocaleString("en-US", { minimumFractionDigits: 2 })}` })}${a.demo ? ` ${t("accounts.created.demoExpires", { days: a.demo.expiryDays })}` : ""}`}
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <Chip tone={a.type === "live" ? "ember" : "gold"}>{a.type === "live" ? t("accounts.badge.live") : t("accounts.badge.demo")}</Chip>
            {productOf(a) === "options" ? (
              <>
                <Chip tone="ember">{t("accounts.product.chipOptions")}</Chip>
                <Chip>{g.name}</Chip>
              </>
            ) : (
              <>
                <Chip>
                  {g.name} · {t.dyn(`accounts.mode.${a.mode}`, modeLabel(a.mode))}
                </Chip>
                <Chip>1:{a.leverage.toLocaleString("en-US")}</Chip>
              </>
            )}
            {a.cent && <Chip tone="gold">USC</Chip>}
          </div>
          <div className="mt-6 flex flex-wrap gap-2">
            <TradeButton a={a} size="lg" label={t("accounts.created.tradeNow")} data-testid="created-trade" />
            {a.type === "live" && <FundButton a={a} size="lg" />}
            <Link href={`/accounts/${a.login}`}>
              <Button variant="surface" size="lg">
                {t("accounts.created.viewAccount")}
              </Button>
            </Link>
          </div>
        </div>
        <div className="rounded-[20px] border border-line bg-surface-2/60 p-5">
          <div className="space-y-3">
            <SecretField label={t("accounts.created.accountNumber")} value={String(res.credentials.login ?? a.login)} />
            <SecretField label={t("accounts.label.server")} value={serverOf(a)} hint="GMT+3 / GMT+2" />
          </div>
          <div className="mt-5 border-t border-line pt-4">
            <div className="flex items-center gap-2 text-[14px] font-medium">
              <KeyRound className="size-4 text-fg-3" /> {t("accounts.created.passwordsTitle")}
            </div>
            <p className="mt-1.5 text-[12.5px] leading-snug text-fg-3">{t("accounts.created.passwordsText")}</p>
            <div className="mt-3 flex flex-wrap gap-2">
              <Button size="sm" variant="surface" onClick={() => setPw("trading")} data-testid="created-set-trading">
                <KeyRound /> {t("accountDetail.creds.setTrading")}
              </Button>
              <Button size="sm" variant="surface" onClick={() => setPw("investor")} data-testid="created-set-investor">
                <Eye /> {t("accountDetail.creds.setInvestor")}
              </Button>
            </div>
          </div>
        </div>
      </div>
      {pw && <ChangePasswordDialog a={a} kind={pw} open={!!pw} onOpenChange={(o) => !o && setPw(null)} />}
    </Card>
  );
}

function Wizard() {
  const t = useT();
  const sp = useSearchParams();
  const groupsQ = useGroups();
  const accountsQ = useAccounts(0);
  const groups = React.useMemo(() => groupsQ.data?.groups ?? [], [groupsQ.data]);
  const accounts = accountsQ.data?.accounts ?? [];

  // the broker can switch new demo accounts off (Back Office › Settings › Features); the BFF refuses them too
  const features = useFeatures();
  const demoOn = features?.flags.demo_accounts !== false;
  // and the Options module (then only CFD accounts are offered, no Product step; the BFF refuses Options accounts too)
  const optionsOn = optionsModuleOn(features?.modules);
  const steps: Step[] = optionsOn ? ["product", "account", "done"] : ["account", "done"];
  const readOnly = useReadOnly();
  const suit = useSuitability();
  const [understood, setUnderstood] = React.useState(false);
  const [step, setStep] = React.useState<Step>(steps[0]!);
  const [dir, setDir] = React.useState(1);
  const [cfg, setCfg] = React.useState<Cfg>({
    product: optionsOn && parseProduct(sp.get("product")) === "options" ? "options" : "cfd",
    kind: demoOn && sp.get("type") === "demo" ? "demo" : "live",
    group: sp.get("group") ?? "",
    leverage: 0,
    nickname: "",
    demoBalance: 10000,
    agree: false,
  });
  const [busy, setBusy] = React.useState(false);
  const [created, setCreated] = React.useState<OpenResult | null>(null);
  const set = <K extends keyof Cfg>(k: K, v: Cfg[K]) => setCfg((c) => ({ ...c, [k]: v }));

  const product: Product = optionsOn ? cfg.product : "cfd";
  const available = offeredGroups(groups, cfg.kind, product);
  const g = groups.find((x) => x.code === cfg.group && offers(x, cfg.kind, product)) ?? defaultGroup(available);
  const options = product === "options";
  // the Options path: the options intro first, once (accepting the options terms, the same as on the Options page)
  const needsIntro = options && !suit.data?.eligible;

  // once groups load: honour ?group= (its product), otherwise preselect the Standard type; a link that already names
  // the product or the type goes straight to the Account step (unless the options intro is still to be accepted)
  const [booted, setBooted] = React.useState(false);
  React.useEffect(() => {
    if (booted || !groups.length) return;
    setBooted(true);
    const want = groups.find((x) => x.code === sp.get("group"));
    const wantProduct = want && optionsOn ? productOf(want) : product;
    const pick = want && offers(want, cfg.kind, wantProduct) ? want : defaultGroup(offeredGroups(groups, cfg.kind, wantProduct));
    if (!pick) return;
    setCfg((c) => ({ ...c, product: wantProduct, group: pick.code, leverage: pick.defaultLeverage, demoBalance: pick.demoInitialBalance }));
    const linked = (want && want.code === pick.code) || parseProduct(sp.get("product")) !== null;
    if (linked && !(wantProduct === "options" && !suit.data?.eligible)) setStep("account");
  }, [booted, groups, sp, cfg.kind, optionsOn, product, suit.data?.eligible]);

  const pickGroup = (x: EngineGroup) =>
    setCfg((c) => ({
      ...c,
      group: x.code,
      leverage: x.leverages.includes(c.leverage) ? c.leverage : x.defaultLeverage,
      demoBalance: c.demoBalance || x.demoInitialBalance,
    }));

  // keep the chosen group valid for the chosen product and account kind (switching live / demo keeps the type when
  // that kind offers it, else the Standard type)
  React.useEffect(() => {
    if (booted && g && g.code !== cfg.group) pickGroup(g);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [booted, g?.code, cfg.group]);

  // the engine's account limit counts per (live / demo, product)
  const used = usedFor(accounts, cfg.kind, product);
  const full = g ? used >= g.maxAccountsPerUser : false;
  const introOk = !needsIntro || (!!suit.data?.disclosure && understood && !readOnly);
  const canNext = step === "product" ? (options ? !!suit.data && introOk : true) : step === "account" ? !!g && !full && g.leverages.includes(cfg.leverage) && cfg.agree : true;

  const go = (d: number) => {
    setDir(d);
    setStep((s) => steps[Math.max(0, Math.min(steps.length - 1, steps.indexOf(s) + d))]!);
  };

  const pickProduct = (p: Product) => {
    const first = defaultGroup(offeredGroups(groups, cfg.kind, p));
    setCfg((c) => ({ ...c, product: p, agree: c.product === p && c.agree, group: first?.code ?? "", leverage: first?.defaultLeverage ?? c.leverage, demoBalance: first?.demoInitialBalance ?? c.demoBalance }));
  };

  // leaving the product step on the Options path records the options intro first (once)
  const [accepting, setAccepting] = React.useState(false);
  const next = async () => {
    if (step === "product" && needsIntro) {
      const d = suit.data?.disclosure;
      if (!d) return;
      setAccepting(true);
      try {
        suit.set(await suitabilityApi<Suitability>("options/accept", { version: d.version }));
        toast.success(t("options.intro.toastStarted"));
      } catch (e) {
        if (e instanceof SuitabilityError && e.code === "disclosure_outdated") {
          toast.warning(t("options.intro.toastUpdated"));
          suit.reload();
        } else toast.error(t("options.intro.toastFailed"), { description: e instanceof Error ? e.message : undefined });
        return;
      } finally {
        setAccepting(false);
      }
    }
    go(1);
  };

  // no password: the engine generates its own and the BFF keeps them (the client sets one later if they want)
  const create = async () => {
    if (!g) return;
    setBusy(true);
    try {
      const res = await tradingApi<OpenResult>("accounts", {
        body: {
          type: cfg.kind,
          group: g.code,
          leverage: cfg.leverage,
          name: cfg.nickname.trim() || undefined,
          initialBalance: cfg.kind === "demo" ? cfg.demoBalance : undefined,
        },
      });
      setCreated(res);
      go(1);
      toast.success(t(cfg.kind === "live" ? "accounts.wizard.openedLive" : "accounts.wizard.openedDemo", { login: res.credentials.login }), {
        description: options ? `${t("accounts.product.options")} · ${g.name}` : `${g.name} · ${t.dyn(`accounts.mode.${g.mode}`, modeLabel(g.mode))} · 1:${cfg.leverage}`,
      });
    } catch (e) {
      const msg = e instanceof ApiError ? e.message : t("common.errorRetry");
      toast.error(t("accounts.wizard.openFailed"), { description: msg });
    } finally {
      setBusy(false);
    }
  };

  if (groupsQ.error && !groupsQ.data)
    return (
      <div className="pb-16">
        <PageHeader title={t("accounts.wizard.title")} />
        <Card>
          <EmptyState
            art="connectionLost"
            title={t("accounts.wizard.unavailableTitle")}
            text={t("accounts.wizard.unavailableText")}
            action={
              <Button variant="surface" onClick={groupsQ.reload}>
                <RotateCw /> {t("common.retry")}
              </Button>
            }
          />
        </Card>
      </div>
    );

  const first = steps.indexOf(step) === 0;

  return (
    <div className="pb-16">
      <PageHeader
        title={t("accounts.wizard.title")}
        subtitle={t("accounts.wizard.subtitleQuick")}
        actions={
          <Link href="/accounts">
            <Button variant="surface">
              <ArrowLeft className="rtl:-scale-x-100" /> {t("accounts.list.myAccounts")}
            </Button>
          </Link>
        }
      />

      <Reveal>
        <Card className="mb-4 px-5 py-4 sm:px-6">
          <Stepper steps={steps.map((k) => t(STEP_LABEL[k]))} current={steps.indexOf(step)} />
        </Card>
      </Reveal>

      {!groupsQ.data ? (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
          <Skeleton className="h-[420px] rounded-[20px] xl:col-span-8" />
          <Skeleton className="hidden h-[420px] rounded-[20px] xl:col-span-4 xl:block" />
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
          <div className={cn(step === "done" ? "xl:col-span-12" : "xl:col-span-8")}>
            <AnimatePresence mode="wait" custom={dir}>
              <motion.div key={step} initial={{ opacity: 0, x: dir * 20 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: dir * -20 }} transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}>
                {step === "product" && (
                  <Card>
                    <CardHeader title={t("accounts.wizard.productTitle")} subtitle={t("accounts.wizard.productSubtitle")} />
                    <div className="grid grid-cols-1 gap-4 px-4 pb-6 pt-4 sm:grid-cols-2 sm:px-6">
                      <ProductCard product="cfd" selected={product === "cfd"} onSelect={() => pickProduct("cfd")} />
                      <ProductCard product="options" selected={product === "options"} onSelect={() => pickProduct("options")} />
                    </div>
                    {needsIntro && suit.data && <OptionsIntro data={suit.data} understood={understood} onUnderstood={setUnderstood} readOnly={readOnly} />}
                    {needsIntro && !suit.data && !suit.error && <Skeleton className="mx-4 mb-6 h-40 rounded-[18px] sm:mx-6" />}
                    {needsIntro && suit.error && (
                      <div className="mx-4 mb-6 flex items-center justify-between gap-3 rounded-[14px] border border-warn/30 bg-warn-soft px-4 py-3 text-[13px] text-fg-2 sm:mx-6">
                        {t("options.error.load")}
                        <Button size="sm" variant="surface" onClick={suit.reload}>
                          <RotateCw /> {t("options.error.retry")}
                        </Button>
                      </div>
                    )}
                  </Card>
                )}

                {step === "account" && (
                  <Card>
                    <CardHeader title={t("accounts.wizard.chooseTitle")} subtitle={t("accounts.wizard.chooseSubtitle")} />
                    <div className="space-y-6 px-4 pb-6 pt-4 sm:px-6">
                      {demoOn && (
                        <div>
                          <Segmented
                            block
                            size={36}
                            aria-label={t("accounts.wizard.step.account")}
                            value={cfg.kind}
                            // the acknowledgement differs for live and demo: ticked again for the other
                            onChange={(v) => setCfg((c) => (c.kind === v ? c : { ...c, kind: v, agree: false }))}
                            options={[
                              { value: "live" as const, label: t("common.live") },
                              { value: "demo" as const, label: t("common.demo") },
                            ]}
                          />
                          <p className="mt-2 text-[13px] text-fg-3">
                            {cfg.kind === "live" ? t("accounts.kind.liveText") : t("accounts.kind.demoText")}
                            {cfg.kind === "demo" && g ? ` ${t("accounts.wizard.refillNote", { count: g.demoRefillsPerDay, days: g.demoExpiryDays })}` : ""}
                          </p>
                        </div>
                      )}

                      <div>
                        <div className="mb-3 text-[12.5px] font-medium text-fg-2">{t("accounts.wizard.typeLabel")}</div>
                        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 2xl:grid-cols-3">
                          {available.map((x) => (
                            <EngineGroupCard key={x.code} g={x} kind={cfg.kind} used={used} selected={g?.code === x.code} onSelect={() => pickGroup(x)} />
                          ))}
                        </div>
                        {available.length === 0 && <EmptyState art="welcome" title={t("accounts.wizard.noGroups")} />}
                      </div>

                      {g && <MoreOptions cfg={{ ...cfg, product }} g={g} set={set} />}

                      {g && (
                        <>
                          <ul className="space-y-1.5 text-[13px] text-fg-2">
                            {cfg.kind === "live" && (
                              <li className="flex items-start gap-2">
                                <Check className="mt-0.5 size-3.5 shrink-0 text-up" /> {t("accounts.wizard.review.zeroBalance")} {g.minDeposit > 0 ? t("accounts.wizard.review.minDeposit", { amount: `$${g.minDeposit.toLocaleString("en-US")}` }) : ""}
                              </li>
                            )}
                            <li className="flex items-start gap-2">
                              <Check className="mt-0.5 size-3.5 shrink-0 text-up" /> {t("accounts.wizard.review.fixed", { mode: t.dyn(`accounts.mode.${g.mode}`, modeLabel(g.mode)), currency: g.cent ? "USC" : "USD" })}
                            </li>
                            <li className="flex items-start gap-2">
                              <Check className="mt-0.5 size-3.5 shrink-0 text-up" /> {t("accounts.wizard.review.noPassword")}
                            </li>
                          </ul>
                          <label className="flex cursor-pointer items-start gap-3 text-[13px] text-fg-2">
                            <input type="checkbox" checked={cfg.agree} onChange={(e) => set("agree", e.target.checked)} className="mt-0.5 size-4 accent-[var(--k-ember)]" data-testid="wizard-agree" />
                            <span>{cfg.kind === "live" ? t(options ? "accounts.wizard.agreeLiveOptions" : "accounts.wizard.agreeLive") : t("accounts.wizard.agreeDemo")}</span>
                          </label>
                        </>
                      )}
                    </div>
                  </Card>
                )}

                {step === "done" && created && g && <Created res={created} g={g} />}
              </motion.div>
            </AnimatePresence>

            {step !== "done" && (
              <div className="mt-4 flex items-center justify-between gap-3">
                <Button variant="ghost" onClick={() => go(-1)} disabled={first || busy}>
                  <ArrowLeft className="rtl:-scale-x-100" /> {t("common.back")}
                </Button>
                {step === "product" ? (
                  <Button variant="ember" size="lg" onClick={() => void next()} disabled={!canNext || accepting} data-testid="wizard-continue">
                    {t("common.continue")} <ArrowRight className="rtl:-scale-x-100" />
                  </Button>
                ) : (
                  <Button variant="ember" size="lg" disabled={!canNext || busy} onClick={create} data-testid="wizard-open">
                    {busy ? t("accounts.wizard.opening") : cfg.kind === "live" ? t("accounts.wizard.openLive") : t("accounts.wizard.openDemo")} {!busy && <Check />}
                  </Button>
                )}
              </div>
            )}
          </div>

          {step !== "done" && g && (
            <div className="hidden xl:col-span-4 xl:block">
              <div className="sticky top-24">
                <Summary cfg={{ ...cfg, product }} g={g} />
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function LiveOpenAccount() {
  return (
    <React.Suspense fallback={null}>
      <Wizard />
    </React.Suspense>
  );
}
