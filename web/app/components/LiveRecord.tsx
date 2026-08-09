"use client";

import { useEffect, useState } from "react";
import { WorldInputStatus, type WorldInputMetadata } from "./WorldInputStatus";
import { createPublicLifeLabels } from "./lifeLabels";
import { HabitatStage } from "./HabitatStage";
import { commonSpeciesName } from "./speciesNames";

type World = WorldInputMetadata & {
  world_id: string;
  status: "initializing" | "running" | "extinct" | "archived";
  through_sequence: string | number;
  tick: string | number;
  manifest_hash: string;
  event_hash: string;
  state_hash: string;
};
type TimelineItem = { source_event_id: string; source_sequence: string | number; source_tick: string | number; title: string; summary: string };
type Organism = { organism_id: string; role: "person" | "fauna"; species: { scientific_name: string; source_url: string }; ended_event_id: string | null; introduced_sequence: string | number; introduced_tick: string | number };
type Finding = { finding_key: string; title: string; summary: string; kind: "first" | "record" | "streak" };
type Artifact = { object_id: string; material: { canonical_name: string; source_url: string }; first_trace_sequence: string | number; first_trace_tick: string | number; latest_trace_sequence: string | number; latest_trace_tick: string | number; surface_trace_units: number };
type RecordState =
  | { state: "loading" }
  | { state: "empty" }
  | { state: "error" }
  | { state: "ready"; world: World; timeline: TimelineItem[]; organisms: Organism[]; findings: Finding[]; artifacts: Artifact[] };

