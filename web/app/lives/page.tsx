import type { Metadata } from "next";
import Link from "next/link";
import { FoundationPulse } from "../components/FoundationPulse";
import { LifeDirectory } from "../components/LifeDirectory";

export const metadata: Metadata = {
  title: "Lives",
  description: "Choose and follow an individual life in the public civilization record.",
};

export default function LivesPage() {
  return (
    <main className="life-page">
      <header className="life-topbar">
        <Link className="living-brand" href="/" aria-label="Return to A Tiny Civilization">
          <span className="living-brand-world" aria-hidden="true" />
          <span><strong>A Tiny Civilization</strong><small>Live observatory</small></span>
        </Link>
        <nav><Link href="/">World</Link><Link aria-current="page" href="/lives">Lives</Link><Link href="/memory">Memories</Link></nav>
        <FoundationPulse compact />
      </header>
      <section className="life-directory-hero">
        <p className="eyebrow">Lives inside the world</p>
        <h1>Choose someone to return to.</h1>
        <p>Pick a person or animal. Follow their wandering, their memories, and the lives they keep meeting. They will never know you are there.</p>
      </section>
      <LifeDirectory />
    </main>
  );
}
