"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { createPublicLifeLabels, type LabelableOrganism } from "./lifeLabels";

export type MemoryObservation = {
  document_id: string;
  agent_id: string;
  subject_id?: string;
  source_sequence: string | number;
  tick: string | number;
  channel: "vision" | "touch" | "sound" | "odour" | "taste" | "interoception";
  property_code: string;
  quantized_value: number;
  uncertainty: number;
};

export type MemoryRecall = {
  request_id: string;
  agent_id: string;
  selected_tick: string | number;
  deadline_tick: string | number;
  document_ids: string[];
};

export type MemoryStream = {
  world_id: string;
  observations: MemoryObservation[];
  recalls: MemoryRecall[];
};

type World = { world_id: string; status: "initializing" | "running" | "extinct" | "archived" };
type MemoryState =
  | { state: "loading" | "empty" | "error" }
  | { state: "ready"; world: World; stream: MemoryStream; organisms: LabelableOrganism[] };

export function MemoryIndex() {
  const [record, setRecord] = useState<MemoryState>({ state: "loading" });
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    async function refresh() {
      try {
        const worldsResponse = await fetch("/api/v1/worlds", { cache: "no-store" });
        if (!worldsResponse.ok) throw new Error();
        const { worlds } = (await worldsResponse.json()) as { worlds: World[] };
        const world = worlds.find((candidate) => candidate.status === "running") ?? worlds[0];
        if (!world) { if (active) setRecord({ state: "empty" }); return; }
        const id = encodeURIComponent(world.world_id);
        const [memoryResponse, organismsResponse] = await Promise.all([
          fetch(`/api/v1/worlds/${id}/memory?limit=240`, { cache: "no-store" }),
          fetch(`/api/v1/worlds/${id}/organisms?limit=500`, { cache: "no-store" }),
        ]);
        if (!memoryResponse.ok || !organismsResponse.ok) throw new Error();
        const stream = (await memoryResponse.json()) as MemoryStream;
        const organisms = (await organismsResponse.json()) as { organisms: LabelableOrganism[] };
        if (active) setRecord({ state: "ready", world, stream, organisms: organisms.organisms });
      } catch { if (active) setRecord({ state: "error" }); }
    }
    void refresh();
    const timer = window.setInterval(refresh, 5_000);
    return () => { active = false; window.clearInterval(timer); };
  }, []);

  if (record.state !== "ready") {
    const copy = record.state === "loading" ? "Opening the live memory record…" : record.state === "empty" ? "Memory begins when a world begins." : "The memory record is briefly out of reach.";
    return <section className="memory-status">{copy}</section>;
  }

  const labels = createPublicLifeLabels(record.organisms);
  const label = (id: string) => labels.get(id) ?? "Unindexed life";
  const subjectLabel = (id: string) => labels.get(id) ?? "an unindexed sensed subject";
  const agents = [...new Set(record.stream.observations.map((memory) => memory.agent_id))];
  const visible = selectedAgent ? record.stream.observations.filter((memory) => memory.agent_id === selectedAgent) : record.stream.observations;
  const recalls = selectedAgent ? record.stream.recalls.filter((recall) => recall.agent_id === selectedAgent) : record.stream.recalls;

  return <>
    <section className="memory-overview" aria-label="Memory activity at a glance">
      <article><span>Retained observations</span><strong>{formatNumber(record.stream.observations.length)}</strong><small>latest public window</small></article>
      <article><span>Lives remembering</span><strong>{formatNumber(agents.length)}</strong><small>private banks, observer view</small></article>
      <article><span>Recall pulses</span><strong>{formatNumber(record.stream.recalls.length)}</strong><small>entered recent cognition</small></article>
    </section>
    <section className="memory-workbench">
      <div className="memory-controls">
        <div><p className="eyebrow">Live connection map</p><h2>Experience becoming context.</h2><p>A line means an observation was retained or recalled. It does not mean the individual understood it, named it, or acted because of it.</p></div>
        <label>Follow one memory bank<select value={selectedAgent ?? ""} onChange={(event) => setSelectedAgent(event.target.value || null)}><option value="">All remembering lives</option>{agents.map((agent) => <option value={agent} key={agent}>{label(agent)}</option>)}</select></label>
      </div>
      <MemoryCanvas observations={visible} recalls={recalls} label={label} />
    </section>
    <section className="memory-ledger" aria-labelledby="memory-ledger-title">
      <header><p className="eyebrow">Latest retained experience</p><h2 id="memory-ledger-title">No invented inner monologue.</h2><p>These are label-free sensory records. Observer wording is only a readable rendering of the stored channel, property, value, and provenance.</p></header>
      <ol>{visible.slice(0, 36).map((memory) => <li key={memory.document_id}><time>Moment {formatNumber(memory.tick)}</time><div><strong>{label(memory.agent_id)} retained {channelLabel(memory.channel)}</strong><span>{observationSentence(memory, subjectLabel)}</span><small>Event {formatNumber(memory.source_sequence)} · uncertainty {formatNumber(memory.uncertainty)} · memory {memory.document_id.slice(0, 8)}</small></div></li>)}</ol>
    </section>
  </>;
}

