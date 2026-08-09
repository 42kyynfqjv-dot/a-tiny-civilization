"use client";

import { useEffect, useState } from "react";
import type { MemoryStream } from "./MemoryIndex";

export function MemoryTelemetry({ worldId, labels }: { worldId: string; labels: Map<string, string> }) {
  const [stream, setStream] = useState<MemoryStream | null>(null);
  useEffect(() => {
    let active = true;
    async function refresh() {
      try {
        const response = await fetch(`/api/v1/worlds/${encodeURIComponent(worldId)}/memory?limit=8`, { cache: "no-store" });
        if (!response.ok) return;
        const next = (await response.json()) as MemoryStream;
        if (active) setStream(next);
      } catch { /* The habitat remains usable when observer telemetry is unavailable. */ }
    }
    void refresh();
    const timer = window.setInterval(refresh, 5_000);
    return () => { active = false; window.clearInterval(timer); };
  }, [worldId]);

  const latest = stream?.observations.slice(0, 3) ?? [];
  const recalled = new Set(stream?.recalls.flatMap((recall) => recall.document_ids) ?? []);
  return <aside className="habitat-memory" aria-live="polite" aria-label="Live memory stream">
    <div><span className="habitat-memory-pulse" aria-hidden="true" /><p><strong>Something remembered</strong><small>{stream ? `${stream.recalls.length} recent ${stream.recalls.length === 1 ? "memory" : "memories"} resurfaced` : "listening"}</small></p><a href="/memory"><span className="habitat-memory-open-long">See memories </span><span className="habitat-memory-open-short">Explore </span><span aria-hidden="true">↗</span></a></div>
    {latest.length > 0 && <ol>{latest.map((memory) => <li className={recalled.has(memory.document_id) ? "is-recalled" : undefined} key={memory.document_id}><span>{labels.get(memory.agent_id) ?? "A life"}</span><small>{memory.channel === "interoception" ? "felt something within" : memory.channel === "odour" ? "noticed a scent" : memory.channel === "vision" ? "noticed something nearby" : memory.channel === "touch" ? "felt a surface" : memory.channel === "sound" ? "heard something" : "noticed a taste"}</small></li>)}</ol>}
  </aside>;
}
