/**
 * Issue #630 — Service Worker Advanced Request Interception
 *
 * Routing heuristics:
 *   /api/*              → network-first, offline mock on failure
 *   /_next/static/*     → cache-first (immutable assets)
 *   images / fonts      → cache-first with stale-while-revalidate
 *   navigate (HTML)     → network-first with /offline.html fallback
 *
 * Offline mutations are stored in IndexedDB and replayed via Background Sync
 * when connectivity is restored (sync tag: "stellar-mutation-queue").
 */

const CACHE_VERSION = 'v1';
const STATIC_CACHE = `${CACHE_VERSION}-static`;
const DYNAMIC_CACHE = `${CACHE_VERSION}-dynamic`;
const IMAGE_CACHE = `${CACHE_VERSION}-images`;
const ALL_CACHES = [STATIC_CACHE, DYNAMIC_CACHE, IMAGE_CACHE];

const STATIC_PRECACHE = [
  '/',
  '/offline.html',
  '/manifest.json',
];

// ── Lifecycle ─────────────────────────────────────────────────────────────────

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(STATIC_CACHE)
      .then((cache) => cache.addAll(STATIC_PRECACHE))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(
          names
            .filter((n) => !ALL_CACHES.includes(n))
            .map((n) => caches.delete(n)),
        ),
      )
      .then(() => self.clients.claim()),
  );
});

// ── Fetch interception ────────────────────────────────────────────────────────

self.addEventListener('fetch', (event) => {
  const { request } = event;
  const url = new URL(request.url);

  // Only intercept same-origin requests
  if (url.origin !== self.location.origin) return;

  // Skip non-GET mutations — they go through the offline queue path instead
  if (request.method !== 'GET') {
    event.respondWith(handleMutationRequest(request));
    return;
  }

  if (url.pathname.startsWith('/api/')) {
    event.respondWith(networkFirstWithMock(request));
  } else if (
    url.pathname.startsWith('/_next/static/') ||
    url.pathname.match(/\.(js|css|woff2?|ttf|otf)$/)
  ) {
    event.respondWith(cacheFirst(request, STATIC_CACHE));
  } else if (url.pathname.match(/\.(png|jpg|jpeg|gif|svg|webp|avif|ico)$/)) {
    event.respondWith(staleWhileRevalidate(request, IMAGE_CACHE));
  } else if (request.mode === 'navigate') {
    event.respondWith(navigationHandler(request));
  } else {
    event.respondWith(staleWhileRevalidate(request, DYNAMIC_CACHE));
  }
});

// ── Routing strategies ────────────────────────────────────────────────────────

async function networkFirstWithMock(request) {
  try {
    const response = await fetchWithTimeout(request.clone(), 5000);
    if (response.ok) {
      const cache = await caches.open(DYNAMIC_CACHE);
      cache.put(request.clone(), response.clone());
    }
    return response;
  } catch {
    const cached = await caches.match(request);
    if (cached) return cached;
    return buildOfflineMock(new URL(request.url));
  }
}

async function cacheFirst(request, cacheName) {
  const cached = await caches.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok) {
    const cache = await caches.open(cacheName);
    cache.put(request.clone(), response.clone());
  }
  return response;
}

async function staleWhileRevalidate(request, cacheName) {
  const cache = await caches.open(cacheName);
  const cached = await cache.match(request);

  const networkFetch = fetch(request.clone()).then((response) => {
    if (response.ok) cache.put(request.clone(), response.clone());
    return response;
  });

  return cached ?? networkFetch;
}

async function navigationHandler(request) {
  try {
    const response = await fetchWithTimeout(request, 8000);
    const cache = await caches.open(DYNAMIC_CACHE);
    cache.put(request.clone(), response.clone());
    return response;
  } catch {
    const cached = await caches.match(request);
    if (cached) return cached;
    return caches.match('/offline.html') ?? new Response('Offline', { status: 503 });
  }
}

// ── Offline API mocking ───────────────────────────────────────────────────────

/**
 * Returns a plausible mock response for known API endpoints so the UI
 * degrades gracefully rather than breaking completely while offline.
 */
function buildOfflineMock(url) {
  const body = getMockBody(url.pathname);
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: {
      'Content-Type': 'application/json',
      'X-Served-By': 'service-worker-offline-mock',
    },
  });
}

function getMockBody(pathname) {
  if (pathname.startsWith('/api/creators')) {
    return { data: [], meta: { offline: true, total: 0 } };
  }
  if (pathname.startsWith('/api/bounties')) {
    return { data: [], meta: { offline: true, total: 0 } };
  }
  if (pathname.startsWith('/api/messages')) {
    return { data: [], meta: { offline: true } };
  }
  if (pathname.startsWith('/api/analytics')) {
    return { offline: true, metrics: {} };
  }
  return { offline: true, error: 'Unavailable offline' };
}

// ── Mutation queue (non-GET requests while offline) ───────────────────────────

const DB_NAME = 'stellar-sw-queue';
const STORE_NAME = 'mutations';

function openQueueDB() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE_NAME, { autoIncrement: true });
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function enqueueOfflineMutation(request) {
  const db = await openQueueDB();
  const body = await request.text();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite');
    tx.objectStore(STORE_NAME).add({
      url: request.url,
      method: request.method,
      headers: [...request.headers.entries()],
      body,
      timestamp: Date.now(),
    });
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

async function handleMutationRequest(request) {
  try {
    return await fetch(request);
  } catch {
    await enqueueOfflineMutation(request.clone());
    return new Response(JSON.stringify({ queued: true, offline: true }), {
      status: 202,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}

// ── Background Sync ───────────────────────────────────────────────────────────

self.addEventListener('sync', (event) => {
  if (event.tag === 'stellar-mutation-queue') {
    event.waitUntil(replayMutationQueue());
  }
});

async function replayMutationQueue() {
  const db = await openQueueDB();
  const mutations = await getAllMutations(db);

  for (const { id, record } of mutations) {
    try {
      const headers = new Headers(record.headers);
      await fetch(record.url, {
        method: record.method,
        headers,
        body: record.body || undefined,
      });
      await deleteMutation(db, id);
    } catch {
      // Leave failed mutations in the queue to retry on the next sync event.
    }
  }
}

function getAllMutations(db) {
  return new Promise((resolve, reject) => {
    const results = [];
    const tx = db.transaction(STORE_NAME, 'readonly');
    const cursor = tx.objectStore(STORE_NAME).openCursor();
    cursor.onsuccess = (e) => {
      const cur = e.target.result;
      if (cur) {
        results.push({ id: cur.key, record: cur.value });
        cur.continue();
      } else {
        resolve(results);
      }
    };
    cursor.onerror = () => reject(cursor.error);
  });
}

function deleteMutation(db, key) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite');
    tx.objectStore(STORE_NAME).delete(key);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function fetchWithTimeout(request, ms) {
  return Promise.race([
    fetch(request),
    new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), ms)),
  ]);
}
