import type { Metadata } from "next";
import Link from "next/link";
import { FoundationPulse } from "../components/FoundationPulse";
import { MemoryIndex } from "../components/MemoryIndex";

export const metadata: Metadata = {
  title: "Living Memory",
  description: "Watch direct experiences become retained and recalled context inside the live world.",
};

export default function MemoryPage() {
  return <main className="memory-page">
    <header className="life-topbar memory-topbar">
      <Link className="living-brand" href="/" aria-label="Return to A Tiny Civilization">
        <span className="living-brand-world" aria-hidden="true" />
        <span><strong>A Tiny Civilization</strong><small>Deep-space observatory</small></span>
      </Link>
      <nav><Link href="/">World</Link><Link href="/lives">Lives</Link><Link aria-current="page" href="/memory">Memory</Link><Link href="/wiki">Wiki</Link></nav>
      <FoundationPulse compact />
    </header>
    <section className="memory-hero">
      <p className="eyebrow">Observer memory array · live</p>
      <h1>Watch experience leave a trace.</h1>
      <p>Every glow began as a direct sensation. Bright returning lines are memories admitted into a new cognition request—not thoughts we wrote, meanings we assigned, or proof that the memory caused an action.</p>
    </section>
    <MemoryIndex />
  </main>;
}
