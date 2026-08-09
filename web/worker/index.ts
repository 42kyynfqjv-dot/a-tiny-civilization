/** Cloudflare Worker entry point for the vinext-starter template. */
import { handleImageOptimization, DEFAULT_DEVICE_SIZES, DEFAULT_IMAGE_SIZES } from "vinext/server/image-optimization";
import handler from "vinext/server/app-router-entry";

interface Env {
  ASSETS: Fetcher;
  OBSERVER_API_URL?: string;
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

// Image security config. SVG sources with .svg extension auto-skip the
// optimization endpoint on the client side (served directly, no proxy).
// To route SVGs through the optimizer (with security headers), set
// dangerouslyAllowSVG: true in next.config.js and uncomment below:
// const imageConfig: ImageConfig = { dangerouslyAllowSVG: true };

const worker = {
  async fetch(request: Request, env: Env | undefined, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);

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
      const upstream = new URL(url.pathname + url.search, observerApiUrl);
      return withSecurityHeaders(await fetch(new Request(upstream, request)), url.pathname);
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

    return withSecurityHeaders(await handler.fetch(request, env, ctx), url.pathname);
  },
};

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
