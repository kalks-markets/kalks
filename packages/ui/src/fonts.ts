// Kalks 2 type (docs/design/KALKS2.md §3), self-hosted by next/font at build time. Root layouts put
// `fontVariables` on <html>; styles.css builds the stacks from these variables and picks the script's family from
// <html lang> (the language getI18n() resolves). Only the Latin files of the three main families are preloaded;
// the script fallbacks load when a page actually shows those characters (unicode-range).
//
// Server-only module: import it from a root layout, never from a client component.
import {
  Archivo,
  Be_Vietnam_Pro,
  Golos_Text,
  Instrument_Sans,
  JetBrains_Mono,
  Noto_Kufi_Arabic,
  Noto_Sans_Arabic,
  Noto_Sans_Bengali,
  Noto_Sans_Devanagari,
  Noto_Sans_Tamil,
  Noto_Sans_Thai,
  Unbounded,
} from "next/font/google";

/** Display: wide and heavy for headlines (wdth 125 / 800), narrower and tabular for money (wdth 108 / 700). */
const archivo = Archivo({ subsets: ["latin"], axes: ["wdth"], display: "swap", variable: "--font-archivo" });
/** Text: crisp, slightly narrow, good in dense tables. */
const instrument = Instrument_Sans({ subsets: ["latin"], axes: ["wdth"], display: "swap", variable: "--font-instrument" });
/** Prices, lots, IDs, times (covers Cyrillic too). */
const jetbrains = JetBrains_Mono({ subsets: ["latin"], display: "swap", variable: "--font-jetbrains" });

// script fallbacks: never preloaded
const golos = Golos_Text({ subsets: ["cyrillic"], display: "swap", preload: false, variable: "--font-golos" });
const unbounded = Unbounded({ subsets: ["cyrillic"], display: "swap", preload: false, variable: "--font-unbounded" });
const beVietnam = Be_Vietnam_Pro({ subsets: ["vietnamese"], weight: ["400", "500", "600", "700", "800"], display: "swap", preload: false, variable: "--font-be-vietnam" });
const notoArabic = Noto_Sans_Arabic({ subsets: ["arabic"], display: "swap", preload: false, variable: "--font-noto-arabic" });
const notoKufi = Noto_Kufi_Arabic({ subsets: ["arabic"], display: "swap", preload: false, variable: "--font-noto-kufi" });
const notoDevanagari = Noto_Sans_Devanagari({ subsets: ["devanagari"], display: "swap", preload: false, variable: "--font-noto-devanagari" });
const notoBengali = Noto_Sans_Bengali({ subsets: ["bengali"], display: "swap", preload: false, variable: "--font-noto-bengali" });
const notoTamil = Noto_Sans_Tamil({ subsets: ["tamil"], display: "swap", preload: false, variable: "--font-noto-tamil" });
const notoThai = Noto_Sans_Thai({ subsets: ["thai"], display: "swap", preload: false, variable: "--font-noto-thai" });

/** Class list for <html>: defines every --font-* variable the type stacks use. */
export const fontVariables = [
  archivo,
  instrument,
  jetbrains,
  golos,
  unbounded,
  beVietnam,
  notoArabic,
  notoKufi,
  notoDevanagari,
  notoBengali,
  notoTamil,
  notoThai,
]
  .map((f) => f.variable)
  .join(" ");
