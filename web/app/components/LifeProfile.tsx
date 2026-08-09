"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { createPublicLifeLabels, type LabelableOrganism } from "./lifeLabels";

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
      <div className="life-profile-copy"><p className="eyebrow">Individual public record · {organism.role === "person" ? "person" : "animal"}</p><h1>{label}</h1><p className="life-species"><a href={organism.species.source_url} target="_blank" rel="noreferrer">{organism.species.scientific_name}</a></p><p><strong>{label} is a numerical observer ID, not a name.</strong> It identifies this life until the inhabitants independently develop naming. If that happens, their own name becomes the primary display while this ID remains the permanent audit reference.</p><div className="life-profile-actions"><button className={following ? "active" : undefined} type="button" onClick={() => { if (following) { window.localStorage.removeItem("atiny.followed-organism"); setFollowing(false); } else { window.localStorage.setItem("atiny.followed-organism", organism.organism_id); setFollowing(true); } }}>{following ? "Following this life" : "Follow this life"}</button><Link href={`/lives?world=${encodeURIComponent(worldId)}`}>Choose another</Link></div></div>
      <div className="life-profile-portrait" aria-hidden="true"><span>{organism.role === "person" ? "P" : "A"}</span><small>{organism.organism_id.slice(0, 8)}</small></div>
    </section>
    <section className="life-facts"><article><span>Status</span><strong>{organism.ended_event_id ? "Record ended" : "Alive"}</strong></article><article><span>First public moment</span><strong>{formatNumber(organism.introduced_tick)}</strong></article><article><span>First event</span><strong>{formatNumber(organism.introduced_sequence)}</strong></article><article><span>Provenance</span><strong>World fact</strong></article></section>
    <section className="life-context"><div><p className="eyebrow">The record around this life</p><h2>A life happens inside a world.</h2><p>These are recent world-level moments, not claims that this individual witnessed or caused them. Individual activity appears here only when a public projection can support that connection.</p></div><ol>{timeline.map((item) => <li key={item.source_event_id}><time>Moment {formatNumber(item.source_tick)}</time><div><strong>{item.title}</strong><span>{item.summary}</span></div></li>)}</ol></section>
  </>;
}

function formatNumber(value: string | number) { return new Intl.NumberFormat("en-US").format(Number(value)); }
