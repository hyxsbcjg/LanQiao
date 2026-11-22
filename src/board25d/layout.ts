import { distancePosition } from './model';
import type { BoardManifest, Part, Rect } from './types';

// Shipped sprite alpha > 128 spans x=48..237, y=17..113. Retain a 4px shadow margin.
// This runtime crop preserves the packed WebP; the compact card is illustrative, not world-scale.
export const obstacleCrop: Rect = { x: 44, y: 13, width: 198, height: 105 };

export function boardLayout(manifest: BoardManifest) {
  const source = manifest.items.find(part => part.type === 'obstacle')!;
  // P1 visual sizing: a horizontal strip in the unused margin above JS1/JS2 (y=124).
  // The 8..116px strip is < 5% of board height; it never expands the board-only fit bounds.
  // Keep packed crops/pivots intact. Runtime coordinates remain in board space;
  // the canvas and HTML hits both add offsetY, and pointer input subtracts it.
  const scale = 52 / obstacleCrop.height;
  const width = obstacleCrop.width * scale, height = obstacleCrop.height * scale;
  const offsetY = 0;
  const transducers = manifest.items.filter(part => part.type === 'ultrasonic');
  const left = Math.min(...transducers.map(part => part.x));
  const right = Math.max(...transducers.map(part => part.x + part.width));
  const anchorX = (source.centerX! - source.x - obstacleCrop.x) * scale;
  const anchorY = (source.centerY! - source.y - obstacleCrop.y) * scale;
  const startX = left + anchorX, endX = right - width + anchorX, y = 8 + anchorY;
  const obstacle: Part = { ...source, x: left, y: 8, width, height,
    centerX: startX, centerY: y, track: { start: [startX, y], end: [endX, y] } };
  return { width: manifest.width, height: manifest.height + offsetY, offsetY, obstacle };
}

export function obstacleRect(part: Part, distance: number | null): Rect {
  const t = distancePosition(distance ?? 100), { start, end } = part.track!;
  return { x: part.x + start[0] + (end[0] - start[0]) * t - part.centerX!,
    y: part.y + start[1] + (end[1] - start[1]) * t - part.centerY!,
    width: part.width, height: part.height };
}
