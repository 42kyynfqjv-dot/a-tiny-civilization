"use client";

import { useEffect, useState } from "react";

export function NewsletterPanel() {
  const [enabled, setEnabled] = useState<boolean | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    void fetch("/api/v1/newsletter/status", { cache: "no-store", signal: controller.signal })
      .then(async (response) => response.ok ? (await response.json()) as { enabled?: boolean } : null)
      .then((status) => {
        if (!controller.signal.aborted) setEnabled(status?.enabled === true);
      })
      .catch(() => {
        if (!controller.signal.aborted) setEnabled(false);
      });
    return () => controller.abort();
  }, []);

  return (
    <section className="living-newsletter" aria-labelledby="newsletter-title">
      <div>
        <p className="eyebrow">Letters from behind the glass</p>
        <h2 id="newsletter-title">Come back when the world has changed.</h2>
        <p>A factual handful of new lives, meetings, memories, and firsts—never invented narration.</p>
        <small>Email addresses and unsubscribe preferences live only with the newsletter provider, not in the civilization database.</small>
      </div>
      <div className="living-newsletter-actions" aria-live="polite">
        {enabled ? (
          <>
            <a className="button button-dark" href="/api/v1/newsletter/subscribe?cadence=daily">Daily glimpse</a>
            <a className="button button-outline" href="/api/v1/newsletter/subscribe?cadence=weekly">Weekly letter</a>
          </>
        ) : (
          <span>{enabled === null ? "Checking the postbox…" : "The postbox is being prepared."}</span>
        )}
      </div>
    </section>
  );
}
