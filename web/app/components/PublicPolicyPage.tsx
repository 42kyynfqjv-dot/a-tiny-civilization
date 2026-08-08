import Link from "next/link";
import { FoundationPulse } from "./FoundationPulse";

export type PublicPolicy = {
  title: string;
  status: string;
  summary: string;
  sections: ReadonlyArray<{
    heading: string;
    paragraphs: readonly string[];
  }>;
};

export function PublicPolicyPage({ policy }: { policy: PublicPolicy }) {
  return (
    <main className="policy-page">
      <header className="wiki-topbar">
        <Link className="brand" href="/" aria-label="Return to A Tiny Civilization Observatory">
          <span className="brand-mark" aria-hidden="true"><span /><span /><span /></span>
          <span><strong>A Tiny</strong><small>Civilization Observatory</small></span>
        </Link>
        <nav aria-label="Policy navigation">
          <Link href="/">Live world</Link>
          <Link href="/privacy">Privacy</Link>
          <Link href="/terms">Terms</Link>
        </nav>
        <FoundationPulse compact />
      </header>

      <article className="policy-document">
        <header>
          <p className="eyebrow accent">Public project policy</p>
          <h1>{policy.title}</h1>
          <p className="policy-status">{policy.status}</p>
          <p className="policy-summary">{policy.summary}</p>
        </header>
        {policy.sections.map((section) => (
          <section key={section.heading}>
            <h2>{section.heading}</h2>
            {section.paragraphs.map((paragraph) => <p key={paragraph}>{paragraph}</p>)}
          </section>
        ))}
        <nav className="policy-index" aria-label="All public policies">
          <Link href="/privacy">Privacy notice</Link>
          <Link href="/terms">Terms of use</Link>
          <Link href="/supporter-policy">Supporter naming</Link>
          <Link href="/presentation-policy">World presentation</Link>
        </nav>
      </article>
    </main>
  );
}
