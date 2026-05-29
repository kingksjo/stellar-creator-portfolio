/**
 * Issue #630 — Client-side offline mutation queue
 *
 * Stores pending mutations in IndexedDB when the user is offline.
 * Registers a Background Sync tag so the service worker replays them
 * automatically once connectivity is restored.
 */

const DB_NAME = 'stellar-offline-queue';
const STORE_NAME = 'mutations';
const SYNC_TAG = 'stellar-mutation-queue';

export interface OfflineMutation {
  url: string;
  method: string;
  headers: Record<string, string>;
  body?: string;
  timestamp: number;
}

function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = () => {
      req.result.createObjectStore(STORE_NAME, { autoIncrement: true });
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

/** Queue a mutation for later replay. */
export async function enqueue(mutation: OfflineMutation): Promise<void> {
  const db = await openDB();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite');
    tx.objectStore(STORE_NAME).add(mutation);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });

  // Notify the service worker to replay when online
  if ('serviceWorker' in navigator && 'SyncManager' in window) {
    const reg = await navigator.serviceWorker.ready;
    await reg.sync.register(SYNC_TAG);
  }
}

/** Retrieve all queued mutations without removing them. */
export async function listQueued(): Promise<OfflineMutation[]> {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const results: OfflineMutation[] = [];
    const tx = db.transaction(STORE_NAME, 'readonly');
    const req = tx.objectStore(STORE_NAME).openCursor();
    req.onsuccess = (e) => {
      const cursor = (e.target as IDBRequest<IDBCursorWithValue>).result;
      if (cursor) {
        results.push(cursor.value as OfflineMutation);
        cursor.continue();
      } else {
        resolve(results);
      }
    };
    req.onerror = () => reject(req.error);
  });
}

/**
 * Replay all queued mutations directly from the page context.
 * Prefer the Background Sync path (via the service worker) when available;
 * this is a fallback for browsers that do not support Background Sync.
 */
export async function flush(): Promise<{ replayed: number; failed: number }> {
  const db = await openDB();
  const mutations = await listAllWithKeys(db);

  let replayed = 0;
  let failed = 0;

  for (const { key, mutation } of mutations) {
    try {
      await fetch(mutation.url, {
        method: mutation.method,
        headers: new Headers(mutation.headers),
        body: mutation.body,
      });
      await deleteRecord(db, key);
      replayed++;
    } catch {
      failed++;
    }
  }

  return { replayed, failed };
}

function listAllWithKeys(db: IDBDatabase): Promise<Array<{ key: IDBValidKey; mutation: OfflineMutation }>> {
  return new Promise((resolve, reject) => {
    const results: Array<{ key: IDBValidKey; mutation: OfflineMutation }> = [];
    const tx = db.transaction(STORE_NAME, 'readonly');
    const req = tx.objectStore(STORE_NAME).openCursor();
    req.onsuccess = (e) => {
      const cursor = (e.target as IDBRequest<IDBCursorWithValue>).result;
      if (cursor) {
        results.push({ key: cursor.key, mutation: cursor.value as OfflineMutation });
        cursor.continue();
      } else {
        resolve(results);
      }
    };
    req.onerror = () => reject(req.error);
  });
}

function deleteRecord(db: IDBDatabase, key: IDBValidKey): Promise<void> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite');
    tx.objectStore(STORE_NAME).delete(key);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}
