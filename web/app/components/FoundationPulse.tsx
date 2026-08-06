"use client";

import { useEffect, useState } from "react";

type FoundationStatus = {
  environment: string;
  worlds: {
    initializing: number;
    running: number;
    archived: number;
  };
  latest_runner_heartbeat: string | null;
};

type Pulse =
  | { state: "checking" }
  | { state: "offline" }
  | { state: "online"; status: FoundationStatus };

export function FoundationPulse({ compact = false }: { compact?: boolean }) {
  const [pulse, setPulse] = useState<Pulse>({ state: "checking" });

  useEffect(() => {
    let active = true;

    async function refresh() {
      try {
        const response = await fetch("/api/v1/status", {
          headers: { accept: "application/json" },
          cache: "no-store",
        });
        if (!response.ok) throw new Error("status unavailable");
        const status = (await response.json()) as FoundationStatus;
        if (active) setPulse({ state: "online", status });
      } catch {
        if (active) setPulse({ state: "offline" });
      }
    }

    void refresh();
    const timer = window.setInterval(refresh, 15_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  const label =
    pulse.state === "checking"
      ? "Checking foundation"
      : pulse.state === "offline"
        ? "Foundation offline"
        : pulse.status.worlds.running > 0
          ? `${pulse.status.worlds.running} world live`
          : "Foundation online";

  return (
    <div className={`foundation-pulse ${compact ? "compact" : ""}`} role="status">
      <span className={`pulse-dot pulse-${pulse.state}`} aria-hidden="true" />
      <span>{label}</span>
    </div>
  );
}
