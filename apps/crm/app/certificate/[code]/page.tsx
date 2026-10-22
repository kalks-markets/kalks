import type { Metadata } from "next";
import Link from "next/link";
import { BadgeCheck, ShieldX } from "lucide-react";
import { Logo } from "@kalks/ui/logo";
import { academy } from "@/lib/academy";

// Public certificate verification (linked from the certificate image). No sign-in: anyone with the code can
// confirm who earned which Academy certificate and when.

export const dynamic = "force-dynamic";
export const metadata: Metadata = { title: "Certificate verification", robots: { index: false } };

type Verify = { code: string; tenant_name: string; learner_name: string; phase_order: number; phase_title: string; level: string; score_pct: number; issued_at: string; revoked: boolean; valid: boolean };

export default async function CertificatePage({ params }: { params: Promise<{ code: string }> }) {
  const { code } = await params;
  const ok = /^KA-[A-Z0-9]{5}-[A-Z0-9]{5}$/.test(code);
  const r = ok ? await academy(`/v1/public/certificates/${code}`) : { status: 404, data: null };
  const c = r.status === 200 ? (r.data as Verify) : null;
  const unavailable = r.status >= 500;

  return (
    <main className="min-h-dvh bg-bg px-4 py-10 text-fg sm:py-16">
      <div className="mx-auto max-w-[880px]">
        <div className="mb-8 flex items-center justify-between">
          <Logo height={22} />
          <span className="text-[12.5px] text-fg-3">Academy certificate verification</span>
        </div>
        {c ? (
          <>
            <div className={`k-card flex items-start gap-4 p-6 ${c.valid ? "" : "border-down/40"}`} data-testid="verify-result">
              <span className={`grid size-12 shrink-0 place-items-center rounded-full border ${c.valid ? "border-up/30 bg-up-soft text-up" : "border-down/30 bg-down-soft text-down"}`}>
                {c.valid ? <BadgeCheck className="size-6" /> : <ShieldX className="size-6" />}
              </span>
              <div className="min-w-0">
                <div className={`text-[13px] font-medium ${c.valid ? "text-up" : "text-down"}`}>{c.valid ? "Valid certificate" : "This certificate has been revoked"}</div>
                <h1 className="mt-1 text-[24px] font-medium leading-tight tracking-tight">{c.learner_name}</h1>
                <p className="mt-1 text-[14px] text-fg-2">
                  Completed Phase {c.phase_order} · {c.phase_title} ({c.level}) of the {c.tenant_name} Academy with a final exam score of {c.score_pct}%.
                </p>
                <p className="k-num mt-2 text-[12.5px] text-fg-3">
                  {c.code} · issued {new Date(c.issued_at).toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric" })}
                </p>
              </div>
            </div>
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img src={`/certificate/${c.code}/image`} alt={`Certificate ${c.code}`} className="mt-6 block w-full rounded-[16px] border border-line" />
          </>
        ) : (
          <div className="k-card p-8 text-center" data-testid="verify-result">
            <ShieldX className="mx-auto size-8 text-fg-3" />
            <h1 className="mt-3 text-[20px] font-medium">{unavailable ? "Verification is unavailable" : "Certificate not found"}</h1>
            <p className="mx-auto mt-1 max-w-md text-[13.5px] text-fg-3">
              {unavailable ? "Please try again in a moment." : "Check the certificate ID. IDs look like KA-XXXXX-XXXXX and are printed at the bottom of the certificate."}
            </p>
          </div>
        )}
        <p className="mt-8 text-center text-[11.5px] text-fg-3">
          Educational certificate. It is not a financial qualification or licence. <Link href="/login" className="underline underline-offset-2">Client Area</Link>
        </p>
      </div>
    </main>
  );
}