function MemoryCanvas({ observations, recalls, label }: { observations: MemoryObservation[]; recalls: MemoryRecall[]; label: (id: string) => string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const recalled = useMemo(() => new Set(recalls.flatMap((recall) => recall.document_ids)), [recalls]);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const context = canvas.getContext("2d");
    if (!context) return;
    let frame = 0;
    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      const scale = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = Math.max(1, Math.round(rect.width * scale));
      canvas.height = Math.max(1, Math.round(rect.height * scale));
      context.setTransform(scale, 0, 0, scale, 0, 0);
    };
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);
    const draw = (now: number) => {
      drawMemoryGraph(context, canvas.clientWidth, canvas.clientHeight, observations.slice(0, 90), recalled, label, now);
      frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
    return () => { observer.disconnect(); cancelAnimationFrame(frame); };
  }, [label, observations, recalled]);
  return <canvas className="memory-canvas" ref={canvasRef} role="img" aria-label="Live graph connecting lives to retained sensory observations and recalled memories" />;
}

function drawMemoryGraph(context: CanvasRenderingContext2D, width: number, height: number, observations: MemoryObservation[], recalled: Set<string>, label: (id: string) => string, now: number) {
  context.clearRect(0, 0, width, height);
  const backdrop = context.createRadialGradient(width * .5, height * .45, 10, width * .5, height * .45, Math.max(width, height) * .7);
  backdrop.addColorStop(0, "#112c36"); backdrop.addColorStop(1, "#03070d"); context.fillStyle = backdrop; context.fillRect(0, 0, width, height);
  const agents = [...new Set(observations.map((memory) => memory.agent_id))];
  const agentPoint = new Map(agents.map((id, index) => [id, radialPoint(index, agents.length, width * .5, height * .5, Math.min(width, height) * .31)]));
  for (const memory of observations) {
    const agent = agentPoint.get(memory.agent_id); if (!agent) continue;
    const angle = hashUnit(memory.document_id) * Math.PI * 2;
    const distance = 34 + hashUnit(memory.document_id.slice(8)) * 105;
    const point = { x: agent.x + Math.cos(angle) * distance, y: agent.y + Math.sin(angle) * distance };
    const pulse = recalled.has(memory.document_id) ? 3 + Math.sin(now / 220 + angle) * 2 : 0;
    context.beginPath(); context.moveTo(agent.x, agent.y); context.lineTo(point.x, point.y); context.strokeStyle = recalled.has(memory.document_id) ? "rgba(114,226,194,.55)" : "rgba(137,177,185,.15)"; context.lineWidth = recalled.has(memory.document_id) ? 1.4 : .6; context.stroke();
    context.beginPath(); context.arc(point.x, point.y, 2.1 + pulse, 0, Math.PI * 2); context.fillStyle = recalled.has(memory.document_id) ? "rgba(130,240,207,.78)" : channelColor(memory.channel); context.fill();
  }
  for (const [id, point] of agentPoint) {
    context.beginPath(); context.arc(point.x, point.y, 8, 0, Math.PI * 2); context.fillStyle = "#ef8258"; context.shadowColor = "#ef8258"; context.shadowBlur = 16; context.fill(); context.shadowBlur = 0;
    context.fillStyle = "rgba(235,244,241,.82)"; context.font = "9px ui-monospace, monospace"; context.textAlign = "center"; context.fillText(label(id), point.x, point.y + 24);
  }
}

function radialPoint(index: number, total: number, x: number, y: number, radius: number) { const angle = (index / Math.max(1, total)) * Math.PI * 2 - Math.PI / 2; return { x: x + Math.cos(angle) * radius, y: y + Math.sin(angle) * radius }; }
function hashUnit(value: string) { let hash = 2166136261; for (const char of value) hash = Math.imul(hash ^ char.charCodeAt(0), 16777619); return (hash >>> 0) / 4294967295; }
function channelColor(channel: MemoryObservation["channel"]) { return ({ vision: "#8cc8e5", touch: "#e2bd72", sound: "#aa9ee8", odour: "#8bcf9d", taste: "#e89584", interoception: "#d6d5ce" } as const)[channel]; }
function channelLabel(channel: MemoryObservation["channel"]) { return channel === "interoception" ? "a bodily sensation" : channel === "odour" ? "a scent" : channel === "sound" ? "a sound" : channel === "vision" ? "a sight" : channel === "taste" ? "a taste" : "a touch"; }
function observationSentence(memory: MemoryObservation, label: (id: string) => string) { const subject = memory.subject_id ? label(memory.subject_id) : "their own body or surroundings"; return `${subject} · ${memory.property_code.replaceAll("_", " ")} registered ${formatNumber(memory.quantized_value)}.`; }
function formatNumber(value: string | number) { return new Intl.NumberFormat("en-US").format(Number(value)); }
