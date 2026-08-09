import type { Metadata } from "next";
import Link from "next/link";
import { FoundationPulse } from "../components/FoundationPulse";
import { MemoryIndex } from "../components/MemoryIndex";

export const metadata: Metadata = {
  title: "Living Memory",
  description: "Watch small experiences stay with the lives inside A Tiny Civilization.",
};

export default function MemoryPage() {
  return <main className="memory-page">
    <header className="life-topbar memory-topbar">
      <Link className="living-brand" href="/" aria-label="Return to A Tiny Civilization">
        <span className="living-brand-world" aria-hidden="true" />
        <span><strong>A Tiny Civilization</strong><small>Deep-space observatory</small></span>
      </Link>
      <nav><Link href="/">World</Link><Link href="/lives">Lives</Link><Link aria-current="page" href="/memory">Memories</Link></nav>
      <FoundationPulse compact />
    </header>
    <section className="memory-hero">
      <p className="eyebrow">Inside their memories · live</p>
      <h1>What stays with them?</h1>
      <p>A sound. A scent. A patch of ground. Another life passing close by. Watch small experiences remain—and sometimes return later.</p>
    </section>
    <MemoryIndex />
  </main>;
}
