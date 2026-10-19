"use client";

import Link from "next/link";
import { Button, Card, EmptyState } from "@/components/kit";
import { IS_DEMO } from "@kalks/mock/mode";
import { LiveLeaderboardPage } from "@/components/growth/leaderboard";

function DemoLeaderboard() {
  return (
    <Card>
      <EmptyState
        illustration="trophy"
        title="Leaderboard"
        text="Weekly, monthly and all-time XP rankings and every contest board open here in the live Client Area. Traders appear by nickname only."
        action={
          <Link href="/rewards">
            <Button variant="surface">Back to rewards</Button>
          </Link>
        }
      />
    </Card>
  );
}

export default function Page() {
  return IS_DEMO ? <DemoLeaderboard /> : <LiveLeaderboardPage />;
}