export function LiveRecord() {
  const [record, setRecord] = useState<RecordState>({ state: "loading" });
  const [followedOrganismId, setFollowedOrganismId] = useState<string | null>(() =>
    typeof window === "undefined" ? null : window.localStorage.getItem("atiny.followed-organism"),
  );

  useEffect(() => {
    let active = true;
    async function refresh() {
      try {
        const worldsResponse = await fetch("/api/v1/worlds", { cache: "no-store" });
        if (!worldsResponse.ok) throw new Error("world list unavailable");
        const { worlds } = (await worldsResponse.json()) as { worlds: World[] };
        const world = worlds.find((item) => item.status === "running") ?? worlds[0];
        if (!world) { if (active) setRecord({ state: "empty" }); return; }
        const encoded = encodeURIComponent(world.world_id);
        const [timelineResponse, organismsResponse, findingsResponse, artifactsResponse] = await Promise.all([
          fetch(`/api/v1/worlds/${encoded}/timeline?limit=12`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${encoded}/organisms?limit=200`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${encoded}/findings?limit=12`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${encoded}/artifacts?limit=12`, { cache: "no-store" }),
        ]);
        if (!timelineResponse.ok || !organismsResponse.ok || !findingsResponse.ok || !artifactsResponse.ok) throw new Error("world data unavailable");
        const timeline = (await timelineResponse.json()) as { items: TimelineItem[] };
        const organisms = (await organismsResponse.json()) as { organisms: Organism[] };
        const findings = (await findingsResponse.json()) as { findings: Finding[] };
        const artifacts = (await artifactsResponse.json()) as { artifacts: Artifact[] };
        if (active) setRecord({ state: "ready", world, timeline: timeline.items, organisms: organisms.organisms, findings: findings.findings, artifacts: artifacts.artifacts });
      } catch { if (active) setRecord({ state: "error" }); }
    }
    void refresh();
    const timer = window.setInterval(refresh, 15_000);
    return () => { active = false; window.clearInterval(timer); };
  }, []);

  if (record.state !== "ready") return <WaitingWorld state={record.state} />;

  const { world, timeline, organisms, findings, artifacts } = record;
  const people = organisms.filter((organism) => organism.role === "person" && !organism.ended_event_id);
  const animals = organisms.filter((organism) => organism.role === "fauna" && !organism.ended_event_id);
  const followed = organisms.find((organism) => organism.organism_id === followedOrganismId);
  const featured = followed ?? people[0] ?? animals[0];
  const lifeLabels = createPublicLifeLabels(organisms);
  const labelFor = (organism: Organism) => lifeLabels.get(organism.organism_id) ?? "Unindexed life";
  const latestMoment = timeline[0];

  return (
    <section className="living-record" id="happening" aria-labelledby="live-record-title">
      <div className="living-hero living-habitat-hero">
        <HabitatStage worldId={world.world_id} worldTick={world.tick} labels={lifeLabels} />
      </div>
      <div className="living-habitat-caption">
        <div>
          <p className="living-live-label"><span aria-hidden="true" /> Unscripted and happening now</p>
          <h1 id="live-record-title">Watch life unfold.</h1>
        </div>
        <div>
          <p>Every point is one person or animal at its latest committed position. Zoom in, select a life, and stay with it as the world changes.</p>
          {latestMoment && <small><strong>Latest lasting event</strong>{latestMoment.title} · moment {formatNumber(latestMoment.source_tick)}</small>}
          <WorldInputStatus world={world} />
        </div>
      </div>

      <div className="living-glance" aria-label="Current world at a glance">
        <article><span>People alive</span><strong>{formatNumber(people.length)}</strong><small>individual public records</small></article>
        <article><span>Animals alive</span><strong>{formatNumber(animals.length)}</strong><small>individually represented</small></article>
        <article><span>Recorded moments</span><strong>{formatNumber(world.through_sequence)}</strong><small>append-only history</small></article>
        <article><span>Observed findings</span><strong>{formatNumber(findings.length)}</strong><small>firsts, records, and streaks</small></article>
      </div>

      <section className="living-lives" id="people" aria-labelledby="lives-title">
        <header>
          <p className="eyebrow">Every life has a history</p>
          <h2 id="lives-title">Start with someone.</h2>
          <p>There are no protagonists here. The observatory identifies each person as Human N and each animal by recognizable species and number. These labels stand in for names only until the inhabitants discover naming for themselves.</p>
        </header>
        <div className="living-lives-grid">
          <article className="living-featured-life">
            <div className="living-life-portrait" aria-hidden="true"><span /></div>
            <div>
              <p>{featured ? `${labelFor(featured)} · ${featured.ended_event_id ? "record ended" : "alive"}` : "No life in the public record"}</p>
              <h3>{featured ? "A life at the beginning" : "Waiting for a life"}</h3>
              {featured ? <><a href={featured.species.source_url} target="_blank" rel="noreferrer" title={featured.species.scientific_name}>{commonSpeciesName(featured.species.scientific_name)}</a><small>Present since moment {formatNumber(featured.introduced_tick)}. {labelFor(featured)} is an observer ID, not a name known inside the world. If inhabitants develop names, their name becomes primary and this ID remains for audit.</small><div className="living-life-actions"><a href={lifeHref(world.world_id, featured.organism_id)}>Open this life</a><button type="button" className={followed?.organism_id === featured.organism_id ? "is-following" : undefined} onClick={() => followLife(featured.organism_id, setFollowedOrganismId)}>{followed?.organism_id === featured.organism_id ? "Following" : "Follow this life"}</button></div></> : <small>The observatory will open a biography when the public record contains one.</small>}
            </div>
          </article>
          <article className="living-recent">
            <p className="eyebrow">Recent moments</p>
            {timeline.length === 0 ? <p className="living-quiet">Nothing public has changed yet. Quiet time remains quiet.</p> : <ol>{timeline.slice(0, 5).map((item) => <li key={item.source_event_id}><time>Moment {formatNumber(item.source_tick)}</time><div><strong>{item.title}</strong><span>{item.summary}</span></div></li>)}</ol>}
          </article>
        </div>
        {organisms.length > 1 && <div className="living-life-ribbon" id="animals">{organisms.filter((organism) => organism.organism_id !== featured?.organism_id).slice(0, 8).map((organism) => <a href={lifeHref(world.world_id, organism.organism_id)} key={organism.organism_id}><span>{lifeMonogram(labelFor(organism))}</span><div><strong>{labelFor(organism)}</strong><small title={organism.species.scientific_name}>{commonSpeciesName(organism.species.scientific_name)} · {organism.ended_event_id ? "record ended" : "alive"}</small></div></a>)}</div>}
        <a className="living-browse-lives" href={`/lives?world=${encodeURIComponent(world.world_id)}`}>Browse all {formatNumber(organisms.length)} recorded lives <span aria-hidden="true">→</span></a>
      </section>

      <section className="living-evidence" id="discoveries" aria-labelledby="evidence-title">
        <div className="living-section-heading"><div><p className="eyebrow">What the record supports</p><h2 id="evidence-title">Evidence before interpretation.</h2></div><p>The observatory finds noteworthy patterns. It never adds knowledge to the inhabitants or turns guesses into history.</p></div>
        {findings.length === 0 ? <div className="living-empty-evidence"><span>00</span><p>No first, record, or durable streak has crossed the public threshold yet.</p></div> : <div className="living-finding-grid">{findings.map((finding) => <article key={finding.finding_key}><span>{finding.kind}</span><h3>{finding.title}</h3><p>{finding.summary}</p></article>)}</div>}
        {artifacts.length > 0 && <div className="living-artifacts"><p className="eyebrow">Material traces</p>{artifacts.map((artifact) => <article key={artifact.object_id}><strong><a href={artifact.material.source_url} target="_blank" rel="noreferrer">{artifact.material.canonical_name}</a></strong><span>First changed at moment {formatNumber(artifact.first_trace_tick)} · latest trace {formatNumber(artifact.latest_trace_tick)} · surface trace {formatNumber(artifact.surface_trace_units)}</span></article>)}</div>}
      </section>

      <details className="living-verification">
        <summary>Verify this history <span>seed, event head, and state hashes</span></summary>
        <dl><div><dt>Manifest</dt><dd title={world.manifest_hash}>{shortHash(world.manifest_hash)}</dd></div><div><dt>Event head</dt><dd title={world.event_hash}>{shortHash(world.event_hash)}</dd></div><div><dt>State</dt><dd title={world.state_hash}>{shortHash(world.state_hash)}</dd></div>{world.composition_hash && <div><dt>Input composition</dt><dd title={world.composition_hash}>{shortHash(world.composition_hash)}</dd></div>}</dl>
      </details>
    </section>
  );
}

function WaitingWorld({ state }: { state: "loading" | "empty" | "error" }) {
  const copy = state === "loading" ? "Reading the live record…" : state === "empty" ? "The next world has not begun." : "The live record is briefly out of reach.";
  return <section className="living-record living-waiting"><div className="living-hero"><div className="living-hero-copy"><p className="living-live-label"><span aria-hidden="true" /> Public observatory</p><p className="living-life-label">History begins at genesis</p><h1>A world where every life writes its own story.</h1><p className="living-standfirst">{copy}</p></div><PlanetStage people={0} animals={0} /></div></section>;
}

function PlanetStage({ people, animals, latest }: { people: number; animals: number; latest?: TimelineItem }) {
  return <div className="living-planet-stage" role="img" aria-label="Artistic full-Earth observatory view; it does not claim to show live positions"><div className="living-orbit orbit-one" /><div className="living-orbit orbit-two" /><div className="living-planet"><span className="continent continent-one" /><span className="continent continent-two" /><span className="continent continent-three" /><span className="planet-glow" /></div><div className="living-signal signal-one"><i /><span>{formatNumber(people)} people</span></div><div className="living-signal signal-two"><i /><span>{formatNumber(animals)} animals</span></div><div className="living-planet-note"><span>Latest public record</span><strong>{latest?.title ?? "Awaiting a committed moment"}</strong><small>This globe is a reference view, not a map of organism positions.</small></div></div>;
}

function shortHash(hash: string) { return `${hash.slice(0, 12)}…${hash.slice(-8)}`; }
function formatNumber(value: string | number) { const parsed = typeof value === "number" ? value : Number(value); return Number.isFinite(parsed) ? new Intl.NumberFormat("en-US").format(parsed) : String(value); }
function lifeMonogram(label: string) { return label.match(/\d+$/)?.[0] ?? label.slice(0, 1); }
function lifeHref(worldId: string, organismId: string) { return `/lives/${encodeURIComponent(worldId)}/${encodeURIComponent(organismId)}`; }
function followLife(organismId: string, update: (organismId: string) => void) { window.localStorage.setItem("atiny.followed-organism", organismId); update(organismId); }
