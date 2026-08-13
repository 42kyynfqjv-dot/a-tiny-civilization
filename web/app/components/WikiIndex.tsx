"use client";

import { useEffect, useState } from "react";
import { WorldInputStatus, type WorldInputMetadata } from "./WorldInputStatus";
import { createPublicLifeLabels } from "./lifeLabels";
import { commonSpeciesName } from "./speciesNames";

type World = WorldInputMetadata & {
  world_id: string;
  status: "initializing" | "running" | "extinct" | "archived" | "retired";
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

type Artifact = {
  object_id: string;
  material: { canonical_name: string; source_url: string };
  trace_provenance: "world_fact";
  classification_provenance: "observer_inference";
  first_trace_sequence: string | number;
  first_trace_tick: string | number;
  latest_trace_sequence: string | number;
  latest_trace_tick: string | number;
  surface_trace_units: number;
};

type LanguageConvention = {
  signal_form: number;
  tentative_gloss: string;
  evidence_events: number;
  learners: number;
  signal_sources: number;
  dominance_percent: number;
  baseline_percent: number;
  baseline_lift_percent: number;
  first_sequence: string | number;
  first_tick: string | number;
  latest_sequence: string | number;
  latest_tick: string | number;
};

type EmergingLanguagePattern = {
  pattern: LanguageConvention;
  thresholds_met: number;
  thresholds_required: number;
  earlier_half_evidence_events: number;
  recent_half_evidence_events: number;
  earlier_half_dominance_percent: number;
  recent_half_dominance_percent: number;
  trend: "strengthening" | "stable" | "weakening";
};

type LanguageArchive = {
  detector_version: number;
  stage: "undetected" | "proto_lexicon" | "rudimentary_language_candidate";
  threshold: { evidence_window_ticks: number; minimum_evidence_events: number; minimum_learners: number; minimum_signal_sources: number; minimum_tick_span: number; minimum_dominance_percent: number; minimum_baseline_margin_percent: number; minimum_baseline_lift_percent: number; minimum_half_evidence_events: number; minimum_half_dominance_percent: number; conventions_for_language_candidate: number };
  conventions: LanguageConvention[];
  emerging_patterns: EmergingLanguagePattern[];
};

type WikiState =
  | { state: "loading" }
  | { state: "empty" }
  | { state: "error" }
  | { state: "ready"; world: World; findings: Finding[]; organisms: Organism[]; artifacts: Artifact[]; language: LanguageArchive };

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
        // The API orders the ordinary observer world before hidden experiments.
        // Preserve that boundary during a brief successor handoff as well.
        const world = worlds[0];
        if (!world) {
          if (active) setWiki({ state: "empty" });
          return;
        }

        const worldId = encodeURIComponent(world.world_id);
        const [findingsResponse, organismsResponse, artifactsResponse, languageResponse] = await Promise.all([
          fetch(`/api/v1/worlds/${worldId}/findings?limit=24`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${worldId}/organisms?limit=200`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${worldId}/artifacts?limit=24`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${worldId}/language`, { cache: "no-store" }),
        ]);
        if (!findingsResponse.ok || !organismsResponse.ok || !artifactsResponse.ok || !languageResponse.ok) throw new Error("wiki records unavailable");
        const findings = (await findingsResponse.json()) as { findings: Finding[] };
        const organisms = (await organismsResponse.json()) as { organisms: Organism[] };
        const artifacts = (await artifactsResponse.json()) as { artifacts: Artifact[] };
        const language = (await languageResponse.json()) as LanguageArchive;
        if (active) setWiki({ state: "ready", world, findings: findings.findings, organisms: organisms.organisms, artifacts: artifacts.artifacts, language });
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
  const lifeLabels = createPublicLifeLabels(wiki.organisms);

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
          <p>Each person appears as Human N and each animal by recognizable species and number until the inhabitants independently develop naming. Emergent names can become the primary display; the underlying audit identity remains permanent.</p>
          {wiki.organisms.length === 0 ? <p>No individual public records are available yet.</p> : <ul>{wiki.organisms.map((organism) => <li key={organism.organism_id}><span>{organism.role === "person" ? "Human ID" : `${commonSpeciesName(organism.species.scientific_name)} ID`}</span><a href={`/lives/${encodeURIComponent(wiki.world.world_id)}/${encodeURIComponent(organism.organism_id)}`}>{lifeLabels.get(organism.organism_id) ?? "Unindexed life"}</a><small><a href={organism.species.source_url} rel="noreferrer" target="_blank" title={organism.species.scientific_name}>{commonSpeciesName(organism.species.scientific_name)}</a> · introduced at event {organism.introduced_sequence} · {organism.ended_event_id ? "record ended" : "present in record"}</small></li>)}</ul>}
        </article>
        <article id="artifact-archive">
          <h3>Altered material archive</h3>
          {wiki.artifacts.length === 0 ? <p>No durable material alteration has entered the public record.</p> : <ul>{wiki.artifacts.map((artifact) => <li key={artifact.object_id}><a href={artifact.material.source_url} rel="noreferrer" target="_blank">{artifact.material.canonical_name}</a><span>Observed surface trace: {artifact.surface_trace_units} units</span><small>Physical trace: world fact · artifact filing: observer inference</small><small>First evidence event {artifact.first_trace_sequence}, tick {artifact.first_trace_tick} · latest event {artifact.latest_trace_sequence}, tick {artifact.latest_trace_tick}</small></li>)}</ul>}
        </article>
        <article id="language-archive">
          <h3>Language archive and translation</h3>
          <p>{languageStage(wiki.language.stage)}</p>
          {wiki.language.conventions.length === 0 ? <p>Signal emissions alone do not qualify. Detector v{wiki.language.detector_version} examines the latest {wiki.language.threshold.evidence_window_ticks} ticks, so early babbling cannot dilute a later convention forever. A convention needs repeated person-to-person grounding, social spread, distinctiveness from background behavior, and persistence across both halves of that window.</p> : <ol>{wiki.language.conventions.map((convention) => <li key={`${convention.signal_form}:${convention.tentative_gloss}`}><strong>Signal form {convention.signal_form} · “{convention.tentative_gloss}”</strong><span>Tentative observer gloss · {convention.dominance_percent}% after this form versus {convention.baseline_percent}% ordinarily · {convention.baseline_lift_percent}% lift</span><small>{convention.evidence_events} events · {convention.learners} learners · {convention.signal_sources} sources</small><small>First evidence event {convention.first_sequence}, tick {convention.first_tick} · latest event {convention.latest_sequence}, tick {convention.latest_tick}</small></li>)}</ol>}
          {wiki.language.emerging_patterns.length > 0 ? <div className="language-emerging"><h4>Patterns taking shape</h4><p>These are repeated learned mappings, not words. The meter shows how many conservative gates they currently pass.</p><ol>{wiki.language.emerging_patterns.map(({ pattern, thresholds_met, thresholds_required, earlier_half_dominance_percent, recent_half_dominance_percent, trend }) => <li key={`${pattern.signal_form}:${pattern.tentative_gloss}`}><strong>Signal form {pattern.signal_form} may precede {pattern.tentative_gloss}</strong><span>{thresholds_met}/{thresholds_required} evidence gates · {trend}</span><small>{pattern.evidence_events} observations · {pattern.learners} learners · {pattern.signal_sources} sources</small><small>Earlier / recent consistency: {earlier_half_dominance_percent}% / {recent_half_dominance_percent}%</small></li>)}</ol></div> : null}
          <p>Dictionary entries are observer research over committed evidence. They never teach, steer, or reveal a translation to the inhabitants.</p>
        </article>
      </div>
    </section>
  );
}

function languageStage(stage: LanguageArchive["stage"]) {
  if (stage === "rudimentary_language_candidate") return "Several stable, socially learned conventions now support a rudimentary language candidate. This is not evidence of grammar or human-like language.";
  if (stage === "proto_lexicon") return "At least one stable, socially learned signal convention supports a proto-lexicon; it is not yet classified as a language.";
  return "No stable signal convention has crossed the public evidence threshold. The world currently has signaling, not an established language.";
}
