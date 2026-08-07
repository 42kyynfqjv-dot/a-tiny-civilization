"use client";

import { useEffect, useState } from "react";
import { WorldInputStatus, type WorldInputMetadata } from "./WorldInputStatus";

type World = WorldInputMetadata & {
  world_id: string;
  status: "initializing" | "running" | "extinct" | "archived";
  through_sequence: string | number;
  tick: string | number;
  manifest_hash: string;
  event_hash: string;
  state_hash: string;
};

type TimelineItem = {
  source_event_id: string;
  source_sequence: string | number;
  source_tick: string | number;
  title: string;
  summary: string;
};

type Organism = {
  organism_id: string;
  role: "person" | "fauna";
  species: { scientific_name: string; source_url: string };
  ended_event_id: string | null;
};

type Finding = { finding_key: string; title: string; summary: string; kind: "first" | "record" | "streak" };

type RecordState =
  | { state: "loading" }
  | { state: "empty" }
  | { state: "error" }
  | { state: "ready"; world: World; timeline: TimelineItem[]; organisms: Organism[]; findings: Finding[] };

export function LiveRecord() {
  const [record, setRecord] = useState<RecordState>({ state: "loading" });

  useEffect(() => {
    let active = true;
    async function refresh() {
      try {
        const worldsResponse = await fetch("/api/v1/worlds", { cache: "no-store" });
        if (!worldsResponse.ok) throw new Error("world list unavailable");
        const { worlds } = (await worldsResponse.json()) as { worlds: World[] };
        const world = worlds.find((item) => item.status === "running") ?? worlds[0];
        if (!world) {
          if (active) setRecord({ state: "empty" });
          return;
        }
        const encoded = encodeURIComponent(world.world_id);
        const [timelineResponse, organismsResponse, findingsResponse] = await Promise.all([
          fetch(`/api/v1/worlds/${encoded}/timeline?limit=5`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${encoded}/organisms?limit=8`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${encoded}/findings?limit=4`, { cache: "no-store" }),
        ]);
        if (!timelineResponse.ok || !organismsResponse.ok || !findingsResponse.ok) throw new Error("world data unavailable");
        const timeline = (await timelineResponse.json()) as { items: TimelineItem[] };
        const organisms = (await organismsResponse.json()) as { organisms: Organism[] };
        const findings = (await findingsResponse.json()) as { findings: Finding[] };
        if (active) setRecord({ state: "ready", world, timeline: timeline.items, organisms: organisms.organisms, findings: findings.findings });
      } catch {
        if (active) setRecord({ state: "error" });
      }
    }
    void refresh();
    const timer = window.setInterval(refresh, 15_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  if (record.state === "loading") return <section className="live-record loading">Looking for signs of life…</section>;
  if (record.state === "empty") return <section className="live-record empty">The world has not begun yet. When it does, its first moments will appear here.</section>;
  if (record.state === "error") return <section className="live-record empty">The live window is resting. Please check back in a moment.</section>;

  const { world, timeline, organisms, findings } = record;
  return (
    <section className="live-record" aria-labelledby="live-record-title">
      <div className="live-record-heading">
        <div>
          <p className="eyebrow">The world today</p>
          <h2 id="live-record-title">World {world.world_id.slice(0, 8)}</h2>
          <WorldInputStatus world={world} />
        </div>
        <p>{world.status} · moment {world.tick}</p>
      </div>
      <div className="live-record-grid">
        <article>
          <h3>Recent moments</h3>
          {timeline.length === 0 ? <p>It is quiet here for now.</p> : <ol>{timeline.map((item) => <li key={item.source_event_id}><strong>{item.title}</strong><span>{item.summary} <em>Moment {item.source_tick}</em></span></li>)}</ol>}
          {findings.length > 0 && <><h3 className="finding-heading">Things worth noticing</h3><ul>{findings.map((finding) => <li key={finding.finding_key}><strong>{finding.title}</strong><span>{finding.summary}</span></li>)}</ul></>}
        </article>
        <article>
          <h3>Who is here</h3>
          {organisms.length === 0 ? <p>No individual lives are here yet.</p> : <ul>{organisms.map((organism) => <li key={organism.organism_id}><span>{organism.role === "person" ? "Person" : "Animal"}</span><a href={organism.species.source_url} rel="noreferrer" target="_blank">{organism.species.scientific_name}</a><small>{organism.ended_event_id ? "their story has ended" : "here now"}</small></li>)}</ul>}
        </article>
      </div>
      <details className="verification-details">
        <summary>Verify this world</summary>
        <dl className="audit-hashes" aria-label="Public verification hashes">
          <div><dt>Manifest</dt><dd title={world.manifest_hash}>{shortHash(world.manifest_hash)}</dd></div>
          <div><dt>Event head</dt><dd title={world.event_hash}>{shortHash(world.event_hash)}</dd></div>
          <div><dt>State</dt><dd title={world.state_hash}>{shortHash(world.state_hash)}</dd></div>
          {world.composition_hash && <div><dt>Input composition</dt><dd title={world.composition_hash}>{shortHash(world.composition_hash)}</dd></div>}
        </dl>
      </details>
    </section>
  );
}

function shortHash(hash: string) {
  return `${hash.slice(0, 12)}…${hash.slice(-8)}`;
}
