/** Cloudflare Worker entry point for the vinext-starter template. */
import { handleImageOptimization, DEFAULT_DEVICE_SIZES, DEFAULT_IMAGE_SIZES } from "vinext/server/image-optimization";
import handler from "vinext/server/app-router-entry";

interface Env {
  ASSETS: Fetcher;
  OBSERVER_API_URL?: string;
  CANCER_CONSOLE_TOKEN?: string;
  CANCER_WORLD_ID?: string;
  IMAGES: {
    input(stream: ReadableStream): {
      transform(options: Record<string, unknown>): {
        output(options: { format: string; quality: number }): Promise<{ response(): Response }>;
      };
    };
  };
}

interface ExecutionContext {
  waitUntil(promise: Promise<unknown>): void;
  passThroughOnException(): void;
}

const CANCER_ACCESS_COOKIE = "__Host-atc_cancer_access";
const CANCER_ACCESS_HEADER = "x-atc-cancer-console-token";

// Image security config. SVG sources with .svg extension auto-skip the
// optimization endpoint on the client side (served directly, no proxy).
// To route SVGs through the optimizer (with security headers), set
// dangerouslyAllowSVG: true in next.config.js and uncomment below:
// const imageConfig: ImageConfig = { dangerouslyAllowSVG: true };

const worker = {
  async fetch(request: Request, env: Env | undefined, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);
    const cancerConsoleToken =
      env?.CANCER_CONSOLE_TOKEN ??
      (typeof process === "undefined" ? undefined : process.env.CANCER_CONSOLE_TOKEN);
    const cancerWorldId =
      env?.CANCER_WORLD_ID ??
      (typeof process === "undefined" ? undefined : process.env.CANCER_WORLD_ID);
    const cancerConsolePath = cancerConsoleToken ? `/research/${cancerConsoleToken}` : undefined;
    const cancerAccessFingerprint = cancerConsoleToken
      ? await sha256Hex(cancerConsoleToken)
      : undefined;

    if (url.pathname === "/cancer-console") {
      return withSecurityHeaders(new Response("Not found", { status: 404 }), url.pathname);
    }

    let routedRequest = request;
    let isCancerConsole = false;
    if (cancerConsolePath && url.pathname === cancerConsolePath) {
      isCancerConsole = true;
      url.pathname = "/cancer-console";
      const headers = new Headers(request.headers);
      if (cancerWorldId) headers.set("x-atc-cancer-world-id", cancerWorldId);
      const requestInit: RequestInit = {
        headers,
        method: request.method,
        redirect: request.redirect,
      };
      if (request.method !== "GET" && request.method !== "HEAD") {
        requestInit.body = request.body;
      }
      routedRequest = new Request(url, requestInit);
    }

    // Cloudflare's zone-level Always Use HTTPS setting is the preferred first
    // hop. Keep the same invariant at the application boundary so a missing or
    // accidentally disabled edge toggle cannot serve the canonical hostname
    // over plaintext after a release. A 308 preserves POST bodies for OAuth
    // callbacks and other future non-GET routes.
    // Cloudflare Tunnel terminates TLS at the edge and reaches this loopback-only
    // origin over HTTP. In that case the Request URL is `http:` even though the
    // visitor is already on HTTPS; redirecting it would produce a same-URL loop.
    // Accept only the standard proxy scheme signals used on that trusted hop.
    const forwardedProtocol = request.headers.get("x-forwarded-proto")?.split(",", 1)[0]?.trim();
    const visitorProtocol = cloudflareVisitorProtocol(request.headers.get("cf-visitor"));
    const visitorUsedHttps =
      url.protocol === "https:" || forwardedProtocol === "https" || visitorProtocol === "https";
    if (url.hostname === "atinycivilization.com" && !visitorUsedHttps) {
      url.protocol = "https:";
      return withSecurityHeaders(Response.redirect(url, 308), url.pathname);
    }

    if (url.pathname.startsWith("/api/")) {
      // Cloudflare supplies bindings through `env`. Vinext's Node production
      // server does not, so retain the same explicit container setting as a
      // local fallback. The `typeof` guard keeps this valid in Workers.
      const observerApiUrl =
        env?.OBSERVER_API_URL ??
        (typeof process === "undefined" ? undefined : process.env.OBSERVER_API_URL);

      if (!observerApiUrl) {
        return withSecurityHeaders(
          Response.json(
            { error: { code: "observer_api_unconfigured", message: "observer API is unavailable" } },
            { status: 503 },
          ),
          url.pathname,
        );
      }
      const cancerApi = isCancerApiPath(url.pathname, cancerWorldId);
      const authorizedCancerRequest = Boolean(
        cancerApi &&
          cancerConsoleToken &&
          cancerAccessFingerprint &&
          constantTimeEqual(
            cookieValue(request.headers.get("cookie"), CANCER_ACCESS_COOKIE) ?? "",
            cancerAccessFingerprint,
          ),
      );
      if (cancerApi && !authorizedCancerRequest) {
        return withSecurityHeaders(new Response("Not found", { status: 404 }), url.pathname);
      }
      const upstream = new URL(url.pathname + url.search, observerApiUrl);
      const upstreamHeaders = new Headers(request.headers);
      upstreamHeaders.delete(CANCER_ACCESS_HEADER);
      if (authorizedCancerRequest && cancerConsoleToken) {
        upstreamHeaders.set(CANCER_ACCESS_HEADER, cancerConsoleToken);
      }
      const forwarded = new Request(new Request(upstream, request), { headers: upstreamHeaders });
      const upstreamResponse = await fetch(forwarded);
      if (cancerWorldId && url.pathname === "/api/v1/worlds" && upstreamResponse.ok) {
        const filtered = await withoutCancerWorld(upstreamResponse, cancerWorldId);
        return withSecurityHeaders(filtered, url.pathname);
      }
      return withSecurityHeaders(upstreamResponse, url.pathname);
    }

    if (url.pathname === "/_vinext/image") {
      const allowedWidths = [...DEFAULT_DEVICE_SIZES, ...DEFAULT_IMAGE_SIZES];
      return withSecurityHeaders(await handleImageOptimization(request, {
        fetchAsset: (path) => env.ASSETS.fetch(new Request(new URL(path, request.url))),
        transformImage: async (body, { width, format, quality }) => {
          const result = await env.IMAGES.input(body).transform(width > 0 ? { width } : {}).output({ format, quality });
          return result.response();
        },
      }, allowedWidths), url.pathname);
    }

    const response = withSecurityHeaders(await handler.fetch(routedRequest, env, ctx), url.pathname);
    if (!isCancerConsole) return response;
    const headers = new Headers(response.headers);
    headers.set("x-robots-tag", "noindex, nofollow, noarchive, nosnippet");
    if (cancerAccessFingerprint) {
      headers.append(
        "set-cookie",
        `${CANCER_ACCESS_COOKIE}=${cancerAccessFingerprint}; Path=/; Max-Age=604800; HttpOnly; SameSite=Strict; Secure`,
      );
    }
    return new Response(response.body, { headers, status: response.status, statusText: response.statusText });
  },
};

