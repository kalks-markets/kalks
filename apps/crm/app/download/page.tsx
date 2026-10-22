import type { Metadata } from "next";
import Link from "next/link";
import { cookies, headers } from "next/headers";
import { QRCodeSVG } from "qrcode.react";
import { ArrowLeft, Download, Hourglass, ShieldCheck, Smartphone } from "lucide-react";
import { Logo } from "@kalks/ui/logo";
import { Card, Chip } from "@kalks/ui/primitives";
import { getFormatter, getT } from "@kalks/i18n/server";
import { SESSION_COOKIE } from "@/lib/gateway";
import { latestRelease, formatSize } from "@/lib/app-download";

// Public Android app page: app.kalkstrade.com/download. No sign-in (proxy.ts lets /download through) and outside the
// (app) group, so signed-out visitors and clients see the same page. The release comes from latest.json next to the
// APK (lib/app-download.ts, deploy/README.md); without a valid manifest the page says "coming soon" and links nothing.

export const dynamic = "force-dynamic";

const ICON = "/brand/icon-192.png";

async function origin(): Promise<string> {
  const h = await headers();
  const host = (h.get("x-forwarded-host") ?? h.get("host") ?? "localhost:3000").split(",")[0]!.trim();
  const proto = h.get("x-forwarded-proto") ?? (host.startsWith("localhost") || host.startsWith("127.") ? "http" : "https");
  return `${proto === "https" ? "https" : "http"}://${host}`;
}

export async function generateMetadata(): Promise<Metadata> {
  const [t, base] = await Promise.all([getT(), origin()]);
  const title = t("shell.download.metaTitle");
  const description = t("shell.download.metaDescription");
  const url = `${base}/download`;
  const image = `${base}/brand/icon-512.png`;
  return {
    title: { absolute: title },
    description,
    alternates: { canonical: url },
    robots: { index: true, follow: true },
    openGraph: { type: "website", url, title, description, siteName: "Kalks", images: [{ url: image, width: 512, height: 512, alt: "Kalks" }] },
    twitter: { card: "summary", title, description, images: [image] },
  };
}

