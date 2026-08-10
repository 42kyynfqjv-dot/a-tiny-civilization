import type { Metadata } from "next";
import { headers } from "next/headers";
import { CancerWorldConsole } from "../components/CancerWorldConsole";

export const metadata: Metadata = {
  title: "Research Console",
  robots: { index: false, follow: false, noarchive: true, nosnippet: true },
};

export default async function CancerConsolePage() {
  const requestHeaders = await headers();
  const worldId = requestHeaders.get("x-atc-cancer-world-id") ?? "";
  return <CancerWorldConsole worldId={worldId} />;
}
