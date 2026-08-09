"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { commonSpeciesName } from "./speciesNames";
import { MemoryTelemetry } from "./MemoryTelemetry";

type Detail = "planet" | "region" | "local";
type Role = "person" | "fauna";
type Action = "move" | "orient" | "reach" | "grasp" | "release" | "apply_force" | "chew" | "swallow" | "rest" | "emit_signal";
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
type Camera = { latitude: number; longitude: number };
type DragState = {
  pointerId: number;
  startX: number;
  startY: number;
  lastX: number;
  lastY: number;
  lastAt: number;
  startCamera: Camera;
  velocity: Camera;
  moved: boolean;
};
type PointerPosition = { x: number; y: number };
type PinchState = {
  startDistance: number;
  startZoom: number;
  startCamera: Camera;
  anchor: Camera;
  scale: number;
};

const WORLD_BOUNDS: Bounds = { west: -1_799_999_999, south: -900_000_000, east: 1_799_999_999, north: 900_000_000 };

export function HabitatStage({ worldId, worldTick, labels }: { worldId: string; worldTick: string | number; labels: Map<string, string> }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const pointsRef = useRef<Point[]>([]);
  const viewRef = useRef<HabitatView | null>(null);
  const dragRef = useRef<DragState | null>(null);
  const pointersRef = useRef(new Map<number, PointerPosition>());
  const pinchRef = useRef<PinchState | null>(null);
  const cameraRef = useRef<Camera>({ latitude: 0, longitude: 0 });
  const zoomRef = useRef(1);
  const detailRef = useRef<Detail>("local");
  const inertiaFrameRef = useRef<number | null>(null);
  const wheelCommitRef = useRef<number | null>(null);
  const [view, setView] = useState<HabitatView | null>(null);
  const [detail, setDetail] = useState<Detail>("local");
  const [center, setCenter] = useState<Camera>({ latitude: 0, longitude: 0 });
  const [localZoom, setLocalZoom] = useState(1);
  const [selectedId, setSelectedId] = useState<string | null>(() => typeof window === "undefined" ? null : window.localStorage.getItem("atiny.followed-organism"));
  const [status, setStatus] = useState<"loading" | "live" | "error">("loading");
  const [interacting, setInteracting] = useState(false);

  useEffect(() => { cameraRef.current = center; }, [center]);
  useEffect(() => { zoomRef.current = localZoom; }, [localZoom]);
  useEffect(() => { detailRef.current = detail; }, [detail]);
  useEffect(() => () => {
    if (inertiaFrameRef.current !== null) cancelAnimationFrame(inertiaFrameRef.current);
    if (wheelCommitRef.current !== null) window.clearTimeout(wheelCommitRef.current);
  }, []);

  const fetchView = useCallback(async (nextDetail: Detail, nextCenter: Camera, nextZoom: number) => {
    const bounds = boundsFor(nextDetail, nextCenter, nextZoom);
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
        const planet = await fetchView("planet", { latitude: 0, longitude: 0 }, 1);
        const focus = [...planet.clusters].sort((a, b) => b.total - a.total)[0];
        let nextCenter = focus ? { latitude: focus.latitude_e7, longitude: focus.longitude_e7 } : { latitude: 0, longitude: 0 };
        if (!active) return;
        const overview = await fetchView("local", nextCenter, 1);
        const fitted = fitLocalCamera(overview.entities, nextCenter);
        nextCenter = fitted.center;
        const local = fitted.zoom > 1.05 ? await fetchView("local", nextCenter, fitted.zoom) : overview;
        if (!active) return;
        cameraRef.current = nextCenter;
        zoomRef.current = fitted.zoom;
        detailRef.current = "local";
        setCenter(nextCenter);
        setLocalZoom(fitted.zoom);
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
        const next = await fetchView(detail, center, localZoom);
        if (!active) return;
        viewRef.current = next;
        setView(next);
        setStatus("live");
      } catch { if (active) setStatus("error"); }
    }
    void refresh();
    const timer = window.setInterval(refresh, 3_000);
    return () => { active = false; window.clearInterval(timer); };
  }, [center, detail, fetchView, localZoom, status]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const context = canvas.getContext("2d");
    if (!context) return;
    let frame = 0;
    let start = performance.now();
    let width = 1;
    let height = 1;
    const resize = new ResizeObserver(() => {
      const rect = canvas.getBoundingClientRect();
      width = Math.max(1, rect.width);
      height = Math.max(1, rect.height);
      const requestedScale = Math.min(window.devicePixelRatio || 1, 3);
      const pixelBudgetScale = Math.sqrt(14_000_000 / Math.max(1, width * height));
      const scale = Math.max(1, Math.min(requestedScale, pixelBudgetScale));
      canvas.width = Math.max(1, Math.round(width * scale));
      canvas.height = Math.max(1, Math.round(height * scale));
      context.setTransform(scale, 0, 0, scale, 0, 0);
      context.imageSmoothingEnabled = true;
      context.imageSmoothingQuality = "high";
      start = performance.now();
    });
    resize.observe(canvas);
    function draw(now: number) {
      pointsRef.current = drawHabitat(context, width, height, viewRef.current, detailRef.current, cameraRef.current, zoomRef.current, Math.min(1, (now - start) / 2_400), selectedId, labels);
      frame = requestAnimationFrame(draw);
    }
    frame = requestAnimationFrame(draw);
    return () => { cancelAnimationFrame(frame); resize.disconnect(); };
  }, [labels, selectedId]);

  const selected = view?.entities.find((entity) => entity.organism_id === selectedId);
  const activity = useMemo(() => view?.activity.slice(0, 8) ?? [], [view]);
  const chooseDetail = async (next: Detail, focus = center, nextZoom = next === "local" ? localZoom : 1) => {
    detailRef.current = next;
    cameraRef.current = focus;
    zoomRef.current = nextZoom;
    setDetail(next);
    setCenter(focus);
    setLocalZoom(nextZoom);
    setStatus("loading");
    try {
      const nextView = await fetchView(next, focus, nextZoom);
      viewRef.current = nextView;
      setView(nextView);
      setStatus("live");
    } catch { setStatus("error"); }
  };
  const selectPoint = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    const hitSlop = event.pointerType === "touch" ? 16 : 7;
    const point = [...pointsRef.current].reverse().find((candidate) => Math.hypot(candidate.x - x, candidate.y - y) <= candidate.radius + hitSlop);
    if (point?.entity) setSelectedId(point.entity.organism_id);
    if (point?.cluster) {
      const nextCenter = { latitude: point.cluster.latitude_e7, longitude: point.cluster.longitude_e7 };
      setCenter(nextCenter);
      void chooseDetail(detail === "planet" ? "region" : "local", nextCenter);
    }
  };
  const beginPointer = (event: React.PointerEvent<HTMLCanvasElement>) => {
    if (inertiaFrameRef.current !== null) cancelAnimationFrame(inertiaFrameRef.current);
    if (wheelCommitRef.current !== null) window.clearTimeout(wheelCommitRef.current);
    inertiaFrameRef.current = null;
    wheelCommitRef.current = null;
    pointersRef.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    event.currentTarget.setPointerCapture(event.pointerId);
    if (pointersRef.current.size >= 2) {
      const [first, second] = [...pointersRef.current.values()].slice(0, 2);
      const middle = midpoint(first, second);
      const rect = event.currentTarget.getBoundingClientRect();
      pinchRef.current = {
        startDistance: Math.max(1, pointDistance(first, second)),
        startZoom: zoomRef.current,
        startCamera: cameraRef.current,
        anchor: worldAtScreen(detailRef.current, cameraRef.current, zoomRef.current, middle, rect),
        scale: 1,
      };
      dragRef.current = null;
      setInteracting(true);
      return;
    }
    const now = performance.now();
    dragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      lastX: event.clientX,
      lastY: event.clientY,
      lastAt: now,
      startCamera: cameraRef.current,
      velocity: { latitude: 0, longitude: 0 },
      moved: false,
    };
    setInteracting(true);
  };
  const movePointer = (event: React.PointerEvent<HTMLCanvasElement>) => {
    if (!pointersRef.current.has(event.pointerId)) return;
    pointersRef.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    const pinch = pinchRef.current;
    if (pinch && pointersRef.current.size >= 2) {
      const [first, second] = [...pointersRef.current.values()].slice(0, 2);
      const middle = midpoint(first, second);
      pinch.scale = pointDistance(first, second) / pinch.startDistance;
      if (detailRef.current === "local") {
        const nextZoom = Math.max(1, Math.min(64, pinch.startZoom * pinch.scale));
        const rect = event.currentTarget.getBoundingClientRect();
        const worldBelowFingers = worldAtScreen("local", pinch.startCamera, nextZoom, middle, rect);
        cameraRef.current = clampCamera({
          longitude: pinch.startCamera.longitude + pinch.anchor.longitude - worldBelowFingers.longitude,
          latitude: pinch.startCamera.latitude + pinch.anchor.latitude - worldBelowFingers.latitude,
        });
        zoomRef.current = nextZoom;
      }
      return;
    }
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const bounds = boundsFor(detailRef.current, drag.startCamera, zoomRef.current);
    const next = clampCamera({
      longitude: drag.startCamera.longitude - (event.clientX - drag.startX) / Math.max(1, rect.width) * (bounds.east - bounds.west),
      latitude: drag.startCamera.latitude + (event.clientY - drag.startY) / Math.max(1, rect.height) * (bounds.north - bounds.south),
    });
    const now = performance.now();
    const elapsed = Math.max(1, now - drag.lastAt);
    const instantaneous = {
      longitude: (next.longitude - cameraRef.current.longitude) / elapsed,
      latitude: (next.latitude - cameraRef.current.latitude) / elapsed,
    };
    drag.velocity = {
      longitude: drag.velocity.longitude * .58 + instantaneous.longitude * .42,
      latitude: drag.velocity.latitude * .58 + instantaneous.latitude * .42,
    };
    drag.lastX = event.clientX;
    drag.lastY = event.clientY;
    drag.lastAt = now;
    drag.moved ||= Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) >= 4;
    cameraRef.current = next;
  };
  const endPointer = (event: React.PointerEvent<HTMLCanvasElement>) => {
    pointersRef.current.delete(event.pointerId);
    const pinch = pinchRef.current;
    if (pinch) {
      if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
      pinchRef.current = null;
      if (detailRef.current === "local") {
        if (pinch.scale < .88 && pinch.startZoom <= 1.05) {
          void chooseDetail("region", pinch.anchor, 1);
        } else {
          setCenter(cameraRef.current);
          setLocalZoom(zoomRef.current);
        }
      } else if (pinch.scale > 1.12) {
        const next = detailRef.current === "planet" ? "region" : "local";
        void chooseDetail(next, pinch.anchor, 1);
      } else if (pinch.scale < .88 && detailRef.current === "region") {
        void chooseDetail("planet", pinch.anchor, 1);
      }
      const remaining = [...pointersRef.current.entries()][0];
      if (remaining) {
        const [pointerId, position] = remaining;
        dragRef.current = {
          pointerId,
          startX: position.x,
          startY: position.y,
          lastX: position.x,
          lastY: position.y,
          lastAt: performance.now(),
          startCamera: cameraRef.current,
          velocity: { latitude: 0, longitude: 0 },
          moved: true,
        };
      } else {
        dragRef.current = null;
        setInteracting(false);
      }
      return;
    }
    const drag = dragRef.current;
    dragRef.current = null;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (!drag.moved) {
      setInteracting(false);
      selectPoint(event);
      return;
    }
    const span = boundsFor(detailRef.current, cameraRef.current, zoomRef.current);
    const maximumVelocity = Math.max(1, span.east - span.west) * .001;
    let velocity = {
      longitude: Math.max(-maximumVelocity, Math.min(maximumVelocity, drag.velocity.longitude)),
      latitude: Math.max(-maximumVelocity, Math.min(maximumVelocity, drag.velocity.latitude)),
    };
    let prior = performance.now();
    const coast = (now: number) => {
      const elapsed = Math.min(32, now - prior);
      prior = now;
      cameraRef.current = clampCamera({
        longitude: cameraRef.current.longitude + velocity.longitude * elapsed,
        latitude: cameraRef.current.latitude + velocity.latitude * elapsed,
      });
      const decay = Math.exp(-elapsed / 145);
      velocity = { longitude: velocity.longitude * decay, latitude: velocity.latitude * decay };
      if (Math.hypot(velocity.longitude, velocity.latitude) > maximumVelocity * .012) {
        inertiaFrameRef.current = requestAnimationFrame(coast);
      } else {
        inertiaFrameRef.current = null;
        setCenter(cameraRef.current);
        setInteracting(false);
      }
    };
    inertiaFrameRef.current = requestAnimationFrame(coast);
  };
  const cancelPointer = (event: React.PointerEvent<HTMLCanvasElement>) => {
    pointersRef.current.delete(event.pointerId);
    pinchRef.current = null;
    dragRef.current = null;
    setCenter(cameraRef.current);
    setLocalZoom(zoomRef.current);
    setInteracting(false);
  };
  const zoomBy = (direction: 1 | -1) => {
    if (direction > 0 && detail === "planet") { void chooseDetail("region", center, 1); return; }
    if (direction > 0 && detail === "region") { void chooseDetail("local", center, 1); return; }
    if (direction < 0 && detail === "region") { void chooseDetail("planet", center, 1); return; }
    if (direction < 0 && detail === "local" && localZoom <= 1.05) { void chooseDetail("region", center, 1); return; }
    if (detail === "local") {
      const nextZoom = Math.max(1, Math.min(64, localZoom * (direction > 0 ? 1.65 : 1 / 1.65)));
      void chooseDetail("local", center, nextZoom);
    }
  };
  const followSelected = () => {
    if (!selectedId) return;
    window.localStorage.setItem("atiny.followed-organism", selectedId);
    setSelectedId(selectedId);
  };

  const wheelZoom = (event: React.WheelEvent<HTMLCanvasElement>) => {
    event.preventDefault();
    if (detailRef.current !== "local") {
      zoomBy(event.deltaY < 0 ? 1 : -1);
      return;
    }
    if (event.deltaY > 0 && zoomRef.current <= 1.02) {
      void chooseDetail("region", cameraRef.current, 1);
      return;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    const x = Math.max(0, Math.min(1, (event.clientX - rect.left) / Math.max(1, rect.width)));
    const y = Math.max(0, Math.min(1, (event.clientY - rect.top) / Math.max(1, rect.height)));
    const before = boundsFor("local", cameraRef.current, zoomRef.current);
    const longitude = before.west + x * (before.east - before.west);
    const latitude = before.north - y * (before.north - before.south);
    const nextZoom = Math.max(1, Math.min(64, zoomRef.current * Math.exp(-event.deltaY * .0018)));
    const after = boundsFor("local", cameraRef.current, nextZoom);
    cameraRef.current = clampCamera({
      longitude: cameraRef.current.longitude + longitude - (after.west + x * (after.east - after.west)),
      latitude: cameraRef.current.latitude + latitude - (after.north - y * (after.north - after.south)),
    });
    zoomRef.current = nextZoom;
    setInteracting(true);
    if (wheelCommitRef.current !== null) window.clearTimeout(wheelCommitRef.current);
    wheelCommitRef.current = window.setTimeout(() => {
      wheelCommitRef.current = null;
      setCenter(cameraRef.current);
      setLocalZoom(zoomRef.current);
      setInteracting(false);
    }, 120);
  };

  return <section className={`habitat-stage ${interacting ? "is-interacting" : ""}`} aria-label="Deep-space observatory window onto the live habitat">
    <div className="habitat-starfield" aria-hidden="true"><i /><i /><i /></div>
    <div className="habitat-window">
    <canvas ref={canvasRef} onPointerDown={beginPointer} onPointerMove={movePointer} onPointerUp={endPointer} onPointerCancel={cancelPointer} onWheel={wheelZoom} aria-label="Live positions of inhabitants and animals. Drag to pan, pinch or scroll to zoom, and select a point to inspect it." />
    <div className="habitat-wash" aria-hidden="true" />
    <div className="habitat-glass" aria-hidden="true"><i /><span className="glass-corner corner-nw" /><span className="glass-corner corner-ne" /><span className="glass-corner corner-sw" /><span className="glass-corner corner-se" /></div>
    <header className="habitat-toolbar">
      <div><span className={`habitat-status ${status}`} /> <strong>{status === "live" ? "Live habitat" : status === "loading" ? "Locating life" : "Reconnecting"}</strong><small>Moment {formatNumber(worldTick)}</small></div>
      <nav aria-label="Habitat detail">
        {(["planet", "region", "local"] as Detail[]).map((item) => <button type="button" className={detail === item ? "active" : undefined} onClick={() => void chooseDetail(item)} key={item}>{item}</button>)}
        <button type="button" aria-label="Zoom out" onClick={() => zoomBy(-1)}>−</button><button type="button" aria-label="Zoom in" onClick={() => zoomBy(1)}>+</button>
      </nav>
    </header>
    <div className="habitat-key"><span className="person" /> People <span className="animal" /> Animals <i /> committed movement</div>
    <aside className="habitat-activity" aria-live="polite">
      <p>Happening now</p>
      {activity.length === 0 ? <span>The habitat is quiet.</span> : <ol>{activity.map((item) => <li key={item.source_event_id}><time>{formatNumber(item.source_tick)}</time><span>{activitySentence(item, labels)}</span></li>)}</ol>}
    </aside>
    <MemoryTelemetry worldId={worldId} labels={labels} />
    <div className={`habitat-selection ${selected ? "has-selection" : "is-hint"}`}>
      {selected ? <><p>{labels.get(selected.organism_id) ?? shortId(selected.organism_id)} · {selected.role === "person" ? "person" : "animal"}</p><strong title={selected.species.scientific_name}>{commonSpeciesName(selected.species.scientific_name)}</strong><span>{actionSentence(selected.last_action, selected.signal_form)}</span><div><button type="button" onClick={followSelected}>Follow this life</button><a href={`/lives/${encodeURIComponent(worldId)}/${encodeURIComponent(selected.organism_id)}`}>Open record</a></div></> : <><p>Look closely</p><strong>Select any moving point</strong><span>Drag to pan; pinch or scroll to zoom. Nearby markers fan apart visually so each committed life remains selectable.</span></>}
    </div>
    <footer><span>Positions are committed · orbital glass and terrain are observer styling · drag / pinch / scroll</span><span>{detail === "local" ? `${localZoom.toFixed(localZoom < 10 ? 1 : 0)}× · ` : ""}{view?.truncated ? `view capped at ${formatNumber(view.maximum_entities)} lives` : detail === "local" ? `${formatNumber(view?.entities.length ?? 0)} lives in view` : `${formatNumber(view?.clusters.length ?? 0)} population clusters`}</span></footer>
    </div>
    <div className="habitat-observatory-frame" aria-hidden="true"><span>ATC · DEEP-SPACE OBSERVATORY</span><i /><i /></div>
  </section>;
}

