import type { Metadata } from "next";
import { ArchiveIndex } from "./components/ArchiveIndex";
import { FoundationPulse } from "./components/FoundationPulse";
import { LiveRecord } from "./components/LiveRecord";
import { NewsletterPanel } from "./components/NewsletterPanel";
import { SupporterPanel } from "./components/SupporterPanel";

export const metadata: Metadata = {
  title: "Live World",
  description:
    "Look in on a tiny unscripted world, follow individual lives, and come back to see what changed.",
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
          <a href="/memory">Memories</a>
          <a href="#discoveries">Discoveries</a>
        </nav>
        <FoundationPulse compact />
      </header>

      <main id="live">
        <LiveRecord />

        <section className="living-depth" aria-labelledby="depth-title">
          <div className="living-section-kicker"><span>Look closer</span><span>A whole world beneath every life</span></div>
          <div className="living-depth-copy">
            <div>
              <p className="eyebrow">Their world</p>
              <h2 id="depth-title">No quests. No script. No idea what comes next.</h2>
            </div>
            <p>
              They wake hungry, cold, curious, and surrounded by the same kinds of earth, water,
              weather, plants, and animals we know. Everything beyond instinct is theirs to stumble into.
            </p>
          </div>
          <div className="living-depth-grid">
            <article><span>01</span><h3>A familiar planet</h3><p>Real landscapes, weather, creatures, and materials make up their home.</p></article>
            <article><span>02</span><h3>Nothing to unlock</h3><p>There is no technology tree. If a habit, tool, or tradition appears, they made it happen.</p></article>
            <article><span>03</span><h3>No second takes</h3><p>What happens becomes their past. We do not rewind an awkward day or rescue a bad decision.</p></article>
            <article><span>04</span><h3>Behind the glass</h3><p>We can watch, follow, and wonder. Nothing we click can reach them.</p></article>
          </div>
        </section>

        <section className="living-notebook" id="wiki" aria-labelledby="notebook-title">
          <div className="living-notebook-intro">
            <p className="eyebrow">Their unfolding story</p>
            <h2 id="notebook-title">When something new happens, it gets a page.</h2>
            <p>
              Follow lives, revisit places, and see the first time something truly new appears. If
              they ever make writing, art, tools, or research, it will grow here with them.
            </p>
            <a className="living-text-link" href="/wiki">Explore everything discovered <span aria-hidden="true">↗</span></a>
          </div>
          <div className="living-notebook-stack" aria-label="Wiki sections">
            <article><span>People &amp; animals</span><strong>Lives you can return to</strong><small>See where they have been, what they remember, and who keeps crossing their path.</small></article>
            <article><span>Discoveries</span><strong>First times that matter</strong><small>A new behavior, a lasting habit, or something this world has never seen before.</small></article>
            <article><span>Things they make</span><strong>Objects with a story</strong><small>If an ordinary object starts changing or being kept, you can follow its history.</small></article>
          </div>
        </section>

        <section className="living-archive" id="archive" aria-labelledby="archive-title">
          <div className="living-section-heading">
            <div><p className="eyebrow">Past worlds</p><h2 id="archive-title">Nothing that lived here disappears.</h2></div>
            <p>If everyone dies, their world becomes a story you can still explore. A new one begins from nothing.</p>
          </div>
          <ArchiveIndex />
        </section>

        <NewsletterPanel />

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
          <a href="/memory">Memories</a><a href="/wiki">Research &amp; wiki</a><a href="/privacy">Privacy</a><a href="/terms">Terms</a>
          <a href="/supporter-policy">Supporter policy</a><a href="/presentation-policy">Presentation</a>
        </nav>
        <p>Open source · Apache 2.0</p>
      </footer>
    </div>
  );
}
