/**
 * First-party proxy to the metadata API.
 *
 * The browser only ever talks to this origin, so the HttpOnly session cookie
 * set by the API stays first-party and the API never needs to be reachable from
 * the user's network. The proxy forwards the headers that matter for auth and
 * idempotency, and strips everything else.
 *
 * `OPENWORK_API_URL` is a server-only variable. It is never exposed to the
 * browser bundle.
 */
import { NextResponse } from 'next/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const API_ORIGIN = (process.env.OPENWORK_API_URL ?? 'http://127.0.0.1:8080').replace(
  /\/+$/,
  '',
);

/** Request headers that carry meaning for the API. */
const FORWARDED_REQUEST_HEADERS = [
  'accept',
  'accept-language',
  'content-type',
  'cookie',
  'origin',
  'referer',
  'user-agent',
  'x-openwork-csrf',
  'idempotency-key',
] as const;

/** Response headers that are safe and useful to hand back to the browser. */
const FORWARDED_RESPONSE_HEADERS = [
  'content-type',
  'cache-control',
  'x-request-id',
  'retry-after',
] as const;

function upstreamUrl(request: Request, path: readonly string[]): string {
  const search = new URL(request.url).search;
  // Each segment is already decoded by the router; re-encode so a segment
  // cannot introduce a new path or query.
  const suffix = path.map((segment) => encodeURIComponent(segment)).join('/');
  return `${API_ORIGIN}/${suffix}${search}`;
}

async function proxy(
  request: Request,
  context: { params: Promise<{ path?: string[] }> },
): Promise<Response> {
  const { path } = await context.params;
  if (!path || path.length === 0) {
    return NextResponse.json(
      {
        error: {
          code: 'not_found',
          message: 'The proxy needs a path, for example /api/proxy/health/live',
        },
      },
      { status: 404 },
    );
  }

  const headers = new Headers();
  for (const name of FORWARDED_REQUEST_HEADERS) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  // Ask for an uncompressed body: we stream it through without re-encoding.
  headers.set('accept-encoding', 'identity');

  let body: ArrayBuffer | undefined;
  if (request.method !== 'GET' && request.method !== 'HEAD') {
    body = await request.arrayBuffer();
  }

  let upstream: Response;
  try {
    upstream = await fetch(upstreamUrl(request, path), {
      method: request.method,
      headers,
      body,
      redirect: 'manual',
      cache: 'no-store',
      signal: AbortSignal.timeout(30_000),
    });
  } catch {
    // A transport failure is not an API response. 502 keeps the client's
    // chain-only fallback honest instead of pretending the request succeeded.
    return NextResponse.json(
      {
        error: {
          code: 'api_unavailable',
          message: 'The OpenWork API is not reachable',
        },
      },
      { status: 502 },
    );
  }

  const responseHeaders = new Headers();
  for (const name of FORWARDED_RESPONSE_HEADERS) {
    const value = upstream.headers.get(name);
    if (value) responseHeaders.set(name, value);
  }

  // Multiple Set-Cookie headers must be preserved individually.
  const cookies =
    typeof upstream.headers.getSetCookie === 'function'
      ? upstream.headers.getSetCookie()
      : [upstream.headers.get('set-cookie')].filter((value): value is string =>
          Boolean(value),
        );
  for (const cookie of cookies) responseHeaders.append('set-cookie', cookie);

  const empty = upstream.status === 204 || upstream.status === 304;
  return new NextResponse(empty ? null : upstream.body, {
    status: upstream.status,
    headers: responseHeaders,
  });
}

export const GET = proxy;
export const HEAD = proxy;
export const POST = proxy;
export const PUT = proxy;
export const PATCH = proxy;
export const DELETE = proxy;
export const OPTIONS = proxy;
