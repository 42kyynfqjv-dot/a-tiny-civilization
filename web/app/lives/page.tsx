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
        <nav><Link href="/">World</Link><Link aria-current="page" href="/lives">Lives</Link><Link href="/wiki">Wiki</Link></nav>
        <FoundationPulse compact />
      </header>
      <section className="life-directory-hero">
        <p className="eyebrow">Lives inside the world</p>
        <h1>Choose someone to return to.</h1>
        <p>Every person and individually represented animal has a numerical observer ID until the inhabitants develop their own naming. Following changes only your observatory; it cannot be perceived inside the world.</p>
      </section>
      <LifeDirectory />
    </main>
  );
}
