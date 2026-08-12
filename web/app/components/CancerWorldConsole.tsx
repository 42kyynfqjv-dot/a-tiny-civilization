"use client";

import { FormEvent, useEffect, useState } from "react";

type ResearchProgram = "devices" | "treatments";
type ResearchFilter = "all" | "promising" | "killed" | "inconclusive" | "needs_testing";

type FoundationStatus = {
  latest_runner_heartbeat: string | null;
  latest_memory_worker_heartbeat: string | null;
  latest_cognition_worker_heartbeat: string | null;
};

type Telemetry = {
  through_sequence: string | number;
  tick: string | number;
  committed_events: string | number;
  last_committed_at: string;
  living_people: string | number;
};

type Claim = {
  statement: string;
  testable_prediction: string;
  falsification_test: string;
  citation_hashes: string[];
};

type Contribution = {
  contribution_id: string;
  stage: string;
  artifact_kind: string;
  title: string;
  abstract_text: string;
  claims: Claim[];
  virtual_experiment_plan?: {
    subject_model: string;
    intervention_modality: string;
    primary_target: string;
    secondary_target: string | null;
    primary_endpoint: string;
    intensity_parts_per_million: number;
    exposure_hours: number;
    cohort_size: number;
  } | null;
};

type ResearchDuplicate = {
  request_id: string;
  ordinal: number;
  title: string;
  artifact_hash: string;
  result_hash: string;
  created_at: string;
};

type NoveltyAudit = {
  schema_version: number;
  method_version: number;
  audit_id: string;
  world_id: string;
  request_id: string;
  artifact_hash: string;
  query_terms: string[];
  status: "known_overlap" | "new_combination" | "no_close_match_found" | "possible_error";
  literature_overlap_per_mille: number;
  prior_world_overlap_per_mille: number;
  matches: {
    source_id: string;
    title: string;
    published_on: string | null;
    overlap_per_mille: number;
  }[];
  warnings: string[];
  audit_hash: string;
  created_at: string;
};

type VirtualExperiment = {
  schema_version: number;
  method_version: number;
  experiment_id: string;
  world_id: string;
  request_id: string;
  artifact_hash: string;
  plan_hash: string;
  subject_model: string;
  primary_endpoint: string;
  cohort_size: number;
  control_value_parts_per_million: number;
  intervention_value_parts_per_million: number;
  estimated_change_parts_per_million: number;
  uncertainty_low_parts_per_million: number;
  uncertainty_high_parts_per_million: number;
  interpretation: string;
  model_calibration: string;
  mechanistic_readout?: {
    schema_version: number;
    fidelity: string;
    calibration_grade: string;
    baseline_clones: CloneFractions;
    post_exposure_clones: CloneFractions;
    pharmacokinetics?: {
      systemic_exposure_parts_per_million: number;
      bbb_penetration_parts_per_million: number;
      unbound_brain_exposure_parts_per_million: number;
      effective_exposure_hours: number;
    };
    delivered_exposure_parts_per_million: number;
    target_engagement_parts_per_million: number;
    resistant_selection_parts_per_million: number;
  };
  caveats: string[];
  result_hash: string;
  memory_state: "queued" | "accepted";
  created_at: string;
};

type CloneFractions = {
  treatment_sensitive_parts_per_million: number;
  drug_tolerant_parts_per_million: number;
  resistant_parts_per_million: number;
};

type ResearchArtifact = {
  request_id: string;
  selected_at_tick: string | number;
  ordinal: number;
  program: ResearchProgram;
  target: string;
  task: string;
  inference_tier: string;
  frozen_candidate_hash: string | null;
  contribution: Contribution;
  artifact_hash: string;
  evidence: { kind: string; source_id: string; content_hash: string }[];
  recalled_artifact_hashes: string[];
  requested_model: string;
  resolved_model: string;
  prompt_tokens: number;
  completion_tokens: number;
  billed_micro_usd: number;
  result_hash: string;
  memory_state: "queued" | "accepted";
  novelty_audit: NoveltyAudit | null;
  virtual_experiment: VirtualExperiment | null;
  created_at: string;
  duplicates: ResearchDuplicate[];
};

