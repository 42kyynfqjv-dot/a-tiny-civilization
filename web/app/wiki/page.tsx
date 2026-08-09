/* eslint-disable @next/next/no-html-link-for-pages -- observatory navigation must work without client routing */
import type { Metadata } from "next";
import { FoundationPulse } from "../components/FoundationPulse";
import { WikiIndex } from "../components/WikiIndex";

export const metadata: Metadata = {
  title: "Observer Wiki",
  description: "A provenance-preserving public research index over committed civilization history.",
};

const provenance = [
  ["World fact", "A committed event or a cited scientific input. It is not what any inhabitant knows."],
  ["Observed evidence", "A physical trace or record visible to observers, with its source event retained."],
  ["Contemporary claim", "A claim made inside the world, retained with speaker, context, and uncertainty when the cognition system supports it."],
  ["Later interpretation", "A dated interpretation that remains distinct from its evidence and competing readings."],
] as const;

export default function ObserverWikiPage() {
  return (
    <main className="wiki-page">
      <header className="wiki-topbar">
        <a className="brand" href="/" aria-label="Return to A Tiny Civilization Observatory">
          <span className="brand-mark" aria-hidden="true"><span /><span /><span /></span>
          <span><strong>A Tiny</strong><small>Civilization Observatory</small></span>
        </a>
        <nav aria-label="Observer wiki navigation"><a href="/">Live world</a><a aria-current="page" href="/wiki">Observer wiki</a></nav>
        <FoundationPulse compact />
      </header>

      <section className="wiki-hero">
        <p className="eyebrow accent">Public research index</p>
        <h1>Evidence first. Interpretation stays visible.</h1>
        <p>
          This is a read-only finding aid over committed world history. It never teaches the
          civilization, edits its past, or turns an observer label into an in-world fact.
        </p>
      </section>

      <section className="wiki-rules" aria-labelledby="wiki-rules-title">
        <div><p className="eyebrow">Provenance model</p><h2 id="wiki-rules-title">One record can support many readings. It does not become one story.</h2></div>
        <ol>{provenance.map(([title, description], index) => <li key={title}><span>{String(index + 1).padStart(2, "0")}</span><div><h3>{title}</h3><p>{description}</p></div></li>)}</ol>
      </section>

      <WikiIndex />

      <section className="wiki-future">
        <p className="eyebrow">Research papers and artifacts</p>
        <h2>When language, durable artifacts, or writing genuinely emerge, their pages will cite the record—not grant their meaning.</h2>
        <p>Any future dictionary preserves original signal forms beside tentative translations, confidence, evidence, and competing readings. Until then, this index remains deliberately small.</p>
      </section>
    </main>
  );
}
