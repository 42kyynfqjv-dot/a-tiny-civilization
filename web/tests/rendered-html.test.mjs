import assert from "node:assert/strict";
import test from "node:test";

async function render() {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request("http://localhost/", { headers: { accept: "text/html" } }),
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
  assert.match(html, /History, before anyone knows it is history\./);
  assert.match(html, /Observer wiki/);
  assert.match(html, /Name the next naturally born life\./);
  assert.match(html, /Actual materials/);
  assert.doesNotMatch(html, /codex-preview|react-loading-skeleton|Starter Project/i);
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
