"use client";

import type { ReactNode } from "react";
import { motion } from "motion/react";
import { useT } from "@kalks/i18n/react";
import { Delta, LanguageMenu, Logo, PriceText, SymbolAvatar, useBrand, useQuote } from "@/components/kit";

/** The brand photo of the sign-in pages (founder 2026-10-10: the same photo heroes as the rest of the Client Area). */
const AUTH_PHOTO = "/heroes/dashboard.jpg";

/**
 * Split auth layout: the form on the left, the brand photo on the right (the KALKS letters behind the figure, as on
 * Home) with the logo, the headline and live prices over it. Phones get the photo as a band above the form.
 * Pages render their own form as `children`, so every auth flow (sign in, sign up, reset) shares it.
 * A white-label broker sees its own logo over a plain dark panel (no Kalks photo).
 */
export function AuthSection({ children }: { children: ReactNode }) {
  const t = useT();
  const brand = useBrand();
  return (
    <section className="min-h-dvh bg-bg p-3 text-fg antialiased [font-synthesis:none]">
      <div className="grid min-h-[calc(100dvh-1.5rem)] gap-3 lg:grid-cols-[0.94fr_1.06fr]">
        {/* Left — form */}
        <div className="relative flex flex-col overflow-hidden rounded-[28px] border border-line bg-surface">
          {/* phones: the photo as a band at the top */}
          {!brand && (
            <div aria-hidden className="relative -mb-16 h-56 overflow-hidden lg:hidden">
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img src={AUTH_PHOTO} alt="" className="size-full object-cover" style={{ objectPosition: "50% 35%" }} />
              <div className="absolute inset-0 bg-gradient-to-b from-black/30 via-transparent to-surface" />
            </div>
          )}
          <header className="relative flex items-center justify-between px-6 pt-6 lg:px-10">
            <Logo height={20} className="text-white lg:invisible" />
            <div className="flex items-center gap-2">
              <LanguageMenu />
            </div>
          </header>
          <div className="relative flex flex-1 items-center justify-center px-6 py-10 lg:px-14 lg:py-14 xl:px-20">
            <motion.div
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1] }}
              className="mx-auto w-full max-w-[460px]"
            >
              {children}
            </motion.div>
          </div>
          <p className="relative px-6 pb-6 text-[11px] leading-relaxed text-fg-3 lg:px-10">
            {t("common.riskWarning")}
          </p>
        </div>

        {/* Right — brand photo */}
        <BrandPanel />
      </div>
    </section>
  );
}

function BrandPanel() {
  const brand = useBrand();
  const t = useT();
  return (
    <div className="relative hidden min-h-[720px] flex-col justify-between overflow-hidden rounded-[28px] bg-black p-8 text-white sm:p-12 lg:flex lg:min-h-0 lg:p-14">
      {!brand && (
        <>
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src={AUTH_PHOTO} alt="" aria-hidden className="absolute inset-0 size-full object-cover" style={{ objectPosition: "50% 38%" }} />
          {/* legibility: a soft top for the logo, a deep foot for the headline */}
          <div aria-hidden className="absolute inset-x-0 top-0 h-48 bg-gradient-to-b from-black/55 to-transparent" />
          <div aria-hidden className="absolute inset-x-0 bottom-0 h-[62%] bg-gradient-to-t from-black via-black/75 to-transparent" />
        </>
      )}
      {brand && <div aria-hidden className="pointer-events-none absolute inset-0 bg-[radial-gradient(900px_520px_at_15%_-10%,color-mix(in_oklab,var(--k-ember)_42%,transparent),transparent_62%)]" />}

      <motion.div
        initial={{ opacity: 0, y: 12, filter: "blur(6px)" }}
        animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
        transition={{ duration: 0.7, ease: [0.22, 1, 0.36, 1] }}
        className="relative z-10"
      >
        <Logo height={26} className="text-white" />
      </motion.div>

      <div className="relative z-10 max-w-[520px]">
        <motion.h2
          initial={{ opacity: 0, y: 18, filter: "blur(8px)" }}
          animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
          transition={{ duration: 0.8, delay: 0.12, ease: [0.22, 1, 0.36, 1] }}
          className="text-[30px] font-light leading-[1.08] tracking-[-0.035em] text-white lg:text-[40px]"
        >
          {t("auth.brand.headline")}
        </motion.h2>
        <motion.p
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.7, delay: 0.2 }}
          className="mt-4 max-w-md text-[14.5px] leading-relaxed text-white/75"
        >
          {t("auth.brand.body")}
        </motion.p>
        <motion.div initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.7, delay: 0.28 }} className="mt-7 flex flex-wrap gap-2">
          {["XAUUSD", "EURUSD", "BTCUSD"].map((s) => (
            <Ticker key={s} symbol={s} />
          ))}
        </motion.div>
      </div>
    </div>
  );
}

function Ticker({ symbol }: { symbol: string }) {
  const q = useQuote(symbol);
  return (
    <div className="flex items-center gap-2.5 rounded-full border border-white/15 bg-black/45 py-1.5 ps-1.5 pe-3 backdrop-blur-xl">
      <SymbolAvatar symbol={symbol} size={22} />
      <span className="text-[12px] font-medium text-white/85">{symbol}</span>
      <PriceText symbol={symbol} value={q.bid} dir={q.dir} className="text-[12px] [&_span]:!text-white/90" />
      <Delta value={q.change} className="text-[11px]" />
    </div>
  );
}
