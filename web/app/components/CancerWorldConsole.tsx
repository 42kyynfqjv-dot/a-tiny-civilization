"use client";

import { useEffect, useState } from "react";

type FoundationStatus = {
  database_time: string;
  worlds: { initializing: number; running: number; archived: number };
  latest_runner_heartbeat: string | null;
  latest_projector_heartbeat: string | null;
  latest_memory_worker_heartbeat: string | null;
  latest_cognition_worker_heartbeat: string | null;
};

type Telemetry = {
  world_id: string;
  through_sequence: string | number;
  tick: string | number;
  committed_batches: string | number;
  committed_events: string | number;
  last_committed_at: string;
  timeline_lag_batches: string | number;
  organism_index_lag_batches: string | number;
  findings_lag_batches: string | number;
  telemetry_lag_batches: string | number;
  artifacts_lag_batches: string | number;
  living_people: string | number;
  living_fauna: string | number;
};

type MemoryStream = { observations: unknown[]; recalls: unknown[] };
type ArtifactResponse = { artifacts: unknown[] };
type ConsoleRecord = {
  status: FoundationStatus;
  telemetry: Telemetry | null;
  memories: number;
  recalls: number;
  artifacts: number;
  checkedAt: string;
};

export function CancerWorldConsole({ worldId }: { worldId: string }) {
  const [record, setRecord] = useState<ConsoleRecord | null>(null);
  const [online, setOnline] = useState(true);

  useEffect(() => {
    let active = true;
    async function refresh() {
      try {
        const statusResponse = await fetch("/api/v1/status", { cache: "no-store" });
        if (!statusResponse.ok) throw new Error("status");
        const status = (await statusResponse.json()) as FoundationStatus;
        let telemetry: Telemetry | null = null;
        let memories = 0;
        let recalls = 0;
        let artifacts = 0;
        if (worldId) {
          const id = encodeURIComponent(worldId);
          const [telemetryResponse, memoryResponse, artifactResponse] = await Promise.all([
            fetch(`/api/v1/worlds/${id}/telemetry`, { cache: "no-store" }),
            fetch(`/api/v1/worlds/${id}/memory?limit=500`, { cache: "no-store" }),
            fetch(`/api/v1/worlds/${id}/artifacts?limit=500`, { cache: "no-store" }),
          ]);
          if (telemetryResponse.ok) telemetry = (await telemetryResponse.json()) as Telemetry;
          if (memoryResponse.ok) {
            const memory = (await memoryResponse.json()) as MemoryStream;
            memories = memory.observations.length;
            recalls = memory.recalls.length;
          }
          if (artifactResponse.ok) artifacts = ((await artifactResponse.json()) as ArtifactResponse).artifacts.length;
        }
        if (active) {
          setRecord({ status, telemetry, memories, recalls, artifacts, checkedAt: new Date().toISOString() });
          setOnline(true);
        }
      } catch {
        if (active) setOnline(false);
      }
    }
    void refresh();
    const timer = window.setInterval(refresh, 5_000);
    return () => { active = false; window.clearInterval(timer); };
  }, [worldId]);

  const telemetry = record?.telemetry;
  return <main className="cancer-console">
    <header><div><span className={`cancer-console-pulse ${online ? "online" : "offline"}`} />CANCER WORLD</div><time>{record?.checkedAt ?? "CONNECTING"}</time></header>
    <section className="cancer-console-grid">
      <Metric label="STATE" value={!worldId ? "NOT ASSIGNED" : telemetry ? "RUNNING" : "AWAITING GENESIS"} />
      <Metric label="TICK" value={telemetry?.tick ?? "—"} />
      <Metric label="SEQUENCE" value={telemetry?.through_sequence ?? "—"} />
      <Metric label="PEOPLE" value={telemetry?.living_people ?? "—"} />
      <Metric label="EVENTS" value={telemetry?.committed_events ?? "—"} />
      <Metric label="MEMORIES" value={record?.memories ?? 0} />
      <Metric label="RECALLS" value={record?.recalls ?? 0} />
      <Metric label="ARTIFACTS" value={record?.artifacts ?? 0} />
    </section>
    <section className="cancer-console-ledger">
      <Row label="WORLD" value={worldId || "pending"} />
      <Row label="LAST COMMIT" value={telemetry?.last_committed_at ?? "—"} />
      <Row label="RUNNER" value={record?.status.latest_runner_heartbeat ?? "—"} />
      <Row label="PROJECTOR" value={record?.status.latest_projector_heartbeat ?? "—"} />
      <Row label="HINDSIGHT" value={record?.status.latest_memory_worker_heartbeat ?? "—"} />
      <Row label="COGNITION" value={record?.status.latest_cognition_worker_heartbeat ?? "—"} />
      <Row label="PROJECTION LAG" value={telemetry ? [telemetry.timeline_lag_batches, telemetry.organism_index_lag_batches, telemetry.findings_lag_batches, telemetry.telemetry_lag_batches, telemetry.artifacts_lag_batches].join(" / ") : "—"} />
    </section>
  </main>;
}

function Metric({ label, value }: { label: string; value: string | number }) {
  return <article><span>{label}</span><strong>{String(value)}</strong></article>;
}

function Row({ label, value }: { label: string; value: string }) {
  return <div><dt>{label}</dt><dd>{value}</dd></div>;
}
