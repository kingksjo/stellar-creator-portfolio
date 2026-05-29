/**
 * Issue #629 — Dynamic Theming via CSS Typed OM
 *
 * Replaces raw string interpolation in Tailwind dynamic classes with
 * CSSStyleValue abstractions and registers Houdini paint worklets for
 * exotic gradient backgrounds.
 */

export type CSSUnit = 'px' | 'percent' | 'number' | 'em' | 'rem';

export interface TypedToken {
  name: string;
  unit: CSSUnit;
  value: number;
}

/** Map of canonical design tokens with their expected unit types. */
export const DESIGN_TOKENS: Record<string, CSSUnit> = {
  '--primary-hue': 'number',
  '--accent-hue': 'number',
  '--blur-radius': 'px',
  '--glow-spread': 'px',
  '--surface-opacity': 'percent',
  '--border-radius': 'px',
  '--spacing-base': 'px',
};

function isTypedOMSupported(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof (CSS as any).number === 'function' &&
    'attributeStyleMap' in HTMLElement.prototype
  );
}

function toCSSNumericValue(value: number, unit: CSSUnit): CSSNumericValue {
  switch (unit) {
    case 'px':
      return CSS.px(value);
    case 'percent':
      return CSS.percent(value);
    case 'em':
      return CSS.em(value);
    case 'rem':
      return CSS.rem(value);
    case 'number':
    default:
      return (CSS as any).number(value);
  }
}

/**
 * Apply a typed design token to an element using CSS Typed OM when available,
 * falling back to plain CSS custom property strings otherwise.
 */
export function applyTypedToken(
  element: HTMLElement,
  tokenName: string,
  value: number,
): void {
  if (isTypedOMSupported()) {
    const unit = DESIGN_TOKENS[tokenName] ?? 'number';
    try {
      element.attributeStyleMap.set(tokenName, toCSSNumericValue(value, unit));
      return;
    } catch {
      // fall through to string fallback
    }
  }
  // Fallback: plain string-based CSS variable
  element.style.setProperty(tokenName, String(value));
}

/**
 * Read back a typed design token from an element's computed styles.
 * Returns the raw numeric value or null when the token is not set.
 */
export function readTypedToken(element: HTMLElement, tokenName: string): number | null {
  if (isTypedOMSupported()) {
    try {
      const styleMap = element.computedStyleMap();
      const cssValue = styleMap.get(tokenName);
      if (cssValue && 'value' in cssValue) {
        return (cssValue as CSSUnitValue).value;
      }
    } catch {
      // fall through
    }
  }
  const raw = getComputedStyle(element).getPropertyValue(tokenName).trim();
  const parsed = parseFloat(raw);
  return isNaN(parsed) ? null : parsed;
}

/**
 * Apply a full token map to the document root in a single rAF to batch
 * writes and avoid layout thrashing.
 */
export function applyTheme(tokens: Record<string, number>): void {
  if (typeof window === 'undefined') return;
  requestAnimationFrame(() => {
    const root = document.documentElement;
    for (const [name, value] of Object.entries(tokens)) {
      applyTypedToken(root as unknown as HTMLElement, name, value);
    }
  });
}

/** Register a CSS Paint Worklet module by URL. No-ops when unavailable. */
export async function registerPaintWorklet(name: string, url: string): Promise<void> {
  if (typeof CSS === 'undefined' || !('paintWorklet' in CSS)) return;
  try {
    await (CSS as any).paintWorklet.addModule(url);
    console.debug(`[Theme] Paint worklet "${name}" registered.`);
  } catch (err) {
    console.warn(`[Theme] Paint worklet "${name}" failed to register:`, err);
  }
}

/**
 * Bootstrap the theming system: register the stellar background worklet and
 * register all design token custom properties with the browser so transitions
 * animate correctly.
 */
export async function initTheme(): Promise<void> {
  if (typeof window === 'undefined') return;

  await registerPaintWorklet('stellar-background', '/worklets/stellar-background-worklet.js');

  // Register custom properties for animated transitions (CSS Houdini)
  if ('registerProperty' in CSS) {
    const registrations: Array<{ name: string; syntax: string; inherits: boolean; initialValue: string }> = [
      { name: '--primary-hue', syntax: '<number>', inherits: true, initialValue: '250' },
      { name: '--accent-hue', syntax: '<number>', inherits: true, initialValue: '200' },
      { name: '--blur-radius', syntax: '<length>', inherits: true, initialValue: '8px' },
      { name: '--glow-spread', syntax: '<length>', inherits: true, initialValue: '0px' },
      { name: '--surface-opacity', syntax: '<percentage>', inherits: true, initialValue: '100%' },
    ];
    for (const reg of registrations) {
      try {
        (CSS as any).registerProperty(reg);
      } catch {
        // Property may already be registered — safe to ignore.
      }
    }
  }
}