function drawHabitat(context: CanvasRenderingContext2D, width: number, height: number, view: HabitatView | null, detail: Detail, center: Camera, localZoom: number, progress: number, selectedId: string | null, labels: Map<string, string>): Point[] {
  context.clearRect(0, 0, width, height);
  const gradient = context.createLinearGradient(0, 0, width, height);
  gradient.addColorStop(0, "#0f3126"); gradient.addColorStop(.52, "#173c2b"); gradient.addColorStop(1, "#071d18");
  context.fillStyle = gradient; context.fillRect(0, 0, width, height);
  drawTerrain(context, width, height, center, localZoom, detail);
  if (!view) return [];
  const bounds = boundsFor(detail, center, localZoom);
  const terrainPhaseX = center.longitude / 1_800_000;
  const terrainPhaseY = center.latitude / 2_400_000;
  const project = (longitude: number, latitude: number) => {
    const horizontal = (longitude - bounds.west) / Math.max(1, bounds.east - bounds.west);
    const depth = 1 - (latitude - bounds.south) / Math.max(1, bounds.north - bounds.south);
    if (detail !== "local") return { x: horizontal * width, y: depth * height };
    const clampedDepth = Math.max(-.08, Math.min(1.08, depth));
    const perspectiveWidth = .44 + Math.max(0, clampedDepth) * .72;
    return {
      x: width / 2 + (horizontal - .5) * width * perspectiveWidth,
      y: height * .16 + Math.pow(Math.max(0, clampedDepth), 1.28) * height * .84 - terrainRelief(horizontal, clampedDepth, terrainPhaseX, terrainPhaseY, localZoom),
    };
  };
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
  const positioned = separateOverlaps(view.entities.map((entity) => {
    const from = project(entity.previous_longitude_e7, entity.previous_latitude_e7);
    const to = project(entity.longitude_e7, entity.latitude_e7);
    return { entity, from, to, anchorX: from.x + (to.x - from.x) * ease(progress), anchorY: from.y + (to.y - from.y) * ease(progress) };
  }), selectedId);
  const pulse = Math.sin(Date.now() / 350) * 2;
  const depthOrdered = positioned.sort((a, b) => a.y - b.y || Number(a.entity.organism_id === selectedId) - Number(b.entity.organism_id === selectedId));
  for (const marker of depthOrdered) {
    const { entity, from, to, anchorX, anchorY, x, y } = marker;
    if (Math.hypot(to.x - from.x, to.y - from.y) > 1) {
      context.beginPath(); context.moveTo(from.x, from.y); context.lineTo(to.x, to.y); context.strokeStyle = entity.role === "person" ? "rgba(236,132,89,.34)" : "rgba(229,202,113,.23)"; context.lineWidth = 1; context.stroke();
    }
    if (Math.hypot(x - anchorX, y - anchorY) > 2) {
      context.beginPath(); context.moveTo(anchorX, anchorY); context.lineTo(x, y); context.strokeStyle = "rgba(223,231,211,.18)"; context.lineWidth = .7; context.stroke();
    }
    const selected = entity.organism_id === selectedId;
    const depth = Math.max(0, Math.min(1, (y - height * .16) / Math.max(1, height * .84)));
    const depthScale = .3 + depth * 1.02;
    const radius = (entity.role === "person" ? 6.1 : 4.2) * depthScale;
    const lift = 3 + depth * 6;
    const orbY = y - lift;
    context.save();
    context.globalAlpha = selected ? 1 : .42 + depth * .58;
    context.beginPath(); context.ellipse(x + depthScale * 1.5, y + 1, radius * 1.35, radius * .38, 0, 0, Math.PI * 2); context.fillStyle = "rgba(0,7,5,.38)"; context.fill();
    context.beginPath(); context.moveTo(x, y); context.lineTo(x, orbY + radius * .5); context.strokeStyle = entity.role === "person" ? "rgba(239,130,88,.36)" : "rgba(216,189,104,.28)"; context.lineWidth = Math.max(.55, depthScale * .8); context.stroke();
    if (entity.last_action === "emit_signal") {
      context.beginPath(); context.ellipse(x, orbY, radius + 7 + pulse, (radius + 7 + pulse) * .48, 0, 0, Math.PI * 2); context.strokeStyle = "rgba(121,210,180,.42)"; context.stroke();
    }
    if (selected) { context.beginPath(); context.ellipse(x, orbY, radius + 8, (radius + 8) * .56, 0, 0, Math.PI * 2); context.strokeStyle = "#fff1bd"; context.lineWidth = 1.5; context.stroke(); }
    const color = entity.role === "person" ? "#ef8258" : "#d8bd68";
    const sphere = context.createRadialGradient(x - radius * .35, orbY - radius * .45, .2, x, orbY, radius * 1.35);
    sphere.addColorStop(0, "#fff3cf"); sphere.addColorStop(.24, color); sphere.addColorStop(1, entity.role === "person" ? "#7f2f24" : "#695623");
    context.beginPath(); context.arc(x, orbY, radius, 0, Math.PI * 2); context.fillStyle = sphere; context.shadowColor = color; context.shadowBlur = selected ? 18 : 7 * depthScale; context.fill(); context.shadowBlur = 0;
    if (selected) { context.fillStyle = "#fff7df"; context.font = "10px ui-monospace, monospace"; context.textAlign = "left"; context.fillText(labels.get(entity.organism_id) ?? shortId(entity.organism_id), x + radius + 9, orbY + 4); }
    context.restore();
    points.push({ id: entity.organism_id, x, y: orbY, radius, entity });
  }
  return points;
}

