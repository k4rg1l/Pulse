/** Bass, mids and treble, each from 0 to 1. */
export type Bands = [number, number, number];

/** How much of the line's height each band can move. Together they fill it. */
const BAND_SHARE: Bands = [0.62, 0.26, 0.12];

/** Waves across the line for each band: a slow swell, movement, a fine shimmer. */
const BAND_WAVES: Bands = [1.5, 3.7, 8.3];

/** How far each band's wave travels per second at rest, and more with its energy. */
const BAND_DRIFT: Bands = [1.6, 2.4, 4];
const BAND_SURGE: Bands = [3, 4, 6];

/** How sharply the line narrows to nothing at its ends, as the logo's heartbeat does. */
const END_SHAPE = 0.6;

/** Moves each band's wave along, faster while that band is loud. */
export function advancePhases(phases: Bands, bands: Bands, seconds: number) {
  for (let band = 0; band < 3; band++) {
    phases[band] += seconds * (BAND_DRIFT[band] + bands[band] * BAND_SURGE[band]);
  }
}

/**
 * Draws the sound as a designed curve rather than its raw shape: three layered
 * waves, whose sizes are the real bass, mids and treble. It can't go jagged however
 * the music moves. `seed` gives each line a shape of its own.
 */
export function fillWave(line: Float32Array, phases: Bands, bands: Bands, seed: number) {
  const count = line.length;
  for (let index = 0; index < count; index++) {
    const position = index / (count - 1);
    const across = position * Math.PI * 2;
    const narrowing = Math.sin(position * Math.PI) ** END_SHAPE;
    let height = 0;
    for (let band = 0; band < 3; band++) {
      const waves = BAND_WAVES[band] + seed * 0.13 * (band + 1);
      const direction = band === 1 ? -1 : 1;
      height +=
        Math.sin(across * waves + direction * phases[band] + seed) *
        bands[band] *
        BAND_SHARE[band];
    }
    line[index] = height * narrowing;
  }
}
