"use client";

import { useEffect, useState } from "react";

type World = {
  world_id: string;
  status: "initializing" | "running" | "extinct" | "archived";
  through_sequence: string | number;
  tick: string | number;
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

type RecordState =
  | { state: "loading" }
  | { state: "empty" }
  | { state: "error" }
  | { state: "ready"; world: World; timeline: TimelineItem[]; organisms: Organism[] };

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
        const [timelineResponse, organismsResponse] = await Promise.all([
          fetch(`/api/v1/worlds/${encoded}/timeline?limit=5`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${encoded}/organisms?limit=8`, { cache: "no-store" }),
        ]);
        if (!timelineResponse.ok || !organismsResponse.ok) throw new Error("world data unavailable");
        const timeline = (await timelineResponse.json()) as { items: TimelineItem[] };
        const organisms = (await organismsResponse.json()) as { organisms: Organism[] };
        if (active) setRecord({ state: "ready", world, timeline: timeline.items, organisms: organisms.organisms });
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

  if (record.state === "loading") return <section className="live-record loading">Reading committed observer records…</section>;
  if (record.state === "empty") return <section className="live-record empty">No world has been committed yet. Genesis will appear here from its first recorded event.</section>;
  if (record.state === "error") return <section className="live-record empty">Live records are temporarily unavailable. The static observatory remains available.</section>;

  const { world, timeline, organisms } = record;
  return (
    <section className="live-record" aria-labelledby="live-record-title">
      <div className="live-record-heading">
        <div>
          <p className="eyebrow">Committed observer record</p>
          <h2 id="live-record-title">World {world.world_id.slice(0, 8)} · {world.status}</h2>
        </div>
        <p>Through event {world.through_sequence} · tick {world.tick}</p>
      </div>
      <div className="live-record-grid">
        <article>
          <h3>Recent facts</h3>
          {timeline.length === 0 ? <p>No public events are available yet.</p> : <ol>{timeline.map((item) => <li key={item.source_event_id}><strong>{item.title}</strong><span>{item.summary} <em>Event {item.source_sequence}, tick {item.source_tick}</em></span></li>)}</ol>}
        </article>
        <article>
          <h3>Lives in record</h3>
          {organisms.length === 0 ? <p>No individual lives are available yet.</p> : <ul>{organisms.map((organism) => <li key={organism.organism_id}><span>{organism.role === "person" ? "Person" : "Animal"}</span><a href={organism.species.source_url} rel="noreferrer" target="_blank">{organism.species.scientific_name}</a><small>{organism.ended_event_id ? "record ended" : "present in record"}</small></li>)}</ul>}
        </article>
      </div>
    </section>
  );
}
