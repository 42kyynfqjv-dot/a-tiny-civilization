import type { Metadata } from "next";
import Link from "next/link";
import { FoundationPulse } from "./components/FoundationPulse";
import { LiveRecord } from "./components/LiveRecord";
import { ArchiveIndex } from "./components/ArchiveIndex";

export const metadata: Metadata = {
  title: "Live World",
  description:
    "Observe a persistent civilization whose history is produced by real materials, ecology, memory, and chance—not a script.",
};

const observeLinks = [
  ["The world", "#live", "●"],
  ["What happened", "#timeline", "↟"],
  ["People", "#people", "○"],
  ["Animals", "#animals", "◇"],
  ["Discoveries", "#cultures", "✦"],
  ["Things they make", "#artifacts", "⌁"],
] as const;

const archiveLinks = [
  ["World notebook", "/wiki"],
  ["Past worlds", "#archive"],
  ["How this works", "#evidence"],
] as const;

export default function Home() {
  return (
    <div className="app-shell">
      <header className="topbar">
        <a className="brand" href="#live" aria-label="A Tiny Civilization home">
          <span className="brand-mark" aria-hidden="true">
            <span />
            <span />
            <span />
          </span>
          <span>
            <strong>A Tiny</strong>
            <small>Civilization Observatory</small>
          </span>
        </a>

        <nav className="top-links" aria-label="Quick links">
          <a href="#live">Watch</a>
          <Link href="/wiki">World notebook</Link>
          <a href="#supporters">Join in</a>
        </nav>

        <FoundationPulse compact />
      </header>

      <aside className="sidebar">
        <nav aria-label="Observatory navigation">
          <p className="nav-label">Look around</p>
          <ul className="nav-list">
            {observeLinks.map(([label, href, short], index) => (
              <li key={label}>
                <a className={index === 0 ? "active" : undefined} href={href}>
                  <span className="nav-glyph" aria-hidden="true">
                    {short}
                  </span>
                  {label}
                </a>
              </li>
            ))}
          </ul>

          <p className="nav-label nav-label-secondary">Go deeper</p>
          <ul className="nav-list nav-list-plain">
            {archiveLinks.map(([label, href]) => (
              <li key={label}>
                {href.startsWith("/") ? <Link href={href}>{label}</Link> : <a href={href}>{label}</a>}
              </li>
            ))}
          </ul>
        </nav>

        <section className="support-card" aria-labelledby="support-title">
          <div className="support-orbit" aria-hidden="true">
            <span />
          </div>
          <p className="eyebrow">Take part</p>
          <h2 id="support-title">Give a future life a name.</h2>
          <p>
            One day you will be able to name a person or animal born naturally into this world.
          </p>
          <a className="button button-light" href="#supporters">
            See how it works
            <span aria-hidden="true">↗</span>
          </a>
        </section>

        <p className="open-source-note">
          <span aria-hidden="true">⌁</span>
          Open source · Apache 2.0
        </p>
      </aside>

      <main className="main-content" id="live">
        <section className="hero-copy">
          <div>
            <p className="eyebrow accent">A living world · now unfolding</p>
            <h1>A little world with a life of its own.</h1>
          </div>
          <p className="hero-intro">
            People will be born. Animals will wander. Ideas may catch on—or disappear forever.
            Nobody, including us, knows what they will become.
          </p>
        </section>

        <details className="preview-note">
          <summary>This is a public preview. What does that mean?</summary>
          <p>
            A provisional integration world is live and recording its own history now. The Earth
            model remains provisional—not yet scientifically admitted—and this is not the final
            canonical genesis.
          </p>
        </details>

        <section className="world-panel" aria-labelledby="world-panel-title">
          <div className="panel-toolbar">
            <div>
              <p className="eyebrow">The whole world</p>
              <h2 id="world-panel-title">One world is moving through time</h2>
            </div>
            <div className="map-controls" aria-label="Map display status">
              <span>Land and water</span>
              <span>Weather and seasons</span>
            </div>
          </div>

          <div className="world-map" role="img" aria-label="Abstract global reference field for the live provisional world">
            <div className="terrain terrain-one" />
            <div className="terrain terrain-two" />
            <div className="river river-main" />
            <div className="river river-branch" />
            <div className="map-grid" />
            <span className="map-point point-one" />
            <span className="map-point point-two" />
            <span className="map-point animal point-three" />
            <span className="map-point animal point-four" />
            <div className="map-coordinate coordinate-north">One shared Earth</div>
            <div className="map-coordinate coordinate-scale">History is recording</div>
            <div className="genesis-marker">
              <span className="marker-pulse" />
              <div>
                <strong>The world is live</strong>
                <small>Follow its committed history below</small>
              </div>
            </div>
          </div>

          <div className="world-stats">
            <article>
              <span>People</span>
              <strong>Live</strong>
              <small>current inhabitants appear below</small>
            </article>
            <article>
              <span>World age</span>
              <strong>Now</strong>
              <small>the live record shows its current tick</small>
            </article>
            <article>
              <span>Animals</span>
              <strong>Active</strong>
              <small>source-backed lives are recorded below</small>
            </article>
            <article>
              <span>Discoveries</span>
              <strong>Open</strong>
              <small>nothing is scripted</small>
            </article>
          </div>
        </section>

        <section className="lower-grid">
          <article className="timeline-card" id="timeline">
            <div className="section-heading">
              <div>
                <p className="eyebrow">What happened</p>
                <h2>The newest committed moments.</h2>
              </div>
              <span className="live-tag">Live record below</span>
            </div>
            <div className="empty-timeline">
              <span className="timeline-rule" />
              <span className="timeline-node" />
              <div>
                <strong>Tick 0</strong>
                <p>
                  The live record below refreshes from committed history. Quiet moments are still
                  part of the experiment; nothing is staged for the audience.
                </p>
              </div>
            </div>
          </article>

          <article className="principle-card" id="evidence">
            <p className="eyebrow">The promise</p>
            <blockquote>“We set the world in motion. After that, it belongs to itself.”</blockquote>
            <Link href="/wiki">See how we keep that promise <span aria-hidden="true">→</span></Link>
          </article>
        </section>

        <LiveRecord />

        <section className="archive-section" id="archive" aria-labelledby="archive-title">
          <div className="section-heading archive-heading">
            <div>
              <p className="eyebrow">Past worlds</p>
              <h2 id="archive-title">Even an ending becomes a story.</h2>
            </div>
            <p>
              If every person dies, that world ends. We keep its whole story here, then let a new
              world begin from scratch.
            </p>
          </div>
          <ArchiveIndex />
        </section>

        <section className="wiki-section" id="wiki">
          <div className="section-heading wiki-heading">
            <div>
              <p className="eyebrow">The world notebook</p>
              <h2>The story so far, without making things up.</h2>
            </div>
            <p>
              Follow lives, places, discoveries, and the things people make. The deeper research is
              always there when you want it.
            </p>
          </div>
          <div className="wiki-grid">
            <article>
              <span className="wiki-index">01</span>
              <p className="provenance provenance-fact">The world</p>
              <h3>Places that change</h3>
              <p>Weather, water, paths, shelters, and the marks left behind over time.</p>
            </article>
            <article>
              <span className="wiki-index">02</span>
              <p className="provenance provenance-memory">The lives</p>
              <h3>Someone to follow</h3>
              <p>Every person and special animal can have a life story you return to.</p>
            </article>
            <article>
              <span className="wiki-index">03</span>
              <p className="provenance provenance-inference">The surprises</p>
              <h3>Things they discover</h3>
              <p>If writing, tools, art, or traditions appear, they earn a home in the notebook.</p>
            </article>
          </div>
        </section>

        <section className="supporter-strip" id="supporters">
          <div>
            <p className="eyebrow">A small way to be part of it</p>
            <h2>Give a future life a name.</h2>
          </div>
          <p>
            Pick a person or animal and a gender. When a matching life is born naturally, your name
            joins their story. You can follow them, but never control them.
          </p>
          <button className="button button-dark" type="button" disabled>
            Opens after first births
          </button>
        </section>

        <footer>
          <p>A Tiny Civilization · Open source</p>
          <p>A world to watch, not a game to win</p>
        </footer>
      </main>
    </div>
  );
}
