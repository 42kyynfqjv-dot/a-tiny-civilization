"use client";

import { useEffect, useMemo, useState } from "react";
import { createPublicLifeLabels } from "./lifeLabels";

type World = { world_id: string; status: "initializing" | "running" | "extinct" | "archived" };
type Organism = { organism_id: string; role: "person" | "fauna"; species: { scientific_name: string; source_url: string }; introduced_sequence: string | number; introduced_tick: string | number; ended_event_id: string | null };
type DirectoryState = { state: "loading" | "empty" | "error" } | { state: "ready"; world: World; organisms: Organism[] };

export function LifeDirectory() {
  const [directory, setDirectory] = useState<DirectoryState>({ state: "loading" });
  const [filter, setFilter] = useState<"all" | "person" | "fauna">("all");
  const [followedId, setFollowedId] = useState<string | null>(() =>
    typeof window === "undefined" ? null : window.localStorage.getItem("atiny.followed-organism"),
  );

  useEffect(() => {
    let active = true;
    async function load() {
      try {
        const worldsResponse = await fetch("/api/v1/worlds", { cache: "no-store" });
        if (!worldsResponse.ok) throw new Error();
        const { worlds } = (await worldsResponse.json()) as { worlds: World[] };
        const requested = new URLSearchParams(window.location.search).get("world");
        const world = worlds.find((item) => item.world_id === requested) ?? worlds.find((item) => item.status === "running") ?? worlds[0];
        if (!world) { if (active) setDirectory({ state: "empty" }); return; }
        const response = await fetch(`/api/v1/worlds/${encodeURIComponent(world.world_id)}/organisms?limit=200`, { cache: "no-store" });
        if (!response.ok) throw new Error();
        const { organisms } = (await response.json()) as { organisms: Organism[] };
        if (active) setDirectory({ state: "ready", world, organisms });
      } catch { if (active) setDirectory({ state: "error" }); }
    }
    void load();
    return () => { active = false; };
  }, []);

  const organisms = useMemo(() => directory.state === "ready" ? directory.organisms.filter((item) => filter === "all" || item.role === filter) : [], [directory, filter]);
  if (directory.state === "loading") return <section className="life-directory-status">Opening the life index…</section>;
  if (directory.state === "empty") return <section className="life-directory-status">No individual lives are public yet.</section>;
  if (directory.state === "error") return <section className="life-directory-status">The life index is briefly unavailable.</section>;
  const labels = createPublicLifeLabels(directory.organisms);

  return (
    <section className="life-directory" aria-labelledby="life-directory-title">
      <div className="life-directory-tools">
        <div><p className="eyebrow">Public life index</p><h2 id="life-directory-title">{directory.organisms.length} recorded lives</h2></div>
        <div role="group" aria-label="Filter lives">{(["all", "person", "fauna"] as const).map((value) => <button className={filter === value ? "active" : undefined} key={value} onClick={() => setFilter(value)} type="button">{value === "fauna" ? "Animals" : value === "person" ? "People" : "Everyone"}</button>)}</div>
      </div>
      <div className="life-directory-grid">
        {organisms.map((organism) => {
          const label = labels.get(organism.organism_id) ?? "Unindexed life";
          return <article className={followedId === organism.organism_id ? "followed" : undefined} key={organism.organism_id}>
            <a className="life-card-main" href={`/lives/${encodeURIComponent(directory.world.world_id)}/${encodeURIComponent(organism.organism_id)}`}>
              <span className="life-card-mark">{organism.role === "person" ? "P" : "A"}</span>
              <p>{organism.role === "person" ? "Person" : "Animal"} · {organism.ended_event_id ? "record ended" : "alive"}</p>
              <h3>{label}</h3><em>{organism.species.scientific_name}</em><small>In the record since moment {formatNumber(organism.introduced_tick)}</small>
            </a>
            <button type="button" onClick={() => { window.localStorage.setItem("atiny.followed-organism", organism.organism_id); setFollowedId(organism.organism_id); }}>{followedId === organism.organism_id ? "Following" : "Follow"}</button>
          </article>;
        })}
      </div>
    </section>
  );
}

function formatNumber(value: string | number) { return new Intl.NumberFormat("en-US").format(Number(value)); }
