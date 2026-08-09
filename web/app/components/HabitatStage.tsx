"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

type Detail = "planet" | "region" | "local";
type Role = "person" | "fauna";
type Action = "move" | "orient" | "reach" | "grasp" | "release" | "apply_force" | "bite" | "chew" | "swallow" | "rest" | "emit_signal";
type HabitatEntity = {
  organism_id: string;
  role: Role;
  species: { scientific_name: string };
  latitude_e7: number;
  longitude_e7: number;
  previous_latitude_e7: number;
  previous_longitude_e7: number;
  last_movement_tick: string | number;
  last_action?: Action;
  signal_form?: number;
  alive: boolean;
};
type HabitatCluster = { cluster_key: string; latitude_e7: number; longitude_e7: number; people: number; animals: number; total: number };
type HabitatActivity = { source_event_id: string; source_sequence: string | number; source_tick: string | number; organism_id: string; action: Action; signal_form?: number };
type HabitatView = { through_sequence: string | number; detail: Detail; entities: HabitatEntity[]; clusters: HabitatCluster[]; activity: HabitatActivity[]; truncated: boolean; maximum_entities: number };
type Point = { id: string; x: number; y: number; radius: number; entity?: HabitatEntity; cluster?: HabitatCluster };
type Bounds = { west: number; south: number; east: number; north: number };

const WORLD_BOUNDS: Bounds = { west: -1_799_999_999, south: -900_000_000, east: 1_799_999_999, north: 900_000_000 };

