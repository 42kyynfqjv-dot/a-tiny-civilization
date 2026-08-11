"use client";

import { useEffect, useState } from "react";
import { WorldInputStatus, type WorldInputMetadata } from "./WorldInputStatus";

type World = WorldInputMetadata & {
  world_id: string;
  status: "initializing" | "running" | "extinct" | "archived" | "retired";
  through_sequence: string | number;
  tick: string | number;
  state_hash: string;
  predecessor_world_id: string | null;
};

type ArchiveState =
  | { state: "loading" }
  | { state: "empty" }
  | { state: "error" }
  | { state: "ready"; worlds: World[] };

/** A read-only catalogue of worlds whose canonical history is no longer advancing. */
export function ArchiveIndex() {
  const [archive, setArchive] = useState<ArchiveState>({ state: "loading" });

  useEffect(() => {
    let active = true;
    async function refresh() {
      try {
        const response = await fetch("/api/v1/worlds", { cache: "no-store" });
        if (!response.ok) throw new Error("world archive unavailable");
        const { worlds } = (await response.json()) as { worlds: World[] };
        const archived = worlds.filter((world) => world.status === "archived" || world.status === "extinct" || world.status === "retired");
        if (active) setArchive(archived.length === 0 ? { state: "empty" } : { state: "ready", worlds: archived });
      } catch {
        if (active) setArchive({ state: "error" });
      }
    }
    void refresh();
    const timer = window.setInterval(refresh, 30_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  if (archive.state === "loading") return <p className="archive-status">Reading immutable world records…</p>;
  if (archive.state === "empty") return <p className="archive-status">No world has reached its archive. If one does, its history remains here; a successor is a separate, explicit world.</p>;
  if (archive.state === "error") return <p className="archive-status">The archive index is temporarily unavailable. It never affects a world’s history.</p>;

  return (
    <ol className="archive-list">
      {archive.worlds.map((world) => (
        <li key={world.world_id}>
          <div>
            <p>World {world.world_id.slice(0, 8)}</p>
            <strong>{world.status === "retired" ? "Retired for a successor" : world.status === "archived" ? "Immutable archive" : "Extinction committed"}</strong>
            <WorldInputStatus world={world} />
          </div>
          <dl>
            <div><dt>Cursor</dt><dd>Event {world.through_sequence} · tick {world.tick}</dd></div>
            <div><dt>State hash</dt><dd title={world.state_hash}>{shortHash(world.state_hash)}</dd></div>
            <div><dt>Lineage</dt><dd>{world.predecessor_world_id ? `Successor of ${world.predecessor_world_id.slice(0, 8)}` : "No predecessor"}</dd></div>
          </dl>
        </li>
      ))}
    </ol>
  );
}

function shortHash(hash: string) {
  return `${hash.slice(0, 12)}…${hash.slice(-8)}`;
}