type PositionedEntity = { entity: HabitatEntity; from: { x: number; y: number }; to: { x: number; y: number }; anchorX: number; anchorY: number; x: number; y: number };

function separateOverlaps(items: Omit<PositionedEntity, "x" | "y">[], selectedId: string | null): PositionedEntity[] {
  const cellSize = 12;
  const occupied = new Map<string, { x: number; y: number }[]>();
  const ordered = [...items].sort((a, b) => Number(a.entity.organism_id === selectedId) - Number(b.entity.organism_id === selectedId));
  const result: PositionedEntity[] = [];
  const nearby = (x: number, y: number) => {
    const cellX = Math.floor(x / cellSize); const cellY = Math.floor(y / cellSize);
    for (let offsetX = -1; offsetX <= 1; offsetX++) for (let offsetY = -1; offsetY <= 1; offsetY++) {
      for (const point of occupied.get(`${cellX + offsetX}:${cellY + offsetY}`) ?? []) if (Math.hypot(point.x - x, point.y - y) < 10) return true;
    }
    return false;
  };
  for (const item of ordered) {
    let x = item.anchorX; let y = item.anchorY;
    for (let attempt = 0; attempt < 96 && nearby(x, y); attempt++) {
      const radius = 5 + Math.sqrt(attempt + 1) * 6.5;
      const angle = attempt * 2.3999632297;
      x = item.anchorX + Math.cos(angle) * radius;
      y = item.anchorY + Math.sin(angle) * radius;
    }
    const key = `${Math.floor(x / cellSize)}:${Math.floor(y / cellSize)}`;
    const bucket = occupied.get(key) ?? [];
    bucket.push({ x, y }); occupied.set(key, bucket);
    result.push({ ...item, x, y });
  }
  return result;
}

