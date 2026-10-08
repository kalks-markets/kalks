import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { Kit } from "./kit";

export const metadata: Metadata = { title: "Kalks 2 kit", robots: { index: false, follow: false } };

/**
 * The Kalks 2 design system on one page (every @kalks/ui component, light and dark via the theme switch or
 * ?theme=light|dark), for screenshots and review. Development builds only: production answers 404.
 */
export default async function KitPage({ searchParams }: { searchParams: Promise<{ theme?: string }> }) {
  if (process.env.NODE_ENV !== "development") notFound();
  const { theme } = await searchParams;
  return <Kit theme={theme === "light" || theme === "dark" ? theme : undefined} />;
}