type ResearchCampaign = {
  campaign_id: string;
  program: ResearchProgram;
  root_request_id: string;
  root_artifact_hash: string;
  root_title: string;
  outcome: "testing" | "falsified" | "survived_replication_round" | "inconclusive";
  supporting_tests: number;
  falsifying_tests: number;
  inconclusive_tests: number;
  synthesis_complete: boolean;
  newest_ordinal: number;
};

type LabCapability = {
  capability: string;
  status: "available" | "abstracted" | "missing" | "requires_real_lab";
  detail: string;
};

type ResearchProgramSummary = {
  program: ResearchProgram;
  distinct_artifacts: number;
  duplicate_artifacts: number;
  model_supported: number;
  model_rejected: number;
  model_inconclusive: number;
  awaiting_evaluation: number;
  newest_ordinal: number | null;
};

type ResearchEvidence = {
  evidence_id: string;
  source_id: string;
  title: string;
  license: string;
  published_at: string | null;
  content_hash: string;
  retrieved_at: string;
};

type ResearchView = {
  world_id: string;
  memory_bank_id: string;
  target: string;
  total_requests: number;
  pending_requests: number;
  successful_requests: number;
  unsuccessful_requests: number;
  distinct_artifacts: number;
  duplicate_artifacts: number;
  memory_queued: number;
  memory_accepted: number;
  programs: ResearchProgramSummary[];
  campaigns?: ResearchCampaign[];
  lab_capabilities?: LabCapability[];
  artifacts: ResearchArtifact[];
  evidence: ResearchEvidence[];
};

type SearchResult = {
  document_id: string;
  sim_tick: string | number;
  ordinal: number;
  text: string;
  context: string;
};

type SearchOutcome =
  | { status: "available"; results: SearchResult[] }
  | { status: "unavailable"; reason: string };

type ConsoleRecord = {
  status: FoundationStatus;
  telemetry: Telemetry | null;
  research: ResearchView | null;
  checkedAt: string;
};

