"use client";

import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import { commonSpeciesName } from "./speciesNames";

type Species = {
  catalog: string;
  identifier: string;
  scientific_name: string;
  source_url: string;
};

type PublicWorld = {
  world_id: string;
  status: "initializing" | "running" | "extinct" | "archived" | "retired";
};

type PublicOrganism = { role: "person" | "fauna"; species: Species };

type Reservation = {
  reservation_id: string;
  observer_label: string;
  target: { type: "person" } | { type: "animal"; data: { species: Species } };
  birth_category: string;
  state: string;
  refund_state: string | null;
};

type AuthProvider = "google" | "apple";

type PanelState =
  | { kind: "checking" }
  | { kind: "unavailable" }
  | { kind: "signed_out"; providers: AuthProvider[] }
  | {
      kind: "ready";
      world: PublicWorld;
      animalSpecies: Species[];
      reservations: Reservation[];
    };

export function SupporterPanel() {
  const [panel, setPanel] = useState<PanelState>({ kind: "checking" });
  const [label, setLabel] = useState("");
  const [targetKind, setTargetKind] = useState<"person" | "animal">("person");
  const [birthCategory, setBirthCategory] = useState("female");
  const [speciesIdentifier, setSpeciesIdentifier] = useState("");
  const [message, setMessage] = useState("");
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    void loadPanel(controller.signal).then((next) => {
      if (!controller.signal.aborted) setPanel(next);
    });
    return () => controller.abort();
  }, []);

  const selectedSpecies = useMemo(() => {
    if (panel.kind !== "ready") return undefined;
    return (
      panel.animalSpecies.find((species) => species.identifier === speciesIdentifier) ??
      panel.animalSpecies[0]
    );
  }, [panel, speciesIdentifier]);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (panel.kind !== "ready" || submitting) return;
    if (targetKind === "animal" && !selectedSpecies) {
      setMessage("No individually recorded animal species is eligible yet.");
      return;
    }
    const csrf = csrfToken();
    if (!csrf) {
      setMessage("Your supporter session expired. Please sign in again.");
      return;
    }
    setSubmitting(true);
    setMessage("");
    try {
      const response = await fetch("/api/v1/supporters/checkout", {
        method: "POST",
        headers: { "content-type": "application/json", "x-csrf-token": csrf },
        body: JSON.stringify({
          reservation_id: crypto.randomUUID(),
          world_id: panel.world.world_id,
          observer_label: label,
          target:
            targetKind === "person"
              ? { type: "person" }
              : { type: "animal", data: { species: selectedSpecies } },
          birth_category: birthCategory,
        }),
      });
      if (response.status === 404) {
        setMessage("Supporter checkout is not open yet. The world remains free to observe.");
        return;
      }
      if (response.status === 401) {
        setPanel({ kind: "signed_out", providers: ["google", "apple"] });
        setMessage("Your supporter session expired. Please sign in again.");
        return;
      }
      if (!response.ok) {
        setMessage("That name could not enter moderation. Check it and try again.");
        return;
      }
      const checkout = (await response.json()) as { checkout_url?: string };
      const checkoutUrl = checkout.checkout_url ? new URL(checkout.checkout_url) : null;
      if (!checkoutUrl || checkoutUrl.protocol !== "https:") {
        setMessage("The secure checkout link was unavailable. Nothing was charged.");
        return;
      }
      window.location.assign(checkoutUrl.href);
    } catch {
      setMessage("The supporter service is resting. Nothing was charged; please try again later.");
    } finally {
      setSubmitting(false);
    }
  }

  async function cancel(reservationId: string) {
    const csrf = csrfToken();
    if (!csrf || panel.kind !== "ready") return;
    setMessage("");
    try {
      const response = await fetch(
        `/api/v1/supporters/${encodeURIComponent(reservationId)}/cancel`,
        { method: "POST", headers: { "x-csrf-token": csrf } },
      );
      if (!response.ok) {
        setMessage("That reservation could not be cancelled right now.");
        return;
      }
      setPanel(await loadPanel());
      setMessage("Reservation cancelled. Any verified payment is queued for a full refund.");
    } catch {
      setMessage("That reservation could not be cancelled right now.");
    }
  }

  async function logout() {
    const csrf = csrfToken();
    if (!csrf) return;
    try {
      const response = await fetch("/api/v1/auth/logout", {
        method: "POST",
        headers: { "x-csrf-token": csrf },
      });
      if (response.ok) setPanel(await loadPanel());
    } catch {
      setMessage("Sign out could not be completed right now.");
    }
  }

  if (panel.kind === "checking") {
    return <div className="supporter-console supporter-console-status">Checking supporter access…</div>;
  }
  if (panel.kind === "unavailable") {
    return (
      <div className="supporter-console supporter-console-status">
        <strong>Supporter naming is not open yet.</strong>
        <p>The observatory remains free and complete. Naming will open only after account and payment safeguards are active.</p>
      </div>
    );
  }
  if (panel.kind === "signed_out") {
    return (
      <div className="supporter-console">
        <p className="supporter-step">Sign in to follow the world or choose a future birth.</p>
        <div className="supporter-auth-actions">
          {panel.providers.includes("google") ? <a className="button button-dark" href="/api/v1/auth/google/start">Continue with Google</a> : null}
          {panel.providers.includes("apple") ? <a className="button button-outline" href="/api/v1/auth/apple/start">Continue with Apple</a> : null}
        </div>
        <small>Accounts and payments are observer-side only. They cannot delay, select, or alter a birth.</small>
        {message ? <p className="supporter-message" role="status">{message}</p> : null}
      </div>
    );
  }

  return (
    <div className="supporter-console">
      <div className="supporter-console-heading">
        <p className="supporter-step">Choose the next eligible birth</p>
        <button className="supporter-signout" type="button" onClick={logout}>Sign out</button>
      </div>
      <form className="supporter-form" onSubmit={submit}>
        <label>
          Name
          <input name="observer_label" value={label} onChange={(event) => setLabel(event.target.value)} minLength={1} maxLength={80} autoComplete="off" required />
        </label>
        <label>
          Future life
          <select value={targetKind} onChange={(event) => setTargetKind(event.target.value as "person" | "animal")}>
            <option value="person">Person</option>
            <option value="animal" disabled={panel.animalSpecies.length === 0}>Animal</option>
          </select>
        </label>
        {targetKind === "animal" ? (
          <label>
            Species
            <select value={selectedSpecies?.identifier ?? ""} onChange={(event) => setSpeciesIdentifier(event.target.value)}>
              {panel.animalSpecies.map((species) => (
                <option key={`${species.catalog}:${species.identifier}`} value={species.identifier}>{commonSpeciesName(species.scientific_name)}</option>
              ))}
            </select>
          </label>
        ) : null}
        <label>
          Birth category
          <select value={birthCategory} onChange={(event) => setBirthCategory(event.target.value)}>
            <option value="female">Female</option>
            <option value="male">Male</option>
          </select>
        </label>
        <button className="button button-dark" type="submit" disabled={submitting}>
          {submitting ? "Opening secure checkout…" : "Continue to secure checkout"}
        </button>
      </form>
      <p className="supporter-fine-print">Payment enters moderation; rejected or cancelled unmatched names receive a full refund. A reservation waits for a matching natural birth and never creates one. <a href="/supporter-policy">Read the naming policy.</a></p>
      {message ? <p className="supporter-message" role="status">{message}</p> : null}
      {panel.reservations.length > 0 ? (
        <div className="reservation-list">
          <h3>Your reservations</h3>
          <ul>
            {panel.reservations.map((reservation) => (
              <li key={reservation.reservation_id}>
                <span><strong>{reservation.observer_label}</strong><small>{reservationTarget(reservation)} · {humanState(reservation)}</small></span>
                {["pending_payment", "pending_moderation", "active"].includes(reservation.state) ? <button type="button" onClick={() => void cancel(reservation.reservation_id)}>Cancel</button> : null}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}

async function loadPanel(signal?: AbortSignal): Promise<PanelState> {
  try {
    const session = await fetch("/api/v1/auth/session", { cache: "no-store", signal });
    if (session.status === 404) return { kind: "unavailable" };
    if (!session.ok) return { kind: "signed_out", providers: [] };
    const identity = (await session.json()) as { authenticated?: boolean; providers?: AuthProvider[] };
    if (!identity.authenticated) return { kind: "signed_out", providers: identity.providers ?? [] };
    const worldsResponse = await fetch("/api/v1/worlds", { cache: "no-store", signal });
    if (!worldsResponse.ok) return { kind: "unavailable" };
    const { worlds } = (await worldsResponse.json()) as { worlds: PublicWorld[] };
    const world = worlds[0]?.status === "running" ? worlds[0] : undefined;
    if (!world) return { kind: "unavailable" };
    const [organismsResponse, reservationsResponse] = await Promise.all([
      fetch(`/api/v1/worlds/${encodeURIComponent(world.world_id)}/organisms?limit=200`, { cache: "no-store", signal }),
      fetch("/api/v1/supporters/reservations?limit=100", { cache: "no-store", signal }),
    ]);
    const organisms = organismsResponse.ok ? ((await organismsResponse.json()) as { organisms: PublicOrganism[] }).organisms : [];
    const reservations = reservationsResponse.ok ? ((await reservationsResponse.json()) as { reservations: Reservation[] }).reservations : [];
    const species = new Map<string, Species>();
    for (const organism of organisms) {
      if (organism.role === "fauna") species.set(`${organism.species.catalog}:${organism.species.identifier}`, organism.species);
    }
    return { kind: "ready", world, animalSpecies: [...species.values()].sort((a, b) => commonSpeciesName(a.scientific_name).localeCompare(commonSpeciesName(b.scientific_name))), reservations };
  } catch {
    return { kind: "unavailable" };
  }
}

function csrfToken() {
  for (const pair of document.cookie.split(";")) {
    const [name, ...value] = pair.trim().split("=");
    if (name === "__Host-atiny_csrf" || name === "atiny_csrf") return value.join("=");
  }
  return undefined;
}

function reservationTarget(reservation: Reservation) {
  return reservation.target.type === "person" ? `Person · ${reservation.birth_category}` : `${commonSpeciesName(reservation.target.data.species.scientific_name)} · ${reservation.birth_category}`;
}

function humanState(reservation: Reservation) {
  if (reservation.refund_state === "pending") return "refund pending";
  if (reservation.refund_state === "completed") return "refunded";
  return reservation.state.replaceAll("_", " ");
}