export function HabitatStage({ worldId, worldTick, labels }: { worldId: string; worldTick: string | number; labels: Map<string, string> }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const pointsRef = useRef<Point[]>([]);
  const viewRef = useRef<HabitatView | null>(null);
  const [view, setView] = useState<HabitatView | null>(null);
  const [detail, setDetail] = useState<Detail>("local");
  const [center, setCenter] = useState({ latitude: 0, longitude: 0 });
  const [selectedId, setSelectedId] = useState<string | null>(() => typeof window === "undefined" ? null : window.localStorage.getItem("atiny.followed-organism"));
  const [status, setStatus] = useState<"loading" | "live" | "error">("loading");

  const fetchView = useCallback(async (nextDetail: Detail, nextCenter: { latitude: number; longitude: number }) => {
    const bounds = boundsFor(nextDetail, nextCenter);
    const params = new URLSearchParams({
      detail: nextDetail,
      west_e7: String(bounds.west), south_e7: String(bounds.south), east_e7: String(bounds.east), north_e7: String(bounds.north),
      cell_e7: String(nextDetail === "planet" ? 100_000_000 : nextDetail === "region" ? 2_500_000 : 100_000),
      limit: "2000", activity_limit: "24",
    });
    const response = await fetch(`/api/v1/worlds/${encodeURIComponent(worldId)}/habitat?${params}`, { cache: "no-store" });
    if (!response.ok) throw new Error("habitat unavailable");
    return await response.json() as HabitatView;
  }, [worldId]);

  useEffect(() => {
    let active = true;
    async function bootstrap() {
      try {
        const planet = await fetchView("planet", { latitude: 0, longitude: 0 });
        const focus = [...planet.clusters].sort((a, b) => b.total - a.total)[0];
        const nextCenter = focus ? { latitude: focus.latitude_e7, longitude: focus.longitude_e7 } : { latitude: 0, longitude: 0 };
        if (!active) return;
        setCenter(nextCenter);
        const local = await fetchView("local", nextCenter);
        if (!active) return;
        viewRef.current = local;
        setView(local);
        setStatus("live");
      } catch { if (active) setStatus("error"); }
    }
    void bootstrap();
    return () => { active = false; };
  }, [fetchView]);

  useEffect(() => {
    if (status === "loading") return;
    let active = true;
    async function refresh() {
      try {
        const next = await fetchView(detail, center);
        if (!active) return;
        viewRef.current = next;
        setView(next);
        setStatus("live");
      } catch { if (active) setStatus("error"); }
    }
    void refresh();
    const timer = window.setInterval(refresh, 3_000);
    return () => { active = false; window.clearInterval(timer); };
  }, [center, detail, fetchView, status]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const context = canvas.getContext("2d");
    if (!context) return;
    let frame = 0;
    let start = performance.now();
    const resize = new ResizeObserver(() => {
      const rect = canvas.getBoundingClientRect();
      const scale = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = Math.max(1, Math.round(rect.width * scale));
      canvas.height = Math.max(1, Math.round(rect.height * scale));
      context.setTransform(scale, 0, 0, scale, 0, 0);
      start = performance.now();
    });
    resize.observe(canvas);
    function draw(now: number) {
      const rect = canvas.getBoundingClientRect();
      pointsRef.current = drawHabitat(context, rect.width, rect.height, viewRef.current, detail, center, Math.min(1, (now - start) / 2_400), selectedId, labels);
      frame = requestAnimationFrame(draw);
    }
    frame = requestAnimationFrame(draw);
    return () => { cancelAnimationFrame(frame); resize.disconnect(); };
  }, [center, detail, labels, selectedId]);

  const selected = view?.entities.find((entity) => entity.organism_id === selectedId);
  const activity = useMemo(() => view?.activity.slice(0, 8) ?? [], [view]);
  const chooseDetail = async (next: Detail, focus = center) => {
    setDetail(next);
    setStatus("loading");
    try {
      const nextView = await fetchView(next, focus);
      viewRef.current = nextView;
      setView(nextView);
      setStatus("live");
    } catch { setStatus("error"); }
  };
  const selectPoint = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    const point = [...pointsRef.current].reverse().find((candidate) => Math.hypot(candidate.x - x, candidate.y - y) <= candidate.radius + 7);
    if (point?.entity) setSelectedId(point.entity.organism_id);
    if (point?.cluster) {
      const nextCenter = { latitude: point.cluster.latitude_e7, longitude: point.cluster.longitude_e7 };
      setCenter(nextCenter);
      void chooseDetail(detail === "planet" ? "region" : "local", nextCenter);
    }
  };
  const followSelected = () => {
    if (!selectedId) return;
    window.localStorage.setItem("atiny.followed-organism", selectedId);
    setSelectedId(selectedId);
  };

  return <section className="habitat-stage" aria-label="Live habitat view">
    <canvas ref={canvasRef} onPointerUp={selectPoint} aria-label="Live positions of inhabitants and animals. Select a point to inspect it." />
    <div className="habitat-wash" aria-hidden="true" />
    <header className="habitat-toolbar">
      <div><span className={`habitat-status ${status}`} /> <strong>{status === "live" ? "Live habitat" : status === "loading" ? "Locating life" : "Reconnecting"}</strong><small>Moment {formatNumber(worldTick)}</small></div>
      <nav aria-label="Habitat detail">
        {(["planet", "region", "local"] as Detail[]).map((item) => <button type="button" className={detail === item ? "active" : undefined} onClick={() => void chooseDetail(item)} key={item}>{item}</button>)}
      </nav>
    </header>
    <div className="habitat-key"><span className="person" /> People <span className="animal" /> Animals <i /> committed movement</div>
    <aside className="habitat-activity" aria-live="polite">
      <p>Happening now</p>
      {activity.length === 0 ? <span>The habitat is quiet.</span> : <ol>{activity.map((item) => <li key={item.source_event_id}><time>{formatNumber(item.source_tick)}</time><span>{activitySentence(item, labels)}</span></li>)}</ol>}
    </aside>
    <div className="habitat-selection">
      {selected ? <><p>{labels.get(selected.organism_id) ?? shortId(selected.organism_id)} · {selected.role === "person" ? "person" : "animal"}</p><strong>{selected.species.scientific_name}</strong><span>{actionSentence(selected.last_action, selected.signal_form)}</span><div><button type="button" onClick={followSelected}>Follow this life</button><a href={`/lives/${encodeURIComponent(worldId)}/${encodeURIComponent(selected.organism_id)}`}>Open record</a></div></> : <><p>Look closely</p><strong>Select any moving point</strong><span>Every point is one committed life. Cluster circles replace individuals as the population grows.</span></>}
    </div>
    <footer><span>Actual committed positions · motion interpolated between ticks</span><span>{view?.truncated ? `View capped at ${formatNumber(view.maximum_entities)} lives` : detail === "local" ? `${formatNumber(view?.entities.length ?? 0)} lives in view` : `${formatNumber(view?.clusters.length ?? 0)} population clusters`}</span></footer>
  </section>;
}

