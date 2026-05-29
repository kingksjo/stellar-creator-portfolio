/**
 * Issue #629 — Layout Thrashing Metrics
 *
 * Uses PerformanceObserver to detect and report layout-shift and
 * long-task entries. Consumers can set a CLS threshold callback to
 * surface thrashing in observability pipelines.
 */

export interface LayoutMetrics {
  cls: number;      // Cumulative Layout Shift score
  longTasks: number; // Count of tasks blocking the main thread > 50 ms
}

type MetricsCallback = (metrics: LayoutMetrics) => void;

export class LayoutThrashMonitor {
  private cls = 0;
  private longTasks = 0;
  private observers: PerformanceObserver[] = [];
  private callback: MetricsCallback | null = null;

  constructor(callback?: MetricsCallback) {
    this.callback = callback ?? null;
  }

  start(): void {
    if (typeof window === 'undefined' || typeof PerformanceObserver === 'undefined') return;

    this.cls = 0;
    this.longTasks = 0;

    // Layout Shift observer
    if (PerformanceObserver.supportedEntryTypes?.includes('layout-shift')) {
      const lsObserver = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          const ls = entry as PerformanceEntry & { hadRecentInput?: boolean; value?: number };
          if (!ls.hadRecentInput && ls.value !== undefined) {
            this.cls += ls.value;
            this.flush();
          }
        }
      });
      try {
        lsObserver.observe({ type: 'layout-shift', buffered: true });
        this.observers.push(lsObserver);
      } catch {
        // Unsupported — silently skip
      }
    }

    // Long Task observer
    if (PerformanceObserver.supportedEntryTypes?.includes('longtask')) {
      const ltObserver = new PerformanceObserver((list) => {
        this.longTasks += list.getEntries().length;
        this.flush();
      });
      try {
        ltObserver.observe({ type: 'longtask', buffered: true });
        this.observers.push(ltObserver);
      } catch {
        // Unsupported — silently skip
      }
    }
  }

  stop(): void {
    for (const obs of this.observers) {
      obs.disconnect();
    }
    this.observers = [];
  }

  getMetrics(): LayoutMetrics {
    return { cls: this.cls, longTasks: this.longTasks };
  }

  private flush(): void {
    this.callback?.({ cls: this.cls, longTasks: this.longTasks });
  }
}

/** Convenience factory: starts monitoring and returns a handle to stop it. */
export function monitorLayoutThrash(
  callback: MetricsCallback,
  options: { clsWarningThreshold?: number } = {},
): LayoutThrashMonitor {
  const { clsWarningThreshold = 0.1 } = options;

  const monitor = new LayoutThrashMonitor((metrics) => {
    if (metrics.cls >= clsWarningThreshold) {
      console.warn(`[LayoutThrash] CLS score ${metrics.cls.toFixed(4)} exceeds threshold ${clsWarningThreshold}`);
    }
    callback(metrics);
  });

  monitor.start();
  return monitor;
}