function drawTerrain(context: CanvasRenderingContext2D, width: number, height: number, center: Camera, localZoom: number, detail: Detail) {
  context.save();
  const phaseX = center.longitude / 1_800_000; const phaseY = center.latitude / 2_400_000;
  if (detail !== "local") {
    const overview = context.createRadialGradient(width * .48, height * .46, 0, width * .48, height * .46, Math.max(width, height) * .78);
    overview.addColorStop(0, "#1b4a36"); overview.addColorStop(.55, "#103427"); overview.addColorStop(1, "#051712");
    context.fillStyle = overview; context.fillRect(0, 0, width, height);
    context.strokeStyle = "rgba(146,188,161,.12)"; context.lineWidth = .55;
    for (let line = 0; line < 28; line++) {
      context.beginPath();
      for (let x = -20; x <= width + 20; x += 14) {
        const y = height * (line / 27) + Math.sin(x * .011 + line * .51 + phaseX) * 12 + Math.sin(x * .027 - phaseY) * 4;
        if (x === -20) context.moveTo(x, y); else context.lineTo(x, y);
      }
      context.stroke();
    }
    context.restore();
    return;
  }
  const horizon = height * .16;
  const atmosphere = context.createLinearGradient(0, 0, 0, height);
  atmosphere.addColorStop(0, "#041613"); atmosphere.addColorStop(.16, "#1c4b3a"); atmosphere.addColorStop(.42, "#173d2d"); atmosphere.addColorStop(1, "#071b15");
  context.fillStyle = atmosphere; context.fillRect(0, 0, width, height);
  const glow = context.createRadialGradient(width * .46, horizon * .55, 0, width * .46, horizon * .55, width * .62);
  glow.addColorStop(0, "rgba(168,204,177,.17)"); glow.addColorStop(.42, "rgba(80,139,106,.06)"); glow.addColorStop(1, "rgba(0,0,0,0)");
  context.fillStyle = glow; context.fillRect(0, 0, width, height * .62);

  for (let layer = 0; layer < 3; layer++) {
    context.beginPath(); context.moveTo(0, horizon + layer * 10);
    for (let x = 0; x <= width + 16; x += 16) {
      const y = horizon + layer * 11 - Math.sin(x * (.004 + layer * .0017) + phaseX * .5 + layer) * (14 - layer * 3) - Math.sin(x * .013 - phaseY) * (5 + layer);
      context.lineTo(x, y);
    }
    context.lineTo(width, horizon + 70); context.lineTo(0, horizon + 70); context.closePath();
    context.fillStyle = layer === 0 ? "rgba(22,61,45,.74)" : layer === 1 ? "rgba(18,55,40,.72)" : "rgba(15,48,35,.68)"; context.fill();
  }

  const rows = 18;
  const columns = 24;
  const mesh = Array.from({ length: rows + 1 }, (_, row) => {
    const depth = row / rows;
    const perspectiveWidth = .44 + depth * .72;
    return Array.from({ length: columns + 1 }, (_, column) => {
      const horizontal = column / columns;
      const relief = terrainRelief(horizontal, depth, phaseX, phaseY, localZoom);
      return {
        x: width / 2 + (horizontal - .5) * width * perspectiveWidth,
        y: horizon + Math.pow(depth, 1.28) * (height - horizon) - relief,
        relief,
      };
    });
  });
  for (let row = 0; row < rows; row++) {
    const depth = (row + .5) / rows;
    for (let column = 0; column < columns; column++) {
      const nearLeft = mesh[row + 1][column];
      const nearRight = mesh[row + 1][column + 1];
      const farRight = mesh[row][column + 1];
      const farLeft = mesh[row][column];
      const slopeLight = Math.max(-10, Math.min(14, (nearLeft.relief - nearRight.relief) * .42));
      const red = Math.round(21 - depth * 8 + slopeLight * .28);
      const green = Math.round(70 - depth * 18 + slopeLight);
      const blue = Math.round(49 - depth * 13 + slopeLight * .52);
      context.beginPath(); context.moveTo(farLeft.x, farLeft.y); context.lineTo(farRight.x, farRight.y); context.lineTo(nearRight.x, nearRight.y); context.lineTo(nearLeft.x, nearLeft.y); context.closePath();
      context.fillStyle = `rgba(${red},${green},${blue},${.72 + depth * .2})`; context.fill();
    }
  }
  context.strokeStyle = "rgba(151,190,166,.13)";
  for (let row = 0; row <= rows; row++) {
    context.beginPath();
    for (let column = 0; column <= columns; column++) {
      const point = mesh[row][column];
      if (column === 0) context.moveTo(point.x, point.y); else context.lineTo(point.x, point.y);
    }
    context.lineWidth = .35 + row / rows * .45; context.stroke();
  }
  context.strokeStyle = "rgba(125,172,145,.055)"; context.lineWidth = .45;
  for (let column = 0; column <= columns; column += 2) {
    context.beginPath();
    for (let row = 0; row <= rows; row++) {
      const point = mesh[row][column];
      if (row === 0) context.moveTo(point.x, point.y); else context.lineTo(point.x, point.y);
    }
    context.stroke();
  }

  const water = context.createLinearGradient(width * .2, 0, width * .8, 0); water.addColorStop(0, "rgba(76,151,135,0)"); water.addColorStop(.5, "rgba(90,166,148,.27)"); water.addColorStop(1, "rgba(76,151,135,0)");
  context.strokeStyle = water; context.lineWidth = 10 + Math.min(18, localZoom * .45); context.beginPath(); context.moveTo(width * .43, horizon); context.bezierCurveTo(width * .36, height * .42, width * .66, height * .64, width * .56, height * 1.04); context.stroke();
  const foreground = context.createLinearGradient(0, height * .72, 0, height); foreground.addColorStop(0, "rgba(3,17,13,0)"); foreground.addColorStop(1, "rgba(1,8,6,.54)"); context.fillStyle = foreground; context.fillRect(0, height * .7, width, height * .3);
  context.restore();
}