export function CancerWorldConsole({ worldId }: { worldId: string }) {
  const [record, setRecord] = useState<ConsoleRecord | null>(null);
  const [online, setOnline] = useState(true);
  const [query, setQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [searchOutcome, setSearchOutcome] = useState<SearchOutcome | null>(null);
  const [program, setProgram] = useState<ResearchProgram>("treatments");
  const [filter, setFilter] = useState<ResearchFilter>("all");

  useEffect(() => {
    let active = true;
    async function refresh() {
      try {
        const statusResponse = await fetch("/api/v1/status", { cache: "no-store" });
        if (!statusResponse.ok) throw new Error("status");
        const status = (await statusResponse.json()) as FoundationStatus;
        let telemetry: Telemetry | null = null;
        let research: ResearchView | null = null;
        if (worldId) {
          const id = encodeURIComponent(worldId);
          const [telemetryResponse, researchResponse] = await Promise.all([
            fetch(`/api/v1/worlds/${id}/telemetry`, { cache: "no-store" }),
            fetch(`/api/v1/worlds/${id}/research?limit=240`, { cache: "no-store" }),
          ]);
          if (telemetryResponse.ok) telemetry = (await telemetryResponse.json()) as Telemetry;
          if (researchResponse.ok) research = (await researchResponse.json()) as ResearchView;
        }
        if (active) {
          setRecord({ status, telemetry, research, checkedAt: new Date().toISOString() });
          setOnline(true);
        }
      } catch {
        if (active) setOnline(false);
      }
    }
    void refresh();
    const timer = window.setInterval(refresh, 10_000);
    return () => { active = false; window.clearInterval(timer); };
  }, [worldId]);

  async function searchMemory(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const normalized = query.trim();
    if (!worldId || !normalized) return;
    setSearching(true);
    try {
      const response = await fetch(
        `/api/v1/worlds/${encodeURIComponent(worldId)}/research/search?q=${encodeURIComponent(normalized)}`,
        { cache: "no-store" },
      );
      if (!response.ok) throw new Error("search");
      setSearchOutcome((await response.json()) as SearchOutcome);
    } catch {
      setSearchOutcome({ status: "unavailable", reason: "research_memory_unavailable" });
    } finally {
      setSearching(false);
    }
  }

  const telemetry = record?.telemetry;
  const research = record?.research;
  const summary = research?.programs?.find((item) => item.program === program);
  const programCampaigns = research?.campaigns?.filter((campaign) => campaign.program === program) ?? [];
  const programArtifacts = research?.artifacts.filter((artifact) => artifact.program === program) ?? [];
  const visibleArtifacts = programArtifacts.filter((artifact) => filter === "all" || artifactStatus(artifact) === filter);
  return <main className="cancer-console" data-program={program}>
    <header className="cancer-console-header">
      <div><span className={`cancer-console-pulse ${online ? "online" : "offline"}`} />CANCER WORLD / RESEARCH CONSOLE</div>
      <time>{record?.checkedAt ?? "CONNECTING"}</time>
    </header>

    <section className="cancer-console-intro">
      <div>
        <p className="cancer-console-kicker">LIVE RESEARCH WORLD · {humanize(research?.target ?? "adult_glioblastoma")}</p>
        <h1>Two programs.<br />One impossible problem.</h1>
        <p>One group builds ways to see the disease. The other tries to change it. Every idea is remembered, tested where the model allows, and either advanced, questioned, or killed.</p>
      </div>
      <div className="cancer-console-state">
        <span>{telemetry ? "RUNNING" : "AWAITING GENESIS"}</span>
        <strong>{research?.distinct_artifacts ?? 0}</strong>
        <small>distinct ideas in the library</small>
      </div>
    </section>

    <section className="cancer-console-grid" aria-label="Live experiment metrics">
      <Metric label="WORLD TICK" value={telemetry?.tick ?? "—"} />
      <Metric label="RESEARCH ATTEMPTS" value={research?.total_requests ?? 0} />
      <Metric label="IDEAS KEPT" value={research?.distinct_artifacts ?? 0} />
      <Metric label="REPEATS HIDDEN" value={research?.duplicate_artifacts ?? 0} />
      <Metric label="LIBRARY CONNECTED" value={research?.memory_accepted ?? 0} />
      <Metric label="PEOPLE" value={telemetry?.living_people ?? "—"} />
    </section>

    <nav className="cancer-program-switcher" aria-label="Research programs">
      {(["devices", "treatments"] as ResearchProgram[]).map((item) => {
        const itemSummary = research?.programs?.find((candidate) => candidate.program === item);
        return <button aria-pressed={program === item} key={item} onClick={() => { setProgram(item); setFilter("all"); }} type="button">
          <span>{item === "devices" ? "PROGRAM 01" : "PROGRAM 02"}</span>
          <strong>{humanize(item)}</strong>
          <p>{item === "devices" ? "Instruments, imaging, sensing, and experimental machines." : "Mechanisms, interventions, delivery systems, and treatment machines."}</p>
          <small>{itemSummary?.distinct_artifacts ?? 0} ideas · {itemSummary?.model_rejected ?? 0} killed</small>
        </button>;
      })}
    </nav>

    <section className="cancer-console-section cancer-program-feed">
      <SectionHeading
        eyebrow={program === "devices" ? "PROGRAM 01 · DEVICES" : "PROGRAM 02 · TREATMENTS"}
        title={program === "devices" ? "Machines that help us see" : "Ideas meant to change the disease"}
        detail={`${summary?.distinct_artifacts ?? 0} distinct ideas · newest turn ${summary?.newest_ordinal ?? "—"}`}
      />
      <div className="cancer-program-scoreboard" aria-label={`${program} outcomes`}>
        <ProgramScore label="Promising" value={summary?.model_supported ?? 0} tone="promising" />
        <ProgramScore label="Killed" value={summary?.model_rejected ?? 0} tone="killed" />
        <ProgramScore label="Inconclusive" value={summary?.model_inconclusive ?? 0} tone="inconclusive" />
        <ProgramScore label="Needs testing" value={summary?.awaiting_evaluation ?? 0} tone="needs_testing" />
      </div>
      <div className="cancer-campaigns">
        <div className="cancer-campaigns-heading">
          <span>THEORY CAMPAIGNS</span>
          <p>Only model-supported, low-overlap ideas enter. One adverse result kills a campaign; three distinct supporting model tests are required to survive a round.</p>
        </div>
        {programCampaigns.length ? programCampaigns.map((campaign) => <CampaignCard campaign={campaign} key={campaign.campaign_id} />) : <EmptyState text="No idea in this program has earned an adversarial replication campaign yet." />}
      </div>
      <div className="cancer-research-filters" aria-label="Filter research outcomes">
        {(["all", "promising", "killed", "inconclusive", "needs_testing"] as ResearchFilter[]).map((item) => <button aria-pressed={filter === item} key={item} onClick={() => setFilter(item)} type="button">
          {item === "all" ? "Everything" : humanize(item)}
        </button>)}
      </div>
      <div className="cancer-artifact-list">
        {visibleArtifacts.length ? visibleArtifacts.map((artifact) => <ArtifactCard key={artifact.request_id} artifact={artifact} />) : <EmptyState text={`No ${humanize(filter)} ${program} research is in the current window.`} />}
      </div>
    </section>

    <details className="cancer-console-section cancer-library-drawer">
      <summary><span>RESEARCH LIBRARY</span><strong>Search memory, papers, and provenance</strong><small>Open the technical archive</small></summary>
      <div className="cancer-memory-lab">
      <SectionHeading eyebrow="MEMORY" title="Search the shared research library" detail="Both programs can retrieve earlier work, build on it, and avoid repeating it" />
      <form onSubmit={searchMemory} className="cancer-search-form">
        <input
          aria-label="Search research memory"
          maxLength={4096}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="e.g. Which hypotheses connect clone diversity to immune engagement?"
          value={query}
        />
        <button disabled={searching || !query.trim()} type="submit">{searching ? "SEARCHING…" : "SEARCH MEMORY"}</button>
      </form>
      {searchOutcome && <SearchResults outcome={searchOutcome} />}
      <div className="cancer-memory-ledger">
        <span>BANK</span><code>{research?.memory_bank_id ?? "connecting"}</code>
        <span>QUEUE</span><strong>{research?.memory_queued ?? 0} waiting / {research?.memory_accepted ?? 0} accepted</strong>
      </div>
      </div>

      <SectionHeading eyebrow="EVIDENCE" title="Literature mirror" detail={`${research?.evidence.length ?? 0} immutable CC BY / CC0 snapshots`} />
      <div className="cancer-evidence-list">
        {research?.evidence.length ? research.evidence.map((item) => <a href={item.source_id} key={item.evidence_id} rel="noreferrer" target="_blank">
          <span>{item.license.toUpperCase()} · {item.published_at ?? "DATE UNAVAILABLE"}</span>
          <strong>{item.title}</strong>
          <code>{shortHash(item.content_hash)}</code>
        </a>) : <EmptyState text="No licensed evidence snapshots are available yet." />}
      </div>
    </details>

    <details className="cancer-console-section cancer-lab-boundary">
      <summary><span>SIMULATED LAB</span><strong>What this system can—and cannot—test</strong><small>Open the capability boundary</small></summary>
      <p className="cancer-model-only">THE VIRTUAL LAB TRIAGES IDEAS. IT DOES NOT PRODUCE BIOLOGICAL OR CLINICAL EVIDENCE.</p>
      <div className="cancer-lab-capabilities">
        {(research?.lab_capabilities ?? []).map((item) => <article data-status={item.status} key={item.capability}>
          <span>{humanize(item.status)}</span>
          <strong>{item.capability}</strong>
          <p>{item.detail}</p>
        </article>)}
      </div>
    </details>

    <details className="cancer-console-ledger">
      <summary>SYSTEM LEDGER</summary>
      <dl>
      <Row label="WORLD" value={worldId || "pending"} />
      <Row label="SEQUENCE" value={String(telemetry?.through_sequence ?? "—")} />
      <Row label="EVENTS" value={String(telemetry?.committed_events ?? "—")} />
      <Row label="LAST COMMIT" value={telemetry?.last_committed_at ?? "—"} />
      <Row label="RUNNER" value={record?.status.latest_runner_heartbeat ?? "—"} />
      <Row label="HINDSIGHT WORKER" value={record?.status.latest_memory_worker_heartbeat ?? "—"} />
      <Row label="RESEARCH WORKER" value={record?.status.latest_cognition_worker_heartbeat ?? "—"} />
      </dl>
    </details>
  </main>;
}

function CampaignCard({ campaign }: { campaign: ResearchCampaign }) {
  const tests = campaign.supporting_tests + campaign.falsifying_tests + campaign.inconclusive_tests;
  return <article className="cancer-campaign-card" data-outcome={campaign.outcome}>
    <div><span>{humanize(campaign.outcome)}</span><small>{tests}/5 TESTS · TURN {campaign.newest_ordinal}</small></div>
    <h3>{campaign.root_title}</h3>
    <dl>
      <div><dt>SUPPORTED</dt><dd>{campaign.supporting_tests}</dd></div>
      <div><dt>FALSIFIED</dt><dd>{campaign.falsifying_tests}</dd></div>
      <div><dt>INCONCLUSIVE</dt><dd>{campaign.inconclusive_tests}</dd></div>
    </dl>
    <footer><code>ROOT {shortHash(campaign.root_artifact_hash)}</code><span>{campaign.synthesis_complete ? "SYNTHESIS COMPLETE" : "ACTIVE / AWAITING NEXT STEP"}</span></footer>
  </article>;
}

function ArtifactCard({ artifact }: { artifact: ResearchArtifact }) {
  const contribution = artifact.contribution;
  const novelty = artifact.novelty_audit;
  const status = artifactStatus(artifact);
  return <article className="cancer-artifact-card" data-status={status}>
    <div className="cancer-artifact-meta">
      <span>TURN {artifact.ordinal}</span>
      <span className={`research-outcome ${status}`}>{statusLabel(status)}</span>
      <span>{humanize(contribution.artifact_kind)}</span>
      <span>{humanize(contribution.stage)}</span>
      {artifact.duplicates.length > 0 && <span className="duplicate">{artifact.duplicates.length} DUPLICATE{artifact.duplicates.length === 1 ? "" : "S"} COLLAPSED</span>}
      <span className={`novelty ${novelty?.status ?? "pending"}`}>{noveltyLabel(novelty?.status)}</span>
      {artifact.virtual_experiment && <span className="virtual-run">MODEL TEST RUN</span>}
      <span className={artifact.memory_state === "accepted" ? "accepted" : "queued"}>{artifact.memory_state === "accepted" ? "MEMORY CONNECTED" : "MEMORY QUEUED"}</span>
    </div>
    <h3>{contribution.title}</h3>
    <p>{contribution.abstract_text}</p>
    <div className="cancer-artifact-chain">
      <span>{artifact.recalled_artifact_hashes.length ? `${artifact.recalled_artifact_hashes.length} earlier artifact${artifact.recalled_artifact_hashes.length === 1 ? "" : "s"} recalled` : "Independent starting point"}</span>
      <span>{artifact.evidence.length} evidence reference{artifact.evidence.length === 1 ? "" : "s"}</span>
      <span>{artifact.resolved_model}</span>
    </div>
    <details className="cancer-novelty-audit">
      <summary>{novelty ? `OVERLAP CHECK · ${noveltyLabel(novelty.status)}` : "OVERLAP CHECK · SCANNING"}</summary>
      {novelty ? <div className="cancer-novelty-body">
        <p>This is a literature-overlap finding aid, not proof of novelty or validity. It compares this artifact&apos;s mechanism terms with indexed papers and earlier Cancer World work.</p>
        <div className="cancer-novelty-scores">
          <span><strong>{formatScore(novelty.literature_overlap_per_mille)}</strong> closest literature overlap</span>
          <span><strong>{formatScore(novelty.prior_world_overlap_per_mille)}</strong> closest prior-world overlap</span>
        </div>
        {novelty.warnings.map((warning) => <p className="cancer-novelty-warning" key={warning}>{warning}</p>)}
        {novelty.matches.length > 0 ? <div className="cancer-novelty-matches">
          {novelty.matches.map((match) => <a href={match.source_id} key={match.source_id} rel="noreferrer" target="_blank">
            <span>{formatScore(match.overlap_per_mille)} · {match.published_on ?? "DATE UNAVAILABLE"}</span>
            <strong>{match.title}</strong>
          </a>)}
        </div> : <p>No sufficiently close result was returned by the bounded search. That does not establish scientific novelty.</p>}
        <small>METHOD V{novelty.method_version} · AUDIT {shortHash(novelty.audit_hash)}</small>
      </div> : <p className="cancer-novelty-pending">This new artifact is queued for the next literature scan.</p>}
    </details>
    {contribution.virtual_experiment_plan && <VirtualExperimentPanel artifact={artifact} />}
    {contribution.claims.map((claim, index) => <details key={`${artifact.request_id}-${index}`}>
      <summary>CLAIM {index + 1} · {claim.statement}</summary>
      <dl>
        <div><dt>PREDICTION</dt><dd>{claim.testable_prediction}</dd></div>
        <div><dt>FALSIFIED IF</dt><dd>{claim.falsification_test}</dd></div>
      </dl>
    </details>)}
    {artifact.duplicates.length > 0 && <details className="cancer-duplicate-ledger">
      <summary>DUPLICATE LEDGER · {artifact.duplicates.length} REPEATED TURN{artifact.duplicates.length === 1 ? "" : "S"}</summary>
      <div>
        {artifact.duplicates.map((duplicate) => <p key={duplicate.request_id}>
          <span>DUPLICATE · TURN {duplicate.ordinal}</span>
          <strong>{duplicate.title}</strong>
          <code>{shortHash(duplicate.artifact_hash)}</code>
        </p>)}
      </div>
    </details>}
    <footer><code>ARTIFACT {shortHash(artifact.artifact_hash)}</code><time>{formatTime(artifact.created_at)}</time></footer>
  </article>;
}

function VirtualExperimentPanel({ artifact }: { artifact: ResearchArtifact }) {
  const plan = artifact.contribution.virtual_experiment_plan;
  const result = artifact.virtual_experiment;
  if (!plan) return null;
  return <details className="cancer-virtual-experiment" open={Boolean(result)}>
    <summary>VIRTUAL LAB · {result ? humanize(result.interpretation) : "QUEUED"}</summary>
    {result ? <div className="cancer-virtual-body">
      <p className="cancer-model-only">MODEL PROJECTION ONLY · NOT WET-LAB, ANIMAL, OR CLINICAL EVIDENCE</p>
      <div className="cancer-virtual-plan">
        <span>MODEL<strong>{humanize(result.subject_model)}</strong></span>
        <span>ENDPOINT<strong>{humanize(result.primary_endpoint)}</strong></span>
        <span>COHORT<strong>{result.cohort_size}</strong></span>
        <span>MEMORY<strong>{humanize(result.memory_state)}</strong></span>
      </div>
      <div className="cancer-virtual-values">
        <span><small>CONTROL</small><strong>{formatParts(result.control_value_parts_per_million)}</strong></span>
        <span><small>INTERVENTION</small><strong>{formatParts(result.intervention_value_parts_per_million)}</strong></span>
        <span><small>ESTIMATED CHANGE</small><strong>{formatSignedParts(result.estimated_change_parts_per_million)}</strong></span>
        <span><small>MODEL INTERVAL</small><strong>{formatSignedParts(result.uncertainty_low_parts_per_million)} to {formatSignedParts(result.uncertainty_high_parts_per_million)}</strong></span>
      </div>
      {result.mechanistic_readout && <MechanisticReadout result={result.mechanistic_readout} />}
      {result.caveats.map((caveat) => <p key={caveat}>{caveat}</p>)}
      <small>LAB V{result.method_version} · RESULT {shortHash(result.result_hash)}</small>
    </div> : <p className="cancer-novelty-pending">The closed experiment plan is waiting for the deterministic virtual lab worker.</p>}
  </details>;
}

function MechanisticReadout({ result }: { result: NonNullable<VirtualExperiment["mechanistic_readout"]> }) {
  const pk = result.pharmacokinetics;
  return <div className="cancer-mechanistic-readout">
    <p className="cancer-mechanistic-label">STRUCTURAL MULTISCALE TRACE · {humanize(result.calibration_grade)}</p>
    <div className="cancer-virtual-values">
      <span><small>SENSITIVE CLONES</small><strong>{formatParts(result.baseline_clones.treatment_sensitive_parts_per_million)} → {formatParts(result.post_exposure_clones.treatment_sensitive_parts_per_million)}</strong></span>
      <span><small>DRUG-TOLERANT CLONES</small><strong>{formatParts(result.baseline_clones.drug_tolerant_parts_per_million)} → {formatParts(result.post_exposure_clones.drug_tolerant_parts_per_million)}</strong></span>
      <span><small>RESISTANT CLONES</small><strong>{formatParts(result.baseline_clones.resistant_parts_per_million)} → {formatParts(result.post_exposure_clones.resistant_parts_per_million)}</strong></span>
      <span><small>RESISTANCE SELECTION</small><strong>{formatSignedParts(result.resistant_selection_parts_per_million)}</strong></span>
    </div>
    <div className="cancer-virtual-values">
      <span><small>DELIVERED EXPOSURE</small><strong>{formatParts(result.delivered_exposure_parts_per_million)}</strong></span>
      <span><small>TARGET ENGAGEMENT</small><strong>{formatParts(result.target_engagement_parts_per_million)}</strong></span>
      <span><small>BBB PENETRATION</small><strong>{pk ? formatParts(pk.bbb_penetration_parts_per_million) : "N/A"}</strong></span>
      <span><small>UNBOUND BRAIN EXPOSURE</small><strong>{pk ? formatParts(pk.unbound_brain_exposure_parts_per_million) : "N/A"}</strong></span>
    </div>
  </div>;
}

function SearchResults({ outcome }: { outcome: SearchOutcome }) {
  if (outcome.status === "unavailable") return <p className="cancer-search-status">Memory search is temporarily unavailable: {humanize(outcome.reason)}</p>;
  if (!outcome.results.length) return <p className="cancer-search-status">No matching research memory was found.</p>;
  const results = collapseSearchResults(outcome.results);
  return <div className="cancer-search-results">
    {results.map(({ result, duplicateCount }) => {
      const parsed = parseContribution(result.text);
      return <article key={result.document_id}>
        <span>TURN {result.ordinal} · TICK {String(result.sim_tick)}{duplicateCount ? ` · ${duplicateCount} DUPLICATE${duplicateCount === 1 ? "" : "S"} HIDDEN` : ""}</span>
        <strong>{parsed?.title ?? "Recalled research artifact"}</strong>
        <p>{parsed?.abstract_text ?? result.text}</p>
      </article>;
    })}
  </div>;
}

function collapseSearchResults(results: SearchResult[]) {
  const distinct: { result: SearchResult; duplicateCount: number; title: string; kind: string }[] = [];
  for (const result of results) {
    const parsed = parseContribution(result.text);
    const title = parsed?.title ?? result.text;
    const kind = parsed?.artifact_kind ?? "unknown";
    const prior = distinct.find((candidate) => candidate.kind === kind && titlesDuplicate(candidate.title, title));
    if (prior) prior.duplicateCount += 1;
    else distinct.push({ result, duplicateCount: 0, title, kind });
  }
  return distinct;
}

function titlesDuplicate(left: string, right: string) {
  const leftTerms = titleTerms(left);
  const rightTerms = titleTerms(right);
  if (!leftTerms.size || !rightTerms.size) return false;
  if (leftTerms.size === rightTerms.size && [...leftTerms].every((term) => rightTerms.has(term))) return true;
  const intersection = [...leftTerms].filter((term) => rightTerms.has(term)).length;
  const union = new Set([...leftTerms, ...rightTerms]).size;
  return intersection >= 4 && intersection * 100 >= union * 82;
}

function titleTerms(title: string) {
  const ignored = new Set(["a", "adult", "an", "and", "as", "at", "by", "for", "from", "glioblastoma", "in", "into", "its", "of", "on", "role", "test", "the", "their", "to"]);
  const stems: Record<string, string> = {
    clonal: "clone", clones: "clone", driven: "drive", driver: "drive", drivers: "drive", drives: "drive", driving: "drive",
    modulated: "modulate", modulates: "modulate", modulation: "modulate", promotes: "promote", promoting: "promote",
    proliferation: "proliferate", proliferative: "proliferate", reprogrammed: "reprogram", reprogramming: "reprogram", trajectories: "trajectory",
  };
  return new Set(title.toLocaleLowerCase().replaceAll(/[^\p{L}\p{N}]+/gu, " ").trim().split(/\s+/).filter((term) => term && !ignored.has(term)).map((term) => stems[term] ?? term));
}

function parseContribution(text: string): Contribution | null {
  try {
    const parsed = JSON.parse(text) as Partial<Contribution>;
    return typeof parsed.title === "string" && typeof parsed.abstract_text === "string" ? parsed as Contribution : null;
  } catch {
    return null;
  }
}

function SectionHeading({ eyebrow, title, detail }: { eyebrow: string; title: string; detail: string }) {
  return <div className="cancer-section-heading"><div><span>{eyebrow}</span><h2>{title}</h2></div><p>{detail}</p></div>;
}

function ProgramScore({ label, value, tone }: { label: string; value: number; tone: ResearchFilter }) {
  return <article className={tone}>
    <strong>{value}</strong><span>{label}</span>
  </article>;
}

function artifactStatus(artifact: ResearchArtifact): Exclude<ResearchFilter, "all"> {
  switch (artifact.virtual_experiment?.interpretation) {
    case "model_supports_prediction": return "promising";
    case "model_shows_no_material_effect":
    case "model_shows_concerning_tradeoff": return "killed";
    case "model_inconclusive": return "inconclusive";
    default: return "needs_testing";
  }
}

function statusLabel(status: Exclude<ResearchFilter, "all">) {
  switch (status) {
    case "promising": return "PROMISING IN MODEL";
    case "killed": return "KILLED BY MODEL";
    case "inconclusive": return "INCONCLUSIVE";
    case "needs_testing": return "NEEDS TESTING";
  }
}

function EmptyState({ text }: { text: string }) {
  return <div className="cancer-empty-state"><span>⋯</span><p>{text}</p></div>;
}

function Metric({ label, value }: { label: string; value: string | number }) {
  return <article><span>{label}</span><strong>{String(value)}</strong></article>;
}

function Row({ label, value }: { label: string; value: string }) {
  return <div><dt>{label}</dt><dd>{value}</dd></div>;
}

function humanize(value: string) {
  return value.replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function shortHash(value: string) {
  return value.length > 16 ? `${value.slice(0, 10)}…${value.slice(-6)}` : value;
}

function formatTime(value: string) {
  try { return new Intl.DateTimeFormat("en", { dateStyle: "medium", timeStyle: "short" }).format(new Date(value)); }
  catch { return value; }
}

function noveltyLabel(status: NoveltyAudit["status"] | undefined) {
  switch (status) {
    case "known_overlap": return "KNOWN OVERLAP";
    case "new_combination": return "NEW COMBINATION";
    case "no_close_match_found": return "NO CLOSE MATCH FOUND";
    case "possible_error": return "CHECK NEEDED";
    default: return "NOT YET AUDITED";
  }
}

function formatScore(perMille: number) {
  return `${Math.round(perMille / 10)}%`;
}

function formatParts(partsPerMillion: number) {
  return `${(partsPerMillion / 10_000).toFixed(1)}%`;
}

function formatSignedParts(partsPerMillion: number) {
  const value = partsPerMillion / 10_000;
  return `${value > 0 ? "+" : ""}${value.toFixed(1)}%`;
}
