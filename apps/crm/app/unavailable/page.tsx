import type { Metadata } from "next";
import Link from "next/link";
import { Logo } from "@kalks/ui/logo";
import { Illustration } from "@kalks/ui/illustration";
import { getT } from "@kalks/i18n/server";

export const metadata: Metadata = { title: "Not available" };

const MODULE = /^[a-z_]{2,32}$/;

/** A module the broker has switched off (the proxy rewrites its pages here, with `?m=<module>`). */
export default async function UnavailablePage({ searchParams }: { searchParams: Promise<{ m?: string | string[] }> }) {
  const t = await getT();
  const m = (await searchParams).m;
  const key = typeof m === "string" && MODULE.test(m) ? m : null;
  const name = key ? t.dyn(`shell.module.${key}`, "") : "";
  return (
    <main className="grid min-h-dvh place-items-center bg-bg px-4 text-fg">
      <div className="max-w-md text-center">
        <Logo height={26} className="mx-auto" />
        <Illustration name="marketClosed" width={256} maxHeight={192} priority className="mx-auto mt-10" />
        <h1 className="mt-8 text-[28px] font-medium tracking-[-0.02em]" data-testid="unavailable-title">
          {t("shell.system.unavailable.title")}
        </h1>
        {name && (
          <p className="mt-3 text-[15px] font-medium text-fg" data-testid="unavailable-module">
            {t("shell.system.unavailable.module", { module: name })}
          </p>
        )}
        <p className="mt-3 text-[15px] text-fg-2">{t("shell.system.unavailable.text")}</p>
        <Link href="/" className="mt-8 inline-block text-[13.5px] text-ember hover:underline">
          {t("shell.system.unavailable.back")}
        </Link>
      </div>
    </main>
  );
}
