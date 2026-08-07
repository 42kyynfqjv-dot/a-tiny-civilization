import assert from "node:assert/strict";
import test from "node:test";

async function render(path = "/") {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request(`http://localhost${path}`, { headers: { accept: "text/html" } }),
    { ASSETS: { fetch: async () => new Response("Not found", { status: 404 }) } },
    { waitUntil() {}, passThroughOnException() {} },
  );
}

test("server-renders the civilization observatory", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);

  const html = await response.text();
  assert.match(html, /<title>Live World · A Tiny Civilization<\/title>/i);
  assert.match(html, /A little world with a life of its own\./);
  assert.match(html, /Waiting for its first sunrise/);
  assert.match(html, /provisional—not yet scientifically admitted/);
  assert.doesNotMatch(html, /River basin · seed awaiting launch/);
  assert.match(html, /World notebook/);
  assert.match(html, /Give a future life a name\./);
  assert.match(html, /Even an ending becomes a story\./);
  assert.match(html, /If every person dies/);
  assert.match(html, /Places that change/);
  assert.doesNotMatch(html, /Scientific reference|Committed observer record|Integrity rule 01/);
  assert.doesNotMatch(html, /codex-preview|react-loading-skeleton|Starter Project/i);
});

test("server-renders the observer wiki as a separate read-only route", async () => {
  const response = await render("/wiki");
  assert.equal(response.status, 200);

  const html = await response.text();
  assert.match(html, /<title>Observer Wiki · A Tiny Civilization<\/title>/i);
  assert.match(html, /Evidence first\. Interpretation stays visible\./);
  assert.match(html, /Provenance model/);
  assert.match(html, /Research papers and artifacts/);
  assert.doesNotMatch(html, /create wiki claim|steering wheel/i);
});

test("proxies only observer API paths when configured", async () => {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("proxy", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);
  const originalFetch = globalThis.fetch;
  let upstream;
  globalThis.fetch = async (request) => {
    upstream = new URL(request.url);
    return Response.json({ ok: true });
  };
  try {
    const response = await worker.fetch(
      new Request("http://localhost/api/v1/status?sample=1"),
      {
        ASSETS: { fetch: async () => new Response("Not found", { status: 404 }) },
        OBSERVER_API_URL: "http://observer.internal:8080/",
      },
      { waitUntil() {}, passThroughOnException() {} },
    );
    assert.equal(response.status, 200);
    assert.equal(upstream.href, "http://observer.internal:8080/api/v1/status?sample=1");
  } finally {
    globalThis.fetch = originalFetch;
  }
});
