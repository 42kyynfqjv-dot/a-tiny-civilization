import type { Metadata } from "next";
import Link from "next/link";
import { FoundationPulse } from "../../../components/FoundationPulse";
import { LifeProfile } from "../../../components/LifeProfile";

export const metadata: Metadata = {
  title: "Life Record",
  description: "Follow one individual life through the committed public record.",
};

export default async function LifePage({ params }: { params: Promise<{ worldId: string; organismId: string }> }) {
  const { worldId, organismId } = await params;
  return (
    <main className="life-page">
      <header className="life-topbar">
        <Link className="living-brand" href="/" aria-label="Return to A Tiny Civilization">
          <span className="living-brand-world" aria-hidden="true" />
          <span><strong>A Tiny Civilization</strong><small>Life record</small></span>
        </Link>
        <nav><Link href="/">World</Link><Link href="/lives">All lives</Link><Link href="/wiki">Wiki</Link></nav>
        <FoundationPulse compact />
      </header>
      <LifeProfile worldId={worldId} organismId={organismId} />
    </main>
  );
}
