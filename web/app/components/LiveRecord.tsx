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
  introduced_tick: string | number;
};

type Finding = { finding_key: string; title: string; summary: string; kind: "first" | "record" | "streak" };

type Artifact = {
  object_id: string;
  material: { canonical_name: string; source_url: string };
  first_trace_sequence: string | number;
  first_trace_tick: string | number;
  latest_trace_sequence: string | number;
  latest_trace_tick: string | number;
  surface_trace_units: number;
};

type RecordState =
  | { state: "loading" }
  | { state: "empty" }
  | { state: "error" }
  | { state: "ready"; world: World; timeline: TimelineItem[]; organisms: Organism[]; findings: Finding[]; artifacts: Artifact[] };

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
        const [timelineResponse, organismsResponse, findingsResponse, artifactsResponse] = await Promise.all([
          fetch(`/api/v1/worlds/${encoded}/timeline?limit=12`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${encoded}/organisms?limit=50`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${encoded}/findings?limit=12`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${encoded}/artifacts?limit=12`, { cache: "no-store" }),
        ]);
        if (!timelineResponse.ok || !organismsResponse.ok || !findingsResponse.ok || !artifactsResponse.ok) throw new Error("world data unavailable");
        const timeline = (await timelineResponse.json()) as { items: TimelineItem[] };
        const organisms = (await organismsResponse.json()) as { organisms: Organism[] };
        const findings = (await findingsResponse.json()) as { findings: Finding[] };
        const artifacts = (await artifactsResponse.json()) as { artifacts: Artifact[] };
        if (active) setRecord({ state: "ready", world, timeline: timeline.items, organisms: organisms.organisms, findings: findings.findings, artifacts: artifacts.artifacts });
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

  const { world, timeline, organisms, findings, artifacts } = record;
  const people = organisms.filter((organism) => organism.role === "person" && !organism.ended_event_id);
  const animals = organisms.filter((organism) => organism.role === "fauna" && !organism.ended_event_id);
  const latestMoment = timeline[0];
  const began = timeline.find((item) => item.source_tick === "0" || item.source_tick === 0);
  return (
    <section className="live-record" id="happening" aria-labelledby="live-record-title">
      <div className="live-record-heading">
        <div>
          <p className="eyebrow">What is happening now</p>
          <h2 id="live-record-title">The opening chapter is under way.</h2>
          <WorldInputStatus world={world} />
        </div>
        <p className="live-record-cursor"><span aria-hidden="true" /> live · moment {formatNumber(world.tick)}</p>
      </div>
      <p className="live-record-intro">
        This world began with {people.length === 1 ? "one person" : `${people.length} people`} in the
        public record. Its clock is still advancing; every displayed fact below comes from committed
        history, not a story written for visitors.
      </p>
      <div className="world-now" aria-label="Current world state">
        <article id="people">
          <span>People here</span>
          <strong>{formatNumber(people.length)}</strong>
          <small>{people.length === 0 ? "no public life records yet" : "present in the record"}</small>
        </article>
        <article id="animals">
          <span>Animals here</span>
          <strong>{formatNumber(animals.length)}</strong>
          <small>{animals.length === 0 ? "none seeded into this preview" : "individual lives recorded"}</small>
        </article>
        <article>
          <span>Committed moments</span>
          <strong>{formatNumber(world.through_sequence)}</strong>
          <small>the permanent history so far</small>
        </article>
        <article id="discoveries">
          <span>Discoveries</span>
          <strong>{formatNumber(findings.length)}</strong>
          <small>{findings.length === 0 ? "nothing established yet" : "firsts and records observed"}</small>
        </article>
        <article>
          <span>Altered objects</span>
          <strong>{formatNumber(artifacts.length)}</strong>
          <small>{artifacts.length === 0 ? "no durable traces observed" : "physical traces in the archive"}</small>
        </article>
      </div>
      <div className="live-record-grid">
        <article id="timeline">
          <h3>The record so far</h3>
          {timeline.length === 0 ? <p>The projector has not published a public event yet.</p> : <ol>{timeline.map((item) => <li key={item.source_event_id}><strong>{item.title}</strong><span>{item.summary} <em>moment {formatNumber(item.source_tick)}</em></span></li>)}</ol>}
          {latestMoment?.source_tick === "0" || latestMoment?.source_tick === 0 ? <p className="quiet-note">No later public milestone has been recorded yet. The live clock is advancing, but this preview does not pretend quiet time is drama.</p> : null}
        </article>
        <article>
          <h3>Lives to watch</h3>
          {organisms.length === 0 ? <p>No individual lives are public yet.</p> : <ul>{organisms.map((organism, index) => <li key={organism.organism_id}><strong>{organism.role === "person" ? `Person ${String(index + 1).padStart(2, "0")}` : `Animal ${String(index + 1).padStart(2, "0")}`}</strong><a href={organism.species.source_url} rel="noreferrer" target="_blank">{organism.species.scientific_name}</a><small>{organism.ended_event_id ? "record ended" : `present since moment ${formatNumber(organism.introduced_tick)}`}</small></li>)}</ul>}
        </article>
      </div>
      <section className="finding-board" aria-labelledby="finding-board-title">
        <div>
          <p className="eyebrow">The first things we can say</p>
          <h3 id="finding-board-title">Evidence, not narration.</h3>
        </div>
        {findings.length === 0 ? <p>No firsts or records have been established yet.</p> : <ul>{findings.map((finding) => <li key={finding.finding_key}><span>{finding.kind}</span><strong>{finding.title}</strong><p>{finding.summary}</p></li>)}</ul>}
      </section>
      {artifacts.length > 0 && <section className="finding-board artifact-board" aria-labelledby="artifact-board-title">
        <div>
          <p className="eyebrow">Material evidence</p>
          <h3 id="artifact-board-title">Changed objects, without invented meaning.</h3>
        </div>
        <ul>{artifacts.map((artifact) => <li key={artifact.object_id}><span>surface trace {formatNumber(artifact.surface_trace_units)}</span><strong><a href={artifact.material.source_url} rel="noreferrer" target="_blank">{artifact.material.canonical_name}</a></strong><p>First observed at moment {formatNumber(artifact.first_trace_tick)}; latest trace at moment {formatNumber(artifact.latest_trace_tick)}.</p></li>)}</ul>
      </section>}
      {began && <p className="record-footnote">The record starts at moment {formatNumber(began.source_tick)} and currently ends at moment {formatNumber(world.tick)}. The observatory refreshes every 15 seconds.</p>}
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

function formatNumber(value: string | number) {
  const parsed = typeof value === "number" ? value : Number(value);
  return Number.isFinite(parsed) ? new Intl.NumberFormat("en-US").format(parsed) : String(value);
}
