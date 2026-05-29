/**
 * Issue #629 — Stellar Background CSS Paint Worklet
 *
 * Registered via CSS.paintWorklet.addModule('/worklets/stellar-background-worklet.js').
 * Apply with: background: paint(stellar-background);
 *
 * Supported CSS custom properties (all optional with defaults):
 *   --bg-hue          <number>    Base hue of the gradient (0-360), default 250
 *   --bg-saturation   <number>    Saturation percentage (0-100), default 60
 *   --star-density    <number>    Star count per 10,000 px², default 4
 *   --glow-radius     <length>    Radial glow radius in px, default 80
 */

registerPaint('stellar-background', class {
  static get inputProperties() {
    return ['--bg-hue', '--bg-saturation', '--star-density', '--glow-radius'];
  }

  static get contextOptions() {
    return { alpha: true };
  }

  paint(ctx, geometry, properties) {
    const { width, height } = geometry;

    const hue = parseFloat(properties.get('--bg-hue')) || 250;
    const sat = parseFloat(properties.get('--bg-saturation')) || 60;
    const density = parseFloat(properties.get('--star-density')) || 4;
    const glowRadius = parseFloat(properties.get('--glow-radius')) || 80;

    // Deep-space gradient background
    const gradient = ctx.createLinearGradient(0, 0, 0, height);
    gradient.addColorStop(0, `hsl(${hue}, ${sat}%, 8%)`);
    gradient.addColorStop(0.5, `hsl(${hue + 20}, ${sat - 10}%, 14%)`);
    gradient.addColorStop(1, `hsl(${hue + 40}, ${sat - 20}%, 6%)`);

    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, width, height);

    // Radial nebula glow
    const centerX = width * 0.35;
    const centerY = height * 0.4;
    const radial = ctx.createRadialGradient(centerX, centerY, 0, centerX, centerY, glowRadius);
    radial.addColorStop(0, `hsla(${hue + 60}, 80%, 70%, 0.18)`);
    radial.addColorStop(0.5, `hsla(${hue}, 60%, 50%, 0.06)`);
    radial.addColorStop(1, 'hsla(0, 0%, 0%, 0)');

    ctx.fillStyle = radial;
    ctx.beginPath();
    ctx.arc(centerX, centerY, glowRadius, 0, Math.PI * 2);
    ctx.fill();

    // Stars — seeded deterministically from canvas dimensions to avoid repaints
    const area = (width * height) / 10_000;
    const starCount = Math.round(density * area);
    // Simple deterministic pseudo-random using LCG seeded from size
    let seed = (width * 31337 + height * 1234567) >>> 0;
    const rand = () => {
      seed = (seed * 1664525 + 1013904223) >>> 0;
      return (seed >>> 0) / 0xffffffff;
    };

    for (let i = 0; i < starCount; i++) {
      const x = rand() * width;
      const y = rand() * height;
      const r = rand() * 1.2 + 0.3;
      const alpha = rand() * 0.6 + 0.4;

      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fillStyle = `hsla(0, 0%, 100%, ${alpha})`;
      ctx.fill();
    }
  }
});
