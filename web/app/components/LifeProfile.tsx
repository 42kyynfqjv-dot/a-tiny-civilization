"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { createPublicLifeLabels, type LabelableOrganism } from "./lifeLabels";
import { commonSpeciesName } from "./speciesNames";

type Organism = { organism_id: string; world_id: string; role: "person" | "fauna"; species: { scientific_name: string; source_url: string }; provenance: "world_fact"; introduced_event_id: string; introduced_sequence: string | number; introduced_tick: string | number; ended_event_id: string | null; ended_sequence: string | number | null; ended_tick: string | number | null };
type TimelineItem = { source_event_id: string; source_sequence: string | number; source_tick: string | number; title: string; summary: string };
type ProfileState = { state: "loading" | "missing" | "error" } | { state: "ready"; organism: Organism; timeline: TimelineItem[]; organisms: LabelableOrganism[] };

export function LifeProfile({ worldId, organismId }: { worldId: string; organismId: string }) {
  const [profile, setProfile] = useState<ProfileState>({ state: "loading" });
  const [following, setFollowing] = useState(() =>
    typeof window !== "undefined" && window.localStorage.getItem("atiny.followed-organism") === organismId,
  );
  useEffect(() => {
    let active = true;
    async function load() {
      try {
        const [organismResponse, timelineResponse, organismsResponse] = await Promise.all([
          fetch(`/api/v1/worlds/${encodeURIComponent(worldId)}/organisms/${encodeURIComponent(organismId)}`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${encodeURIComponent(worldId)}/timeline?limit=12`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${encodeURIComponent(worldId)}/organisms?limit=200`, { cache: "no-store" }),
        ]);
        if (organismResponse.status === 404) { if (active) setProfile({ state: "missing" }); return; }
        if (!organismResponse.ok || !timelineResponse.ok || !organismsResponse.ok) throw new Error();
        const organism = (await organismResponse.json()) as Organism;
        const timeline = (await timelineResponse.json()) as { items: TimelineItem[] };
        const organisms = (await organismsResponse.json()) as { organisms: LabelableOrganism[] };
        if (active) setProfile({ state: "ready", organism, timeline: timeline.items, organisms: organisms.organisms });
      } catch { if (active) setProfile({ state: "error" }); }
    }
    void load();
    return () => { active = false; };
  }, [worldId, organismId]);

  if (profile.state === "loading") return <section className="life-profile-status">Reading this life’s public record…</section>;
  if (profile.state === "missing") return <section className="life-profile-status"><h1>This life is not in the public record.</h1><Link href="/lives">Return to all lives</Link></section>;
  if (profile.state === "error") return <section className="life-profile-status"><h1>This life’s record is briefly unavailable.</h1><Link href="/lives">Return to all lives</Link></section>;
  const { organism, timeline } = profile;
  const label = createPublicLifeLabels(profile.organisms).get(organism.organism_id) ?? "Unindexed life";
  return <>
    <section className="life-profile-hero">
      <div className="life-profile-copy"><p className="eyebrow">One life inside the world</p><h1>{label}</h1><p className="life-species"><span title={organism.species.scientific_name}>{commonSpeciesName(organism.species.scientific_name)}</span></p><p><strong>This is what we call them for now.</strong> They do not know this label or know that we are watching. If names ever emerge inside their world, their own name takes over.</p><div className="life-profile-actions"><button className={following ? "active" : undefined} type="button" onClick={() => { if (following) { window.localStorage.removeItem("atiny.followed-organism"); setFollowing(false); } else { window.localStorage.setItem("atiny.followed-organism", organism.organism_id); setFollowing(true); } }}>{following ? "Following this life" : "Follow this life"}</button><Link href={`/lives?world=${encodeURIComponent(worldId)}`}>Choose another</Link></div></div>
      <div className="life-profile-portrait" aria-hidden="true"><span>{label.match(/\d+$/)?.[0] ?? (organism.role === "person" ? "H" : "A")}</span><small>{label}</small></div>
    </section>
    <section className="life-facts"><article><span>Right now</span><strong>{organism.ended_event_id ? "Their story has ended" : "Alive"}</strong></article><article><span>Here since</span><strong>Moment {formatNumber(organism.introduced_tick)}</strong></article><article><span>Kind of life</span><strong>{organism.role === "person" ? "Human" : "Animal"}</strong></article><article><span>Species</span><strong>{commonSpeciesName(organism.species.scientific_name)}</strong></article></section>
    <section className="life-context"><div><p className="eyebrow">Around the same time</p><h2>The world kept moving around them.</h2><p>These moments happened in the wider world. They may not have seen them—and did not necessarily cause them.</p></div><ol>{timeline.map((item) => <li key={item.source_event_id}><time>Moment {formatNumber(item.source_tick)}</time><div><strong>{item.title}</strong><span>{item.summary}</span></div></li>)}</ol></section>
  </>;
}

function formatNumber(value: string | number) { return new Intl.NumberFormat("en-US").format(Number(value)); }
