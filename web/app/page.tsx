import type { Metadata } from "next";
import { ArchiveIndex } from "./components/ArchiveIndex";
import { FoundationPulse } from "./components/FoundationPulse";
import { LiveRecord } from "./components/LiveRecord";
import { SupporterPanel } from "./components/SupporterPanel";

export const metadata: Metadata = {
  title: "Live World",
  description:
    "Follow the lives inside a persistent, unscripted civilization—and inspect the evidence behind every public claim.",
};

export default function Home() {
  return (
    <div className="living-site">
      <header className="living-nav">
        <a className="living-brand" href="#live" aria-label="A Tiny Civilization home">
          <span className="living-brand-world" aria-hidden="true" />
          <span><strong>A Tiny Civilization</strong><small>Live observatory</small></span>
        </a>
        <nav aria-label="Observatory navigation">
          <a href="#live">World</a>
          <a href="#people">Lives</a>
          <a href="/memory">Memory</a>
          <a href="#discoveries">Discoveries</a>
          <a href="/wiki">Wiki</a>
        </nav>
        <FoundationPulse compact />
      </header>

      <main id="live">
        <LiveRecord />

        <section className="living-depth" aria-labelledby="depth-title">
          <div className="living-section-kicker"><span>Look closer</span><span>Every claim has a trail</span></div>
          <div className="living-depth-copy">
            <div>
              <p className="eyebrow">A living Earth</p>
              <h2 id="depth-title">The whole world beneath every life.</h2>
            </div>
            <p>
              Weather, terrain, water, real species, and real materials shape what is possible.
              Nobody inside receives a technology tree or a story to follow. What happens next is
              history—not content written for an audience.
            </p>
          </div>
          <div className="living-depth-grid">
            <article><span>01</span><h3>Actual Earth</h3><p>One shared planet, built from traceable public geographic and environmental sources.</p></article>
            <article><span>02</span><h3>Unscripted lives</h3><p>Needs are innate. Culture, tools, traditions, and explanations are not.</p></article>
            <article><span>03</span><h3>Permanent history</h3><p>Events are append-only and the world can be replayed from its recorded inputs.</p></article>
            <article><span>04</span><h3>An honest window</h3><p>We observe and explain the record. The observer system cannot steer the world.</p></article>
          </div>
        </section>

        <section className="living-notebook" id="wiki" aria-labelledby="notebook-title">
          <div className="living-notebook-intro">
            <p className="eyebrow">The public wiki</p>
            <h2 id="notebook-title">A notebook that grows only when the world gives it something to say.</h2>
            <p>
              Lives, places, evidence, and discoveries are connected to their source events. If
              writing, art, tools, or research emerge, their artifacts receive a dedicated archive.
            </p>
            <a className="living-text-link" href="/wiki">Open the world notebook <span aria-hidden="true">↗</span></a>
          </div>
          <div className="living-notebook-stack" aria-label="Wiki sections">
            <article><span>People &amp; animals</span><strong>Lives with a past</strong><small>Birth, lineage, movement, memory, and the traces each life leaves.</small></article>
            <article><span>Discoveries</span><strong>Firsts without hindsight</strong><small>Observed patterns are recorded without pretending the inhabitants understand them.</small></article>
            <article><span>Artifacts</span><strong>Objects with provenance</strong><small>Material changes appear before an observer assigns them a possible meaning.</small></article>
          </div>
        </section>

        <section className="living-archive" id="archive" aria-labelledby="archive-title">
          <div className="living-section-heading">
            <div><p className="eyebrow">World archive</p><h2 id="archive-title">Nothing lived here is discarded.</h2></div>
            <p>If every person dies, that world closes as an immutable history. A successor begins without intervention or inherited knowledge.</p>
          </div>
          <ArchiveIndex />
        </section>

        <section className="living-support" id="supporters" aria-labelledby="supporter-title">
          <div className="living-support-copy">
            <p className="eyebrow">Stand beside the world</p>
            <h2 id="supporter-title">Give a future life a name.</h2>
            <p>
              Choose a person or individually recorded animal and a birth category. When a matching
              life arrives naturally, an approved name becomes part of its public story.
            </p>
            <ul>
              <li>Naming never creates, schedules, delays, or changes a birth.</li>
              <li>The civilization cannot perceive reservations or supporters.</li>
              <li>Names are screened before they appear in the public record.</li>
            </ul>
          </div>
          <SupporterPanel />
        </section>
      </main>

      <footer className="living-footer">
        <div><strong>A Tiny Civilization</strong><span>We can look in. We cannot reach in.</span></div>
        <nav aria-label="Project policies">
          <a href="/memory">Memory</a><a href="/privacy">Privacy</a><a href="/terms">Terms</a>
          <a href="/supporter-policy">Supporter policy</a><a href="/presentation-policy">Presentation</a>
        </nav>
        <p>Open source · Apache 2.0</p>
      </footer>
    </div>
  );
}
