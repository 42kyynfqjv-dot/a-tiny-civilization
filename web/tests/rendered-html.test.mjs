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

test("redirects the canonical public hostname to HTTPS without losing the request target", async () => {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("https-redirect", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  const response = await worker.fetch(
    new Request("http://atinycivilization.com/api/v1/auth/apple/callback?state=fixture", {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: "code=single-use",
    }),
    { ASSETS: { fetch: async () => new Response("Not found", { status: 404 }) } },
    { waitUntil() {}, passThroughOnException() {} },
  );

  assert.equal(response.status, 308);
  assert.equal(
    response.headers.get("location"),
    "https://atinycivilization.com/api/v1/auth/apple/callback?state=fixture",
  );
  assert.equal(response.headers.get("x-content-type-options"), "nosniff");
  assert.equal(response.headers.get("cache-control"), "no-store");
});

test("does not loop when Cloudflare Tunnel forwards an HTTPS visitor over HTTP", async () => {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("tunnel-https", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  const response = await worker.fetch(
    new Request("http://atinycivilization.com/", {
      headers: {
        "cf-visitor": '{"scheme":"https"}',
        "x-forwarded-proto": "https",
      },
    }),
    { ASSETS: { fetch: async () => new Response("Not found", { status: 404 }) } },
    { waitUntil() {}, passThroughOnException() {} },
  );

  assert.equal(response.status, 200);
  assert.equal(response.headers.get("location"), null);
  assert.match(await response.text(), /A world where every life writes its own story\./);
});

test("server-renders the civilization observatory", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);
  assert.equal(response.headers.get("x-content-type-options"), "nosniff");
  assert.equal(response.headers.get("x-frame-options"), "DENY");
  assert.equal(response.headers.get("cross-origin-resource-policy"), "same-origin");
  assert.equal(response.headers.get("origin-agent-cluster"), "?1");
  assert.equal(response.headers.get("x-permitted-cross-domain-policies"), "none");
  assert.equal(response.headers.get("cache-control"), "no-store");
  assert.match(response.headers.get("content-security-policy") ?? "", /frame-ancestors 'none'/);
  assert.match(response.headers.get("permissions-policy") ?? "", /payment=\(\)/);

  const html = await response.text();
  assert.match(html, /<title>Live World · A Tiny Civilization<\/title>/i);
  assert.match(html, /A world where every life writes its own story\./);
  assert.match(html, /The whole world beneath every life\./);
  assert.match(html, /Living Atlas|A living Earth/);
  assert.match(html, /Every claim has a trail/);
  assert.doesNotMatch(html, /River basin · seed awaiting launch/);
  assert.match(html, /The public wiki/);
  assert.match(html, /Give a future life a name\./);
  assert.match(html, /civilization cannot perceive reservations or supporters/);
  assert.match(html, /Checking supporter access/);
  assert.doesNotMatch(html, /Opens after first births/);
  assert.match(html, /Nothing lived here is discarded\./);
  assert.match(html, /If every person dies/);
  assert.match(html, /Lives with a past/);
  assert.doesNotMatch(html, /class="sidebar"|class="app-shell"/);
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

test("server-renders a selectable life directory", async () => {
  const response = await render("/lives");
  assert.equal(response.status, 200);
  const html = await response.text();
  assert.match(html, /<title>Lives · A Tiny Civilization<\/title>/i);
  assert.match(html, /Choose someone to return to\./);
  assert.match(html, /Following changes only your observatory/);
  assert.match(html, /Opening the life index/);
});

test("server-renders an individual life route", async () => {
  const response = await render("/lives/b3ea736d-7a5a-5161-a74b-fa8c4302d333/00000000-0000-0000-0000-000000000001");
  assert.equal(response.status, 200);
  const html = await response.text();
  assert.match(html, /<title>Life Record · A Tiny Civilization<\/title>/i);
  assert.match(html, /Reading this life’s public record/);
  assert.match(html, /All lives/);
});

for (const [path, title, marker] of [
  ["/privacy", "Privacy notice", "Observer data is not sold"],
  ["/terms", "Terms of use", "not a promise that civilization"],
  ["/supporter-policy", "Supporter naming policy", "never creates, schedules, delays"],
  ["/presentation-policy", "World presentation policy", "never presents sexual activity"],
]) {
  test(`server-renders public policy ${path}`, async () => {
    const response = await render(path);
    assert.equal(response.status, 200);
    const html = await response.text();
    assert.match(html, new RegExp(`<title>${title} · A Tiny Civilization</title>`, "i"));
    assert.match(html, new RegExp(marker, "i"));
    assert.match(html, /Public project policy/);
  });
}

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
    assert.equal(response.headers.get("x-content-type-options"), "nosniff");
    assert.match(response.headers.get("content-security-policy") ?? "", /default-src 'self'/);
    assert.equal(response.headers.get("cache-control"), "no-store");
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("preserves auth POST bodies, cookies, redirects, and response cookies", async () => {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("auth-proxy", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);
  const originalFetch = globalThis.fetch;
  let captured;
  globalThis.fetch = async (request) => {
    captured = {
      url: request.url,
      method: request.method,
      cookie: request.headers.get("cookie"),
      contentType: request.headers.get("content-type"),
      body: await request.text(),
    };
    return new Response(null, {
      status: 303,
      headers: {
        location: "/",
        "set-cookie": "__Host-atiny_session=fixture; Path=/; Secure; HttpOnly; SameSite=Lax",
      },
    });
  };
  try {
    const response = await worker.fetch(
      new Request("https://atinycivilization.com/api/v1/auth/apple/callback", {
        method: "POST",
        headers: {
          cookie: "__Host-atiny_oauth_binding=browser-fixture",
          "content-type": "application/x-www-form-urlencoded",
        },
        body: "code=single-use&state=state-fixture",
      }),
      {
        ASSETS: { fetch: async () => new Response("Not found", { status: 404 }) },
        OBSERVER_API_URL: "http://observer.internal:8080/",
      },
      { waitUntil() {}, passThroughOnException() {} },
    );
    assert.deepEqual(captured, {
      url: "http://observer.internal:8080/api/v1/auth/apple/callback",
      method: "POST",
      cookie: "__Host-atiny_oauth_binding=browser-fixture",
      contentType: "application/x-www-form-urlencoded",
      body: "code=single-use&state=state-fixture",
    });
    assert.equal(response.status, 303);
    assert.equal(response.headers.get("location"), "/");
    assert.match(response.headers.get("set-cookie") ?? "", /__Host-atiny_session=fixture/);
    assert.equal(response.headers.get("cache-control"), "no-store");
  } finally {
    globalThis.fetch = originalFetch;
  }
});