function terrainRelief(horizontal: number, depth: number, phaseX: number, phaseY: number, localZoom: number) {
  const zoomTexture = .9 + Math.min(20, localZoom) * .006;
  return (Math.sin(horizontal * 14 + phaseX + depth * 10.2) * (1.5 + depth * 15)
    + Math.sin(horizontal * 37 - phaseY - depth * 6.1) * (1 + depth * 5.5)) * zoomTexture;
}

function boundsFor(detail: Detail, center: Camera, localZoom = 1): Bounds {
  if (detail === "planet") return WORLD_BOUNDS;
  const span = detail === "region" ? 100_000_000 : 6_000_000 / Math.max(1, localZoom);
  return {
    west: Math.max(WORLD_BOUNDS.west, Math.round(center.longitude - span)),
    east: Math.min(WORLD_BOUNDS.east, Math.round(center.longitude + span)),
    south: Math.max(WORLD_BOUNDS.south, Math.round(center.latitude - span)),
    north: Math.min(WORLD_BOUNDS.north, Math.round(center.latitude + span)),
  };
}

function fitLocalCamera(entities: HabitatEntity[], fallback: Camera): { center: Camera; zoom: number } {
  if (entities.length === 0) return { center: fallback, zoom: 1 };
  const longitudes = entities.flatMap((entity) => [entity.longitude_e7, entity.previous_longitude_e7]);
  const latitudes = entities.flatMap((entity) => [entity.latitude_e7, entity.previous_latitude_e7]);
  const west = Math.min(...longitudes); const east = Math.max(...longitudes);
  const south = Math.min(...latitudes); const north = Math.max(...latitudes);
  const range = Math.max(90_000, east - west, north - south);
  return {
    center: { longitude: (west + east) / 2, latitude: (south + north) / 2 },
    zoom: Math.max(1, Math.min(64, 12_000_000 / (range * 1.5))),
  };
}

