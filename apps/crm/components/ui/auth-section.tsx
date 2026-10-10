"use client";

import type { ReactNode } from "react";
import { FlutedGlass } from "@paper-design/shaders-react";
import { motion } from "motion/react";
import { useT } from "@kalks/i18n/react";
import { Delta, LanguageMenu, Logo, PriceText, SymbolAvatar, useBrand, useQuote } from "@/components/kit";

/**
 * Split auth layout (adapted from "auth-section-3"): the form card on the left,
 * a fluted-glass brand panel with a tilted live product preview on the right.
 * Pages render their own form as `children`, so every auth flow (sign in, sign up, reset) shares it.
 */
export function AuthSection({ children }: { children: ReactNode }) {
  const t = useT();
  return (
    <section className="min-h-dvh bg-bg p-3 text-fg antialiased [font-synthesis:none]">
      <div className="grid min-h-[calc(100dvh-1.5rem)] gap-3 lg:grid-cols-[0.94fr_1.06fr]">
        {/* Left — form */}
        <div className="relative flex flex-col overflow-hidden rounded-md border border-line bg-surface">
          <header className="relative flex items-center justify-between px-6 pt-6 lg:px-10">
            <Logo height={20} className="lg:invisible" />
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

        {/* Right — brand panel */}
        <BrandPanel />
      </div>
    </section>
  );
}

function BrandPanel() {
  const brand = useBrand();
  const t = useT();
  return (
    <div className="relative hidden min-h-[720px] flex-col overflow-hidden rounded-md bg-linear-to-b from-black via-[#140906] to-bg p-8 text-white sm:p-12 lg:flex lg:min-h-0 lg:p-16">
      {/* ember glow + fluted glass shader */}
      <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(900px_520px_at_15%_-10%,color-mix(in_oklab,var(--k-ember)_42%,transparent),transparent_62%)]" />
      <div className="pointer-events-none absolute inset-0 z-0">
        <FlutedGlass
          size={0.89}
          shape="lines"
          angle={0}
          distortionShape="prism"
          distortion={0.5}
          shift={0}
          blur={0}
          edges={0.25}
          stretch={0}
          scale={1.11}
          fit="cover"
          highlights={0.1}
          shadows={0.2}
          grainMixer={0.1}
          grainOverlay={0.1}
          colorBack="#00000000"
          colorHighlight="#FFFFFF"
          colorShadow="#000000"
          className="h-full w-full bg-transparent"
        />
      </div>

      <div className="relative z-10 h-full w-full">
        <div className="max-w-[480px] lg:pt-6">
          <motion.div
            initial={{ opacity: 0, y: 12, filter: "blur(6px)" }}
            animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
            transition={{ duration: 0.7, ease: [0.22, 1, 0.36, 1] }}
          >
            <Logo height={26} className="text-white" />
          </motion.div>
          <motion.h2
            initial={{ opacity: 0, y: 18, filter: "blur(8px)" }}
            animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
            transition={{ duration: 0.8, delay: 0.12, ease: [0.22, 1, 0.36, 1] }}
            className="mt-10 text-2xl font-light leading-tight tracking-[-0.035em] text-white/90 sm:text-3xl lg:text-[34px]"
          >
            {t("auth.brand.headline")}
          </motion.h2>
          <motion.p
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.7, delay: 0.2 }}
            className="mt-4 max-w-md text-[14.5px] leading-relaxed text-white/60"
          >
            {t("auth.brand.body")}
          </motion.p>
          <motion.div initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.7, delay: 0.28 }} className="mt-7 flex flex-wrap gap-2">
            {["XAUUSD", "EURUSD", "BTCUSD"].map((s) => (
              <Ticker key={s} symbol={s} />
            ))}
          </motion.div>
        </div>

        {/* Tilted live product preview (Kalks' own product shots: not shown for a white-label broker) */}
        {!brand && (
          <div className="mt-10 w-full overflow-hidden rounded-2xl border border-white/15 bg-black/70 p-2 shadow-[0_30px_90px_rgba(0,0,0,0.5)] backdrop-blur-xl lg:absolute lg:-bottom-28 lg:left-[12%] lg:mt-0 lg:w-[105%] lg:max-w-none lg:origin-bottom-left lg:-rotate-3 xl:-bottom-[150px] xl:left-[14%] xl:w-[108%] 2xl:-bottom-[170px] 2xl:w-[112%]">
            <motion.div
              initial={{ opacity: 0, y: 72, filter: "blur(10px)" }}
              animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
              transition={{ duration: 1, delay: 0.22, ease: [0.16, 1, 0.3, 1] }}
              className="overflow-hidden rounded-xl border border-white/10 bg-black"
            >
              <div className="flex select-none items-center gap-1.5 border-b border-white/10 bg-black/40 px-4 py-3">
                <div className="size-2 rounded-full bg-white/35" />
                <div className="size-2 rounded-full bg-white/25" />
                <div className="size-2 rounded-full bg-white/15" />
                <span className="ml-4 font-mono text-[9px] tracking-wider text-white/40">app.kalkstrade.com</span>
              </div>
              {/* the current Client Area home (2026-10-10 screenshot; the old Kalks 2 dashboard image is retired) */}
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img src="/shots/client-area-home.webp" alt={t("auth.brand.previewAlt")} width={1600} height={974} className="h-auto w-full object-cover object-top opacity-95" />
            </motion.div>
          </div>
        )}
      </div>
    </div>
  );
}

function Ticker({ symbol }: { symbol: string }) {
  const q = useQuote(symbol);
  return (
    <div className="flex items-center gap-2.5 rounded-full border border-white/12 bg-black/40 py-1.5 ps-1.5 pe-3 backdrop-blur-xl">
      <SymbolAvatar symbol={symbol} size={22} />
      <span className="text-[12px] font-medium text-white/85">{symbol}</span>
      <PriceText symbol={symbol} value={q.bid} dir={q.dir} className="text-[12px] [&_span]:!text-white/90" />
      <Delta value={q.change} className="text-[11px]" />
    </div>
  );
}