export default async function DownloadPage() {
  const base = await origin();
  const [t, f, release, jar] = await Promise.all([getT(), getFormatter(), latestRelease(base), cookies()]);
  const signedIn = !!jar.get(SESSION_COOKIE)?.value;
  const facts: [string, string][] = release
    ? [
        [t("shell.download.version"), `${release.version} (${release.build})`],
        [t("shell.download.size"), formatSize(release.size, f.number)],
        [t("shell.download.released"), f.date(release.releasedAt, { day: "2-digit", month: "short", year: "numeric", timeZone: "UTC" })],
        [t("shell.download.requires"), t("shell.download.androidVersion", { version: release.minAndroid })],
      ]
    : [];

  return (
    <div className="min-h-dvh bg-bg text-fg">
      <header className="border-b border-line">
        <div className="mx-auto flex h-16 max-w-5xl items-center justify-between px-4 sm:px-6">
          <Link href="/" aria-label="Kalks">
            <Logo height={20} className="text-fg" />
          </Link>
          {signedIn ? (
            <Link href="/" className="flex items-center gap-1.5 text-[13px] text-fg-2 hover:text-fg">
              <ArrowLeft className="size-4 rtl:-scale-x-100" /> {t("shell.download.backToClientArea")}
            </Link>
          ) : (
            <Link href="/login" className="text-[13px] font-medium text-fg-2 hover:text-fg">
              {t("shell.download.signIn")}
            </Link>
          )}
        </div>
      </header>

      <main className="mx-auto grid max-w-5xl grid-cols-1 gap-6 px-4 py-8 sm:px-6 sm:py-12 lg:grid-cols-[1.45fr_1fr]">
        <div className="min-w-0 space-y-6">
          <Card className="p-6 sm:p-8">
            <div className="flex items-center gap-4">
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img src={ICON} alt="" width={64} height={64} className="size-16 shrink-0 rounded-[18px] border border-line" />
              <div className="min-w-0">
                <h1 className="text-[24px] font-medium tracking-[-0.02em] sm:text-[28px]">{t("shell.download.title")}</h1>
                <p className="mt-1 text-[14px] text-fg-3">{t("shell.download.subtitle")}</p>
              </div>
            </div>

            {release ? (
              <>
                <a
                  href={release.url}
                  download={release.file}
                  data-testid="download-apk"
                  className="k-btn k-btn-primary k-btn-56 k-btn-block mt-7 gap-2 text-[16px]"
                >
                  <Download className="size-5" /> {t("shell.download.button")}
                  <span className="k-num font-normal opacity-75">· {formatSize(release.size, f.number)}</span>
                </a>
                <dl className="mt-6 grid grid-cols-2 gap-x-6 gap-y-4 text-[13.5px] sm:grid-cols-4">
                  {facts.map(([k, v]) => (
                    <div key={k} className="min-w-0">
                      <dt className="text-[12px] text-fg-3">{k}</dt>
                      <dd className="k-num mt-0.5 font-medium">{v}</dd>
                    </div>
                  ))}
                </dl>
              </>
            ) : (
              <div data-testid="download-soon" className="mt-7 flex items-start gap-3 rounded-[16px] border border-warn/30 bg-warn-soft px-5 py-4">
                <Hourglass className="mt-0.5 size-5 shrink-0 text-warn" />
                <div>
                  <div className="text-[15px] font-medium">{t("shell.download.soonTitle")}</div>
                  <div className="mt-0.5 text-[13px] text-fg-2">{t("shell.download.soonText")}</div>
                </div>
              </div>
            )}
          </Card>

          <Card className="p-6 sm:p-8">
            <h2 className="text-[17px] font-medium tracking-tight">{t("shell.download.howTitle")}</h2>
            <ol className="mt-4 space-y-3">
              {(["step1", "step2", "step3", "step4"] as const).map((s, i) => (
                <li key={s} className="flex gap-3 text-[14px] leading-relaxed text-fg-2">
                  <span className="k-num grid size-6 shrink-0 place-items-center rounded-full bg-surface-3 text-[12px] font-semibold text-fg">{i + 1}</span>
                  <span>{t(`shell.download.${s}`)}</span>
                </li>
              ))}
            </ol>
            <p className="mt-5 flex items-start gap-2 text-[12.5px] text-fg-3">
              <ShieldCheck className="mt-px size-4 shrink-0 text-up" />
              {t("shell.download.officialNote")}
            </p>
          </Card>
        </div>

        <div className="min-w-0 space-y-6">
          {release && (
            <Card className="hidden p-6 md:block">
              <h2 className="text-[15px] font-medium">{t("shell.download.scanTitle")}</h2>
              <p className="mt-1 text-[12.5px] text-fg-3">{t("shell.download.scanText")}</p>
              <div className="mt-4 w-fit rounded-[16px] bg-white p-3">
                <QRCodeSVG value={`${base}/download`} size={168} level="M" bgColor="#ffffff" fgColor="#000000" title={t("shell.download.scanTitle")} />
              </div>
            </Card>
          )}
          {release && (
            <Card className="p-6">
              <h2 className="text-[15px] font-medium">{t("shell.download.verifyTitle")}</h2>
              <p className="mt-1 text-[12.5px] text-fg-3">{t("shell.download.verifyText")}</p>
              <div className="mt-3 flex items-center gap-2">
                <Chip size="sm" tone="neutral">
                  SHA-256
                </Chip>
              </div>
              <code data-testid="download-sha256" className="mt-2 block select-all break-all rounded-[12px] bg-surface-2 px-3 py-2.5 font-mono text-[12px] leading-relaxed text-fg-2">
                {release.sha256}
              </code>
            </Card>
          )}
          <Card className="p-6">
            <div className="flex items-start gap-3">
              <span className="k-tile k-tile-neutral grid size-10 shrink-0 place-items-center rounded-[12px] [&_svg]:size-[19px]">
                <Smartphone strokeWidth={1.8} />
              </span>
              <div>
                <h2 className="text-[15px] font-medium">{t("shell.download.iphoneTitle")}</h2>
                <p className="mt-1 text-[13px] text-fg-3">{t("shell.download.iphoneText")}</p>
              </div>
            </div>
          </Card>
        </div>
      </main>

      <footer className="mx-auto max-w-5xl px-4 pb-10 text-[11px] leading-relaxed text-fg-3 sm:px-6">{t("common.riskWarning")}</footer>
    </div>
  );
}
