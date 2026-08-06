const DEFAULT_OBSERVER_API = "http://127.0.0.1:8080";

export async function GET(): Promise<Response> {
  const baseUrl = (process.env.OBSERVER_API_URL ?? DEFAULT_OBSERVER_API).replace(/\/$/, "");

  try {
    const upstream = await fetch(`${baseUrl}/api/v1/status`, {
      headers: { accept: "application/json" },
      cache: "no-store",
      signal: AbortSignal.timeout(2_000),
    });
    const body = await upstream.text();

    return new Response(body, {
      status: upstream.status,
      headers: {
        "content-type": upstream.headers.get("content-type") ?? "application/json",
        "cache-control": "no-store",
      },
    });
  } catch {
    return Response.json(
      {
        error: {
          code: "observer_api_unavailable",
          message: "The observer API is temporarily unavailable.",
        },
      },
      { status: 503, headers: { "cache-control": "no-store" } },
    );
  }
}
