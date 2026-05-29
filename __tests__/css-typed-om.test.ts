/**
 * Tests for Issue #629 — CSS Typed OM Theming System
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { DESIGN_TOKENS, applyTypedToken, readTypedToken, applyTheme } from '../lib/theme/css-typed-om';
import { LayoutThrashMonitor, monitorLayoutThrash } from '../lib/theme/layout-thrash-monitor';

// ── DESIGN_TOKENS ─────────────────────────────────────────────────────────────

describe('DESIGN_TOKENS', () => {
  it('exports canonical token names with unit types', () => {
    expect(DESIGN_TOKENS['--primary-hue']).toBe('number');
    expect(DESIGN_TOKENS['--blur-radius']).toBe('px');
    expect(DESIGN_TOKENS['--surface-opacity']).toBe('percent');
  });
});

// ── applyTypedToken / readTypedToken (CSS Typed OM unavailable path) ──────────

describe('applyTypedToken (fallback path)', () => {
  let el: HTMLElement;

  beforeEach(() => {
    el = document.createElement('div');
    document.body.appendChild(el);
    // Ensure CSS Typed OM is simulated as unavailable
    (el as any).attributeStyleMap = undefined;
  });

  it('falls back to setProperty when Typed OM is absent', () => {
    applyTypedToken(el, '--primary-hue', 250);
    expect(el.style.getPropertyValue('--primary-hue')).toBe('250');
  });

  it('stores numeric value as a string on the inline style', () => {
    applyTypedToken(el, '--blur-radius', 12);
    expect(el.style.getPropertyValue('--blur-radius')).toBe('12');
  });
});

describe('readTypedToken (fallback path)', () => {
  let el: HTMLElement;

  beforeEach(() => {
    el = document.createElement('div');
    document.body.appendChild(el);
  });

  it('returns null when the token is not set', () => {
    expect(readTypedToken(el, '--unknown-token')).toBeNull();
  });

  it('returns the numeric value when the token is set via inline style', () => {
    el.style.setProperty('--primary-hue', '180');
    // getComputedStyle reflects inline styles in JSDOM
    expect(readTypedToken(el, '--primary-hue')).toBe(180);
  });
});

// ── applyTheme ────────────────────────────────────────────────────────────────

describe('applyTheme', () => {
  it('schedules token application via rAF', () => {
    const rafSpy = vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
      cb(0);
      return 0;
    });
    applyTheme({ '--primary-hue': 200 });
    expect(rafSpy).toHaveBeenCalledOnce();
    rafSpy.mockRestore();
  });
});

// ── LayoutThrashMonitor ───────────────────────────────────────────────────────

describe('LayoutThrashMonitor', () => {
  it('starts with zero CLS and zero long tasks', () => {
    const monitor = new LayoutThrashMonitor();
    expect(monitor.getMetrics()).toEqual({ cls: 0, longTasks: 0 });
  });

  it('does not throw when PerformanceObserver is unavailable', () => {
    const orig = (global as any).PerformanceObserver;
    (global as any).PerformanceObserver = undefined;
    const monitor = new LayoutThrashMonitor();
    expect(() => monitor.start()).not.toThrow();
    (global as any).PerformanceObserver = orig;
  });

  it('stop() can be called before start() safely', () => {
    const monitor = new LayoutThrashMonitor();
    expect(() => monitor.stop()).not.toThrow();
  });
});

describe('monitorLayoutThrash', () => {
  it('returns a LayoutThrashMonitor instance', () => {
    const monitor = monitorLayoutThrash(() => {});
    expect(monitor).toBeInstanceOf(LayoutThrashMonitor);
    monitor.stop();
  });
});
