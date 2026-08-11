"use client";

import { FormEvent, useEffect, useState } from "react";

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
};

type ResearchDuplicate = {
  request_id: string;
  ordinal: number;
  title: string;
  artifact_hash: string;
  result_hash: string;
  created_at: string;
};

type ResearchArtifact = {
  request_id: string;
  selected_at_tick: string | number;
  ordinal: number;
  target: string;
  task: string;
  inference_tier: string;
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
  created_at: string;
  duplicates: ResearchDuplicate[];
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
            fetch(`/api/v1/worlds/${id}/research?limit=120`, { cache: "no-store" }),
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
  return <main className="cancer-console">
    <header className="cancer-console-header">
      <div><span className={`cancer-console-pulse ${online ? "online" : "offline"}`} />CANCER WORLD / RESEARCH CONSOLE</div>
      <time>{record?.checkedAt ?? "CONNECTING"}</time>
    </header>

    <section className="cancer-console-intro">
      <div>
        <p className="cancer-console-kicker">LIVE EXPERIMENT · {humanize(research?.target ?? "adult_glioblastoma")}</p>
        <h1>The research record,<br />as it forms.</h1>
        <p>Every card below is a durable model contribution—not a validated treatment or medical advice. Earlier artifacts are mirrored into an isolated memory bank so later turns can retrieve and challenge them.</p>
      </div>
      <div className="cancer-console-state">
        <span>{telemetry ? "RUNNING" : "AWAITING GENESIS"}</span>
        <strong>{research?.distinct_artifacts ?? 0}</strong>
        <small>distinct research entries</small>
      </div>
    </section>

    <section className="cancer-console-grid" aria-label="Live experiment metrics">
      <Metric label="WORLD TICK" value={telemetry?.tick ?? "—"} />
      <Metric label="RESEARCH TURNS" value={research?.total_requests ?? 0} />
      <Metric label="DISTINCT WORK" value={research?.distinct_artifacts ?? 0} />
      <Metric label="DUPLICATES FLAGGED" value={research?.duplicate_artifacts ?? 0} />
      <Metric label="HINDSIGHT ACCEPTED" value={research?.memory_accepted ?? 0} />
      <Metric label="PEOPLE" value={telemetry?.living_people ?? "—"} />
    </section>

    <section className="cancer-console-section">
      <SectionHeading eyebrow="OUTPUT" title="Latest distinct research" detail="Newest activity first · repeated work is flagged and collapsed under its original" />
      <div className="cancer-artifact-list">
        {research?.artifacts.length ? research.artifacts.map((artifact) => <ArtifactCard key={artifact.request_id} artifact={artifact} />) : <EmptyState text="The first research contribution has not landed yet." />}
      </div>
    </section>

    <section className="cancer-console-section cancer-memory-lab">
      <SectionHeading eyebrow="HINDSIGHT" title="Search the internal research library" detail="The same isolated catalogue is supplied to new research turns to support cumulative work" />
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
    </section>

    <section className="cancer-console-section">
      <SectionHeading eyebrow="EVIDENCE" title="Literature mirror" detail={`${research?.evidence.length ?? 0} immutable CC BY / CC0 snapshots`} />
      <div className="cancer-evidence-list">
        {research?.evidence.length ? research.evidence.map((item) => <a href={item.source_id} key={item.evidence_id} rel="noreferrer" target="_blank">
          <span>{item.license.toUpperCase()} · {item.published_at ?? "DATE UNAVAILABLE"}</span>
          <strong>{item.title}</strong>
          <code>{shortHash(item.content_hash)}</code>
        </a>) : <EmptyState text="No licensed evidence snapshots are available yet." />}
      </div>
    </section>

    <section className="cancer-console-ledger">
      <Row label="WORLD" value={worldId || "pending"} />
      <Row label="SEQUENCE" value={String(telemetry?.through_sequence ?? "—")} />
      <Row label="EVENTS" value={String(telemetry?.committed_events ?? "—")} />
      <Row label="LAST COMMIT" value={telemetry?.last_committed_at ?? "—"} />
      <Row label="RUNNER" value={record?.status.latest_runner_heartbeat ?? "—"} />
      <Row label="HINDSIGHT WORKER" value={record?.status.latest_memory_worker_heartbeat ?? "—"} />
      <Row label="RESEARCH WORKER" value={record?.status.latest_cognition_worker_heartbeat ?? "—"} />
    </section>
  </main>;
}

function ArtifactCard({ artifact }: { artifact: ResearchArtifact }) {
  const contribution = artifact.contribution;
  return <article className="cancer-artifact-card">
    <div className="cancer-artifact-meta">
      <span>TURN {artifact.ordinal}</span>
      <span>{humanize(contribution.artifact_kind)}</span>
      <span>{humanize(contribution.stage)}</span>
      {artifact.duplicates.length > 0 && <span className="duplicate">{artifact.duplicates.length} DUPLICATE{artifact.duplicates.length === 1 ? "" : "S"} COLLAPSED</span>}
      <span className={artifact.memory_state === "accepted" ? "accepted" : "queued"}>{artifact.memory_state === "accepted" ? "MEMORY CONNECTED" : "MEMORY QUEUED"}</span>
    </div>
    <h3>{contribution.title}</h3>
    <p>{contribution.abstract_text}</p>
    <div className="cancer-artifact-chain">
      <span>{artifact.recalled_artifact_hashes.length ? `${artifact.recalled_artifact_hashes.length} earlier artifact${artifact.recalled_artifact_hashes.length === 1 ? "" : "s"} recalled` : "Independent starting point"}</span>
      <span>{artifact.evidence.length} evidence reference{artifact.evidence.length === 1 ? "" : "s"}</span>
      <span>{artifact.resolved_model}</span>
    </div>
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
