"use client";

import { useEffect, useState } from "react";
import { WorldInputStatus, type WorldInputMetadata } from "./WorldInputStatus";

type World = WorldInputMetadata & {
  world_id: string;
  status: "initializing" | "running" | "extinct" | "archived";
};

type Finding = {
  finding_key: string;
  title: string;
  summary: string;
  kind: "first" | "record" | "streak";
  source_sequence: string | number;
  source_tick: string | number;
};

type Organism = {
  organism_id: string;
  role: "person" | "fauna";
  species: { scientific_name: string; source_url: string };
  introduced_sequence: string | number;
  introduced_tick: string | number;
  ended_event_id: string | null;
};

type WikiState =
  | { state: "loading" }
  | { state: "empty" }
  | { state: "error" }
  | { state: "ready"; world: World; findings: Finding[]; organisms: Organism[] };

/**
 * A read-only index over public observer projections. It intentionally cannot create
 * wiki claims, name an inhabitant, or query canonical/private world state.
 */
export function WikiIndex() {
  const [wiki, setWiki] = useState<WikiState>({ state: "loading" });

  useEffect(() => {
    let active = true;

    async function refresh() {
      try {
        const worldsResponse = await fetch("/api/v1/worlds", { cache: "no-store" });
        if (!worldsResponse.ok) throw new Error("world list unavailable");
        const { worlds } = (await worldsResponse.json()) as { worlds: World[] };
        const world = worlds.find((item) => item.status === "running") ?? worlds[0];
        if (!world) {
          if (active) setWiki({ state: "empty" });
          return;
        }

        const worldId = encodeURIComponent(world.world_id);
        const [findingsResponse, organismsResponse] = await Promise.all([
          fetch(`/api/v1/worlds/${worldId}/findings?limit=24`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${worldId}/organisms?limit=24`, { cache: "no-store" }),
        ]);
        if (!findingsResponse.ok || !organismsResponse.ok) throw new Error("wiki records unavailable");
        const findings = (await findingsResponse.json()) as { findings: Finding[] };
        const organisms = (await organismsResponse.json()) as { organisms: Organism[] };
        if (active) setWiki({ state: "ready", world, findings: findings.findings, organisms: organisms.organisms });
      } catch {
        if (active) setWiki({ state: "error" });
      }
    }

    void refresh();
    const timer = window.setInterval(refresh, 15_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  if (wiki.state === "loading") return <p className="wiki-index-status">Reading committed observer records…</p>;
  if (wiki.state === "empty") return <p className="wiki-index-status">No world has been committed. This index will begin at its first public event.</p>;
  if (wiki.state === "error") return <p className="wiki-index-status">The live index is temporarily unavailable. Its evidence rules remain public.</p>;

  return (
    <section className="wiki-live-index" aria-labelledby="wiki-live-index-title">
      <div className="wiki-live-heading">
        <div>
          <p className="eyebrow">Committed index</p>
          <h2 id="wiki-live-index-title">World {wiki.world.world_id.slice(0, 8)}</h2>
        </div>
        <div className="world-status-stack">
          <span className="world-lifecycle-status">{wiki.world.status}</span>
          <WorldInputStatus world={wiki.world} />
        </div>
      </div>
      <div className="wiki-live-grid">
        <article>
          <h3>Finding aids</h3>
          {wiki.findings.length === 0 ? <p>No firsts or records have been established.</p> : <ol>{wiki.findings.map((finding) => <li key={finding.finding_key}><strong>{finding.title}</strong><span>{finding.summary}</span><small>Event {finding.source_sequence} · tick {finding.source_tick}</small></li>)}</ol>}
        </article>
        <article>
          <h3>Lives cited in the record</h3>
          {wiki.organisms.length === 0 ? <p>No individual public records are available yet.</p> : <ul>{wiki.organisms.map((organism) => <li key={organism.organism_id}><span>{organism.role === "person" ? "Person" : "Animal"}</span><a href={organism.species.source_url} rel="noreferrer" target="_blank">{organism.species.scientific_name}</a><small>Introduced at event {organism.introduced_sequence} · {organism.ended_event_id ? "record ended" : "present in record"}</small></li>)}</ul>}
        </article>
      </div>
    </section>
  );
}