function isCancerApiPath(pathname: string, worldId: string | undefined): boolean {
  if (/\/research(?:\/|$)/.test(pathname)) return true;
  if (!worldId) return false;
  const prefix = `/api/v1/worlds/${worldId.toLowerCase()}`;
  return pathname.toLowerCase() === prefix || pathname.toLowerCase().startsWith(`${prefix}/`);
}

function cookieValue(raw: string | null, name: string): string | undefined {
  if (!raw) return undefined;
  const matches = raw
    .split(";")
    .map((part) => part.trim())
    .filter((part) => part.startsWith(`${name}=`))
    .map((part) => part.slice(name.length + 1));
  return matches.length === 1 && matches[0] ? matches[0] : undefined;
}

function constantTimeEqual(left: string, right: string): boolean {
  if (left.length !== right.length) return false;
  let difference = 0;
  for (let index = 0; index < left.length; index += 1) {
    difference |= left.charCodeAt(index) ^ right.charCodeAt(index);
  }
  return difference === 0;
}

async function sha256Hex(value: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

async function withoutCancerWorld(response: Response, cancerWorldId: string): Promise<Response> {
  try {
    const payload = (await response.clone().json()) as { worlds?: Array<{ world_id?: unknown }> };
    if (!Array.isArray(payload.worlds)) return response;
    const hiddenId = cancerWorldId.toLowerCase();
    payload.worlds = payload.worlds.filter(
      (world) => typeof world.world_id !== "string" || world.world_id.toLowerCase() !== hiddenId,
    );
    const headers = new Headers(response.headers);
    headers.set("content-type", "application/json");
    return new Response(JSON.stringify(payload), {
      headers,
      status: response.status,
      statusText: response.statusText,
    });
  } catch {
    return response;
  }
}

function cloudflareVisitorProtocol(value: string | null): string | undefined {
  if (!value) return undefined;
  try {
    const parsed = JSON.parse(value) as unknown;
    if (typeof parsed !== "object" || parsed === null || !("scheme" in parsed)) return undefined;
    const scheme = (parsed as { scheme?: unknown }).scheme;
    return scheme === "http" || scheme === "https" ? scheme : undefined;
  } catch {
    return undefined;
  }
}

function withSecurityHeaders(response: Response, pathname: string): Response {
  const headers = new Headers(response.headers);
  headers.set(
    "content-security-policy",
    "default-src 'self'; base-uri 'self'; connect-src 'self'; font-src 'self'; form-action 'self'; frame-ancestors 'none'; img-src 'self' data: blob:; manifest-src 'self'; object-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; worker-src 'self' blob:; upgrade-insecure-requests",
  );
  headers.set("cross-origin-resource-policy", "same-origin");
  headers.set("cross-origin-opener-policy", "same-origin");
  headers.set("origin-agent-cluster", "?1");
  headers.set(
    "permissions-policy",
    "attribution-reporting=(), browsing-topics=(), camera=(), geolocation=(), join-ad-interest-group=(), microphone=(), payment=(), run-ad-auction=()",
  );
  headers.set("referrer-policy", "no-referrer");
  headers.set("strict-transport-security", "max-age=31536000; includeSubDomains");
  headers.set("x-content-type-options", "nosniff");
  headers.set("x-dns-prefetch-control", "off");
  headers.set("x-frame-options", "DENY");
  headers.set("x-permitted-cross-domain-policies", "none");

  if (
    pathname.startsWith("/api/") ||
    (!pathname.startsWith("/_next/static/") && pathname !== "/_vinext/image")
  ) {
    headers.set("cache-control", "no-store");
  }

  return new Response(response.body, {
    headers,
    status: response.status,
    statusText: response.statusText,
  });
}

export default worker;
