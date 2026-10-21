"use client";

import * as React from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { ArrowLeft } from "lucide-react";
import { Card, EmptyState, PageHeader, Reveal, buttonVariants } from "@kalks/ui";
import { ClientDetailView, ClientHeader } from "./client-detail";
import { ClientControlsProvider, ClientControlsSection, ClientStaffActions } from "@/components/clients/client-controls";
import { EraseClientButton } from "@/components/clients/erase-client";
import { useApi } from "./kit";
import { ClientScoreBlock } from "@/components/trading-live/risk-kit";
import type { ClientDetail } from "./types";

export function LiveClientPage() {
  const params = useParams<{ id: string }>();
  const id = /^\d{1,18}$/.test(params.id ?? "") ? Number(params.id) : null;
  const { data, reload } = useApi<ClientDetail>(id ? `/api/admin/users/${id}` : null);
  // bumped after an erasure so the detail view below reads the anonymised record again
  const [gen, setGen] = React.useState(0);
  const back = (
    <Link href="/clients" className={buttonVariants({ variant: "surface" })}>
      <ArrowLeft /> All clients
    </Link>
  );
  if (!id)
    return (
      <Card className="mt-6">
        <EmptyState title="Client not found" text="Check the client ID and try again." illustration="magnifying_glass_tilted_left" action={back} />
      </Card>
    );
  return (
    <ClientControlsProvider id={id} name={data?.user.name ?? `Client #${id}`}>
      <div className="pb-10">
        <PageHeader title="Client profile" subtitle={data ? `Client #${id} · ${data.user.email}` : `Client #${id}`} actions={
            <>
              <ClientStaffActions />
              {data && <EraseClientButton user={data.user} onErased={() => (reload(), setGen((g) => g + 1))} />}
              {back}
            </>
          }
        />
        {data && (
          <Reveal>
            <Card className="mb-4 px-6 py-5">
              <ClientHeader d={data} />
            </Card>
          </Reveal>
        )}
        {/* presence, devices and restrictions (client-controls.tsx) */}
        <ClientControlsSection />
        {/* engine client score (risk desk), for staff with dealing.read */}
        <Reveal delay={0.03}>
          <ClientScoreBlock userId={String(id)} className="mb-4" />
        </Reveal>
        <Reveal delay={0.05}>
          <ClientDetailView key={gen} id={id} />
        </Reveal>
      </div>
    </ClientControlsProvider>
  );
}
