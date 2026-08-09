/* eslint-disable @next/next/no-html-link-for-pages -- policy navigation must work without client routing */
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
        <a className="brand" href="/" aria-label="Return to A Tiny Civilization Observatory">
          <span className="brand-mark" aria-hidden="true"><span /><span /><span /></span>
          <span><strong>A Tiny</strong><small>Civilization Observatory</small></span>
        </a>
        <nav aria-label="Policy navigation">
          <a href="/">Live world</a>
          <a href="/privacy">Privacy</a>
          <a href="/terms">Terms</a>
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
          <a href="/privacy">Privacy notice</a>
          <a href="/terms">Terms of use</a>
          <a href="/supporter-policy">Supporter naming</a>
          <a href="/presentation-policy">World presentation</a>
        </nav>
      </article>
    </main>
  );
}