function drawHabitat(context: CanvasRenderingContext2D, width: number, height: number, view: HabitatView | null, detail: Detail, center: { latitude: number; longitude: number }, progress: number, selectedId: string | null, labels: Map<string, string>): Point[] {
  context.clearRect(0, 0, width, height);
  const gradient = context.createLinearGradient(0, 0, width, height);
  gradient.addColorStop(0, "#0f3126"); gradient.addColorStop(.52, "#173c2b"); gradient.addColorStop(1, "#071d18");
  context.fillStyle = gradient; context.fillRect(0, 0, width, height);
  drawTerrain(context, width, height);
  if (!view) return [];
  const bounds = visibleBounds(view, detail, center);
  const project = (longitude: number, latitude: number) => ({
    x: ((longitude - bounds.west) / Math.max(1, bounds.east - bounds.west)) * width,
    y: height - ((latitude - bounds.south) / Math.max(1, bounds.north - bounds.south)) * height,
  });
  const points: Point[] = [];
  if (detail !== "local") {
    for (const cluster of view.clusters) {
      const point = project(cluster.longitude_e7, cluster.latitude_e7);
      const radius = Math.max(9, Math.min(48, 7 + Math.sqrt(cluster.total) * 3.5));
      context.beginPath(); context.arc(point.x, point.y, radius + 8, 0, Math.PI * 2); context.fillStyle = "rgba(221,178,92,.10)"; context.fill();
      context.beginPath(); context.arc(point.x, point.y, radius, 0, Math.PI * 2); context.fillStyle = cluster.people > 0 ? "rgba(232,126,83,.86)" : "rgba(208,190,104,.76)"; context.fill();
      context.fillStyle = "#fff8e6"; context.font = "600 11px ui-monospace, monospace"; context.textAlign = "center"; context.textBaseline = "middle"; context.fillText(formatNumber(cluster.total), point.x, point.y);
      points.push({ id: cluster.cluster_key, ...point, radius, cluster });
    }
    return points;
  }
  for (const entity of view.entities) {
    const from = project(entity.previous_longitude_e7, entity.previous_latitude_e7);
    const to = project(entity.longitude_e7, entity.latitude_e7);
    const x = from.x + (to.x - from.x) * ease(progress);
    const y = from.y + (to.y - from.y) * ease(progress);
    if (Math.hypot(to.x - from.x, to.y - from.y) > 1) {
      context.beginPath(); context.moveTo(from.x, from.y); context.lineTo(to.x, to.y); context.strokeStyle = entity.role === "person" ? "rgba(236,132,89,.34)" : "rgba(229,202,113,.23)"; context.lineWidth = 1; context.stroke();
    }
    const selected = entity.organism_id === selectedId;
    const radius = entity.role === "person" ? 5.5 : 3.8;
    if (entity.last_action === "emit_signal") {
      context.beginPath(); context.arc(x, y, radius + 7 + Math.sin(Date.now() / 350) * 2, 0, Math.PI * 2); context.strokeStyle = "rgba(121,210,180,.42)"; context.stroke();
    }
    if (selected) { context.beginPath(); context.arc(x, y, radius + 8, 0, Math.PI * 2); context.strokeStyle = "#fff1bd"; context.lineWidth = 1.5; context.stroke(); }
    context.beginPath(); context.arc(x, y, radius, 0, Math.PI * 2); context.fillStyle = entity.role === "person" ? "#ef8258" : "#d8bd68"; context.shadowColor = context.fillStyle; context.shadowBlur = selected ? 14 : 5; context.fill(); context.shadowBlur = 0;
    if (selected) { context.fillStyle = "#fff7df"; context.font = "10px ui-monospace, monospace"; context.textAlign = "left"; context.fillText(labels.get(entity.organism_id) ?? shortId(entity.organism_id), x + 13, y + 4); }
    points.push({ id: entity.organism_id, x, y, radius, entity });
  }
  return points;
}