function clampCamera(camera: Camera): Camera {
  return {
    longitude: Math.max(WORLD_BOUNDS.west, Math.min(WORLD_BOUNDS.east, camera.longitude)),
    latitude: Math.max(WORLD_BOUNDS.south, Math.min(WORLD_BOUNDS.north, camera.latitude)),
  };
}

function midpoint(first: PointerPosition, second: PointerPosition): PointerPosition {
  return { x: (first.x + second.x) / 2, y: (first.y + second.y) / 2 };
}

function pointDistance(first: PointerPosition, second: PointerPosition) {
  return Math.hypot(second.x - first.x, second.y - first.y);
}

function worldAtScreen(detail: Detail, camera: Camera, zoom: number, point: PointerPosition, rect: DOMRect): Camera {
  const bounds = boundsFor(detail, camera, zoom);
  const screenX = Math.max(0, Math.min(1, (point.x - rect.left) / Math.max(1, rect.width)));
  const screenY = Math.max(0, Math.min(1, (point.y - rect.top) / Math.max(1, rect.height)));
  if (detail !== "local") {
    return {
      longitude: bounds.west + screenX * (bounds.east - bounds.west),
      latitude: bounds.north - screenY * (bounds.north - bounds.south),
    };
  }
  const depth = Math.pow(Math.max(0, (screenY - .16) / .84), 1 / 1.28);
  const perspectiveWidth = .44 + Math.max(0, depth) * .72;
  const horizontal = .5 + (screenX - .5) / perspectiveWidth;
  return {
    longitude: bounds.west + horizontal * (bounds.east - bounds.west),
    latitude: bounds.north - depth * (bounds.north - bounds.south),
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
