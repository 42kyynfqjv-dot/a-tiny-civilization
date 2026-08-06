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
  ["Live world", "#live", "LV"],
  ["Timeline", "#timeline", "TL"],
  ["People", "#people", "PE"],
  ["Animals", "#animals", "AN"],
  ["Cultures", "#cultures", "CU"],
  ["Artifacts", "#artifacts", "AR"],
] as const;

const archiveLinks = [
  ["Observer wiki", "/wiki"],
  ["Extinct worlds", "#archive"],
  ["Evidence ledger", "#evidence"],
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

        <label className="search-shell">
          <span className="sr-only">Search the observer wiki</span>
          <span className="search-symbol" aria-hidden="true" />
          <input type="search" placeholder="Search lives, places, evidence…" disabled />
          <kbd>Soon</kbd>
        </label>

        <FoundationPulse compact />
      </header>

      <aside className="sidebar">
        <nav aria-label="Observatory navigation">
          <p className="nav-label">Observe</p>
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

          <p className="nav-label nav-label-secondary">Research</p>
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
          <p className="eyebrow">Support the experiment</p>
          <h2 id="support-title">Follow a life from its first breath.</h2>
          <p>
            Reserve an observer name for a future person or animal. Labels never alter the
            simulation.
          </p>
          <a className="button button-light" href="#supporters">
            Supporter preview
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
            <p className="eyebrow accent">World 001 · Genesis chamber</p>
            <h1>History, before anyone knows it is history.</h1>
          </div>
          <p className="hero-intro">
            A persistent Earth simulation with no technology tree, narrator, or promised future.
            Every idea must survive contact with matter, memory, and other lives.
          </p>
        </section>

        <section className="world-panel" aria-labelledby="world-panel-title">
          <div className="panel-toolbar">
            <div>
              <p className="eyebrow">Live terrain</p>
              <h2 id="world-panel-title">River basin · seed awaiting launch</h2>
            </div>
            <div className="map-controls" aria-label="Map display status">
              <span>Scientific truth</span>
              <span>Agent perception</span>
            </div>
          </div>

          <div className="world-map" role="img" aria-label="Abstract river basin initialization map">
            <div className="terrain terrain-one" />
            <div className="terrain terrain-two" />
            <div className="river river-main" />
            <div className="river river-branch" />
            <div className="map-grid" />
            <span className="map-point point-one" />
            <span className="map-point point-two" />
            <span className="map-point animal point-three" />
            <span className="map-point animal point-four" />
            <div className="map-coordinate coordinate-north">N 52°</div>
            <div className="map-coordinate coordinate-scale">12 km</div>
            <div className="genesis-marker">
              <span className="marker-pulse" />
              <div>
                <strong>Initial conditions</strong>
                <small>World creation has not been committed</small>
              </div>
            </div>
          </div>

          <div className="world-stats">
            <article>
              <span>Population</span>
              <strong>—</strong>
              <small>awaiting first world</small>
            </article>
            <article>
              <span>Simulated time</span>
              <strong>00:00</strong>
              <small>tick zero</small>
            </article>
            <article>
              <span>Known concepts</span>
              <strong>0</strong>
              <small>nothing granted</small>
            </article>
            <article>
              <span>Interventions</span>
              <strong>0</strong>
              <small>observer influence</small>
            </article>
          </div>
        </section>

        <section className="lower-grid">
          <article className="timeline-card" id="timeline">
            <div className="section-heading">
              <div>
                <p className="eyebrow">Event stream</p>
                <h2>The first page is still blank.</h2>
              </div>
              <span className="live-tag">Awaiting genesis</span>
            </div>
            <div className="empty-timeline">
              <span className="timeline-rule" />
              <span className="timeline-node" />
              <div>
                <strong>Tick 0</strong>
                <p>
                  Once the world begins, objective events appear here without being promoted to
                  knowledge inside the civilization.
                </p>
              </div>
            </div>
          </article>

          <article className="principle-card" id="evidence">
            <p className="eyebrow">Integrity rule 01</p>
            <blockquote>“We create the laws and initial conditions. We do not create the destination.”</blockquote>
            <Link href="/wiki">Read the evidence model <span aria-hidden="true">→</span></Link>
          </article>
        </section>

        <LiveRecord />

        <section className="archive-section" id="archive" aria-labelledby="archive-title">
          <div className="section-heading archive-heading">
            <div>
              <p className="eyebrow">World archives</p>
              <h2 id="archive-title">A reset never erases a history.</h2>
            </div>
            <p>
              Extinction is mechanical. An archived world cannot be revived or rewritten; any
              successor starts with a separate, explicit seed.
            </p>
          </div>
          <ArchiveIndex />
        </section>

        <section className="wiki-section" id="wiki">
          <div className="section-heading wiki-heading">
            <div>
              <p className="eyebrow">Observer wiki</p>
              <h2>Every claim carries its evidence.</h2>
            </div>
            <p>
              World fact, remembered experience, cultural teaching, and observer inference never
              collapse into one story.
            </p>
          </div>
          <div className="wiki-grid">
            <article>
              <span className="wiki-index">01</span>
              <p className="provenance provenance-fact">World fact</p>
              <h3>Actual materials</h3>
              <p>Measured properties, scientific sources, uncertainty, and ruleset transformations.</p>
            </article>
            <article>
              <span className="wiki-index">02</span>
              <p className="provenance provenance-memory">Situated memory</p>
              <h3>What a life could know</h3>
              <p>Perception, testimony, forgotten details, confidence, distortion, and contradiction.</p>
            </article>
            <article>
              <span className="wiki-index">03</span>
              <p className="provenance provenance-inference">Observer inference</p>
              <h3>Artifacts, if they emerge</h3>
              <p>Physical marks and objects gain special pages without teaching agents what they mean.</p>
            </article>
          </div>
        </section>

        <section className="supporter-strip" id="supporters">
          <div>
            <p className="eyebrow">A front-row seat, never a steering wheel</p>
            <h2>Name the next naturally born life.</h2>
          </div>
          <p>
            Choose a person or species and birth sex, then wait for the simulation to produce a
            matching birth naturally. Supporters receive a permanent observer profile—not control.
          </p>
          <button className="button button-dark" type="button" disabled>
            Opens after first births
          </button>
        </section>

        <footer>
          <p>A Tiny Civilization · Public foundation build</p>
          <p>No LLM key connected · deterministic mode</p>
        </footer>
      </main>
    </div>
  );
}
