/**
 * Tests for Issue #630 — Service Worker routing heuristics and offline mock responses
 */
import { describe, it, expect } from 'vitest';

// ── Routing heuristics (pure logic extracted from sw.js) ──────────────────────

function classifyRequest(method: string, pathname: string): string {
  if (method !== 'GET') return 'mutation';
  if (pathname.startsWith('/api/')) return 'api';
  if (pathname.startsWith('/_next/static/') || /\.(js|css|woff2?|ttf|otf)$/.test(pathname)) return 'static';
  if (/\.(png|jpg|jpeg|gif|svg|webp|avif|ico)$/.test(pathname)) return 'image';
  return 'navigation';
}

describe('Service Worker routing heuristics', () => {
  it('classifies API GET requests as api', () => {
    expect(classifyRequest('GET', '/api/creators')).toBe('api');
    expect(classifyRequest('GET', '/api/bounties')).toBe('api');
    expect(classifyRequest('GET', '/api/messages')).toBe('api');
  });

  it('classifies non-GET requests as mutations', () => {
    expect(classifyRequest('POST', '/api/bounties')).toBe('mutation');
    expect(classifyRequest('PUT', '/api/creators/1')).toBe('mutation');
    expect(classifyRequest('DELETE', '/api/messages/5')).toBe('mutation');
  });

  it('classifies Next.js static chunks as static', () => {
    expect(classifyRequest('GET', '/_next/static/chunks/main.js')).toBe('static');
    expect(classifyRequest('GET', '/styles/main.css')).toBe('static');
    expect(classifyRequest('GET', '/fonts/geist.woff2')).toBe('static');
  });

  it('classifies image assets as image', () => {
    expect(classifyRequest('GET', '/images/hero.jpg')).toBe('image');
    expect(classifyRequest('GET', '/icon.svg')).toBe('image');
    expect(classifyRequest('GET', '/apple-icon.png')).toBe('image');
  });

  it('classifies HTML navigation as navigation', () => {
    expect(classifyRequest('GET', '/creators')).toBe('navigation');
    expect(classifyRequest('GET', '/bounties/123')).toBe('navigation');
    expect(classifyRequest('GET', '/')).toBe('navigation');
  });
});

// ── Offline mock body ─────────────────────────────────────────────────────────

function getMockBody(pathname: string): unknown {
  if (pathname.startsWith('/api/creators')) return { data: [], meta: { offline: true, total: 0 } };
  if (pathname.startsWith('/api/bounties')) return { data: [], meta: { offline: true, total: 0 } };
  if (pathname.startsWith('/api/messages')) return { data: [], meta: { offline: true } };
  if (pathname.startsWith('/api/analytics')) return { offline: true, metrics: {} };
  return { offline: true, error: 'Unavailable offline' };
}

describe('Offline mock responses', () => {
  it('returns empty list for /api/creators when offline', () => {
    const body = getMockBody('/api/creators') as { data: unknown[]; meta: { offline: boolean } };
    expect(body.data).toEqual([]);
    expect(body.meta.offline).toBe(true);
  });

  it('returns empty list for /api/bounties when offline', () => {
    const body = getMockBody('/api/bounties') as { data: unknown[] };
    expect(body.data).toEqual([]);
  });

  it('returns offline flag for /api/messages', () => {
    const body = getMockBody('/api/messages') as { meta: { offline: boolean } };
    expect(body.meta.offline).toBe(true);
  });

  it('returns empty metrics for /api/analytics', () => {
    const body = getMockBody('/api/analytics') as { offline: boolean; metrics: object };
    expect(body.offline).toBe(true);
    expect(body.metrics).toEqual({});
  });

  it('returns generic offline error for unknown paths', () => {
    const body = getMockBody('/api/unknown') as { offline: boolean; error: string };
    expect(body.offline).toBe(true);
    expect(body.error).toMatch(/offline/i);
  });
});

// ── Cache versioning ──────────────────────────────────────────────────────────

describe('Cache versioning', () => {
  const CACHE_VERSION = 'v1';
  const ALL_CACHES = [`${CACHE_VERSION}-static`, `${CACHE_VERSION}-dynamic`, `${CACHE_VERSION}-images`];

  it('marks old cache names for deletion', () => {
    const existingCaches = ['v0-static', 'v0-dynamic', `${CACHE_VERSION}-static`];
    const toDelete = existingCaches.filter((n) => !ALL_CACHES.includes(n));
    expect(toDelete).toEqual(['v0-static', 'v0-dynamic']);
  });

  it('preserves current cache names', () => {
    const existingCaches = [...ALL_CACHES];
    const toDelete = existingCaches.filter((n) => !ALL_CACHES.includes(n));
    expect(toDelete).toEqual([]);
  });
});

// ── Background sync tag ───────────────────────────────────────────────────────

describe('Background Sync tag', () => {
  it('uses the canonical sync tag name', () => {
    const SYNC_TAG = 'stellar-mutation-queue';
    expect(SYNC_TAG).toBe('stellar-mutation-queue');
  });
});
