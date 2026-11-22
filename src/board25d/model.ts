import type { Brightness } from '../sim/types';

export function clamp(value: number, min = 0, max = 1): number {
  return Number.isNaN(value) ? min : Math.min(max, Math.max(min, value));
}

function validFraction(value: number | undefined): value is number {
  return value !== undefined && Number.isFinite(value) && value >= 0 && value <= 1;
}

export function perceptualOpacity(fraction: number, ghost = false): number {
  const duty = clamp(fraction);
  if (duty === 0) return 0;
  // P2/V1 visual policy, not an electrical model: main segments retain the 0.6 floor
  // and sqrt duty curve (10/50/90% -> 0.726/0.883/0.979).
  // Ghosts retain the fourth-root toe and 0.45 cap, with a V1 0.22 visibility floor.
  return ghost ? Math.max(0.22, 0.45 * (Math.min(duty, 0.2) / 0.2) ** 0.25) : 0.6 + 0.4 * Math.sqrt(duty);
}

export function digitOpacities(brightness: Brightness | undefined, digits: (number | null)[]): number[][] {
  const measured = brightness && Number.isFinite(brightness.windowNs) && brightness.windowNs > 0;
  const fractions = digits.map((_, digit) => Array.from({ length: 8 }, (_, bit) =>
    measured ? brightness.seg?.[digit]?.[bit] : undefined));
  const peaks = fractions.map(row => Math.max(0, ...row.map(value => validFraction(value) ? value : 0)));
  const maxPeak = Math.max(0, ...peaks);
  // PLAN.md §2: integrated fractions measure physical on-time, not cached segment bytes.
  // Estimate scan width from nonblank digits with nonzero peaks >= 15% of the largest peak.
  // Clamp N to 1..8. Stale digit patterns and faint ghost rows must not inflate a slow scan.
  // display.rs sample_digits records selected blank COM slots as 0xff, distinct from unseen null.
  // Count these only with complete measurements and zero/ghost peaks below the scan threshold.
  // The 0.001 ceiling is a conservative visual policy (33us/33ms), not a hardware timing limit.
  const active = clamp(peaks.filter((peak, digit) => digits[digit] !== null && (digits[digit] === 0xff
    ? fractions[digit].every(validFraction) && peak <= 0.001 && (peak === 0 || peak < maxPeak * 0.15)
    : peak > 0 && peak >= maxPeak * 0.15)).length, 1, 8);
  // Normalize measured fraction*N, never divide by peak. Apply only a perceptual curve afterward.
  // Invalid/missing samples and zero windows use the active-low pattern fallback (paused snapshots).
  // BOARD25D.md "数码管和 LED": a..g,dp = bits 0..7; null and 0xff are blank.
  return fractions.map((row, digit) => row.map((fraction, bit) => {
    const patternLit = digits[digit] !== null && !(digits[digit]! & (1 << bit));
    // A missing pattern bit or a sub-threshold row identifies ghost light, not intended PWM.
    // Cached patterns choose the curve only; measured zero always wins, including slow-scan gaps.
    return validFraction(fraction)
      ? perceptualOpacity(fraction * active, !patternLit || peaks[digit] < maxPeak * 0.15)
      : Number(patternLit);
  }));
}

export function ledOpacities(brightness: Brightness | undefined, leds: boolean[]): number[] {
  const measured = brightness && Number.isFinite(brightness.windowNs) && brightness.windowNs > 0;
  // LEDs are not multiplexed. V1 visual policy uses a 0.4 floor and quadratic duty
  // to separate 90% from 100% before the additive red sprite saturates; zero stays dark.
  // P3 replaces that workaround with evenly spaced opacity; render.ts keeps lens headroom.
  return leds.map((lit, index) => {
    const fraction = measured ? brightness.led?.[index] : undefined;
    return validFraction(fraction) ? (fraction === 0 ? 0 : 0.35 + 0.65 * fraction) : Number(lit);
  });
}

export function thermalStep(current: number, target: number, dtSeconds: number, heating: boolean): number {
  // G3 interaction model: phenomenological first-order lag, not a DS18B20 datasheet response promise.
  // Heating uses tau=20s; cooling uses tau=40s. Negative/NaN elapsed time leaves temperature unchanged.
  return current + (target - current) * -Math.expm1(-clamp(dtSeconds, 0, Infinity) / (heating ? 20 : 40));
}

// BOARD25D.md "旋钮": +/-135 degrees is visual travel, not calibrated mechanical travel.
export function dialAngle(pos01: number, minDeg = -135, maxDeg = 135): number {
  return minDeg + clamp(pos01) * (maxDeg - minDeg);
}

export function anglePosition(angle: number, minDeg = -135, maxDeg = 135): number {
  return minDeg === maxDeg ? 0 : clamp((angle - minDeg) / (maxDeg - minDeg));
}

export function rb3Frequency(position: number): number {
  // G1-board-controls.md "名义模型与校准边界": output-feedback 555, R8=100ohm, RB3=10kohm, C14=100nF.
  return 1 / (2 * Math.LN2 * (100 + 10_000 * clamp(position)) * 100e-9);
}

export function rb3Position(hz: number): number {
  const frequency = clamp(hz, rb3Frequency(1), rb3Frequency(0));
  return clamp((1 / (2 * Math.LN2 * frequency * 100e-9) - 100) / 10_000);
}

// G3 visual track design, not a pixel-to-distance calibration (BOARD25D.md "传感器与障碍物").
// 0..50cm takes 55% linearly; 50..400cm takes 45% logarithmically, continuous at 50cm.
export function distancePosition(cm: number): number {
  const distance = clamp(cm, 0, 400);
  return distance <= 50 ? distance / 50 * 0.55 : 0.55 + Math.log(distance / 50) / Math.log(8) * 0.45;
}

export function positionDistance(t01: number): number {
  const position = clamp(t01);
  if (position === 1) return 400;
  return position <= 0.55 ? position / 0.55 * 50 : 50 * 8 ** ((position - 0.55) / 0.45);
}

export function distanceInput(cm: number | null): number {
  // ultrasonic.rs saturates this finite f32 distance's echo delay to u64::MAX nanoseconds.
  // ponytail: no echo for the engine's ~584-year time horizon; use a native absence API if that horizon changes.
  return cm === null ? 3.4028234663852886e38 : clamp(cm, 0, 400);
}