function drawTerrain(context: CanvasRenderingContext2D, width: number, height: number) {
  context.save(); context.globalAlpha = .28; context.strokeStyle = "#8eb39a"; context.lineWidth = .55;
  for (let line = 0; line < 18; line++) {
    context.beginPath();
    for (let x = -20; x <= width + 20; x += 16) {
      const y = height * (.08 + line / 19) + Math.sin(x * .012 + line * .73) * 16 + Math.sin(x * .032 - line) * 5;
      if (x === -20) context.moveTo(x, y); else context.lineTo(x, y);
    }
    context.stroke();
  }
  const water = context.createLinearGradient(width * .1, 0, width * .9, 0); water.addColorStop(0, "rgba(73,137,123,0)"); water.addColorStop(.5, "rgba(73,137,123,.28)"); water.addColorStop(1, "rgba(73,137,123,0)");
  context.strokeStyle = water; context.lineWidth = 18; context.beginPath(); context.moveTo(-20, height * .78); context.bezierCurveTo(width * .24, height * .52, width * .63, height * .88, width + 20, height * .58); context.stroke();
  context.restore();
}

function visibleBounds(view: HabitatView, detail: Detail, center: { latitude: number; longitude: number }): Bounds {
  if (detail === "planet") return WORLD_BOUNDS;
  if (detail === "region") return boundsFor(detail, center);
  const coordinates = view.entities.flatMap((entity) => [[entity.longitude_e7, entity.latitude_e7], [entity.previous_longitude_e7, entity.previous_latitude_e7]]);
  if (coordinates.length === 0) return boundsFor(detail, center);
  const west = Math.min(...coordinates.map(([longitude]) => longitude));
  const east = Math.max(...coordinates.map(([longitude]) => longitude));
  const south = Math.min(...coordinates.map(([, latitude]) => latitude));
  const north = Math.max(...coordinates.map(([, latitude]) => latitude));
  const longitudePad = Math.max(45_000, (east - west) * .18);
  const latitudePad = Math.max(45_000, (north - south) * .18);
  return { west: west - longitudePad, east: east + longitudePad, south: south - latitudePad, north: north + latitudePad };
}

function boundsFor(detail: Detail, center: { latitude: number; longitude: number }): Bounds {
  if (detail === "planet") return WORLD_BOUNDS;
  const span = detail === "region" ? 100_000_000 : 6_000_000;
  return {
    west: Math.max(WORLD_BOUNDS.west, Math.round(center.longitude - span)),
    east: Math.min(WORLD_BOUNDS.east, Math.round(center.longitude + span)),
    south: Math.max(WORLD_BOUNDS.south, Math.round(center.latitude - span)),
    north: Math.min(WORLD_BOUNDS.north, Math.round(center.latitude + span)),
  };
}

function activitySentence(item: HabitatActivity, labels: Map<string, string>) { return `${labels.get(item.organism_id) ?? shortId(item.organism_id)} ${actionSentence(item.action, item.signal_form)}`; }
function actionSentence(action?: Action, signalForm?: number) {
  switch (action) {
    case "move": return "crossed into a neighboring patch";
    case "orient": return "turned toward its surroundings";
    case "reach": return "reached into the space nearby";
    case "grasp": return "closed its grasp around something";
    case "release": return "released what it held";
    case "apply_force": return "pressed against a material surface";
    case "bite": return "bit into something nearby";
    case "chew": return "continued chewing";
    case "swallow": return "swallowed material";
    case "rest": return "settled into rest";
    case "emit_signal": return `emitted signal form ${signalForm ?? "—"}`;
    default: return "is present in the habitat";
  }
}
function ease(value: number) { return 1 - Math.pow(1 - value, 3); }
function shortId(value: string) { return value.slice(0, 8); }
function formatNumber(value: string | number) { return new Intl.NumberFormat("en-US").format(Number(value)); }
