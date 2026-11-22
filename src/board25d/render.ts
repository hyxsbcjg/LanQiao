import { KEY_MAP } from '../sim/keymap';
import type { BoardState } from '../sim/types';
import { dialAngle, digitOpacities, distancePosition, ledOpacities } from './model';
import type { BoardManifest, BoardRenderState, Part, Rect } from './types';
import { boardLayout, obstacleCrop, obstacleRect } from './layout';

export interface LoadedBoard {
  manifest: BoardManifest;
  images: Map<string, CanvasImageSource>;
}

// G1-board-controls.md: engine defaults, deliberately not the source model's J5=BTN.
const DEFAULT_JUMPERS: BoardState['jumpers'] = {
  J5: 'KBD', J13: 'IO', J6: 'ON', J2: 'US', J3: 'NE555_OFF',
};
const JUMPER_POSITIONS = {
  J5: ['KBD', 'BTN'], J13: ['IO', 'MM'], J6: ['ON', 'OFF'],
  J2: ['US', 'IR'], J3: ['NE555_ON', 'NE555_OFF'],
};

function requireValue(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`Invalid board25d: ${message}`);
}

function object(value: unknown): asserts value is Record<string, unknown> {
  requireValue(value !== null && typeof value === 'object' && !Array.isArray(value), 'expected object');
}

function dimensions(value: Record<string, unknown>): void {
  requireValue(Number.isSafeInteger(value.width) && Number(value.width) > 0 && Number(value.width) <= 3200
    && Number.isSafeInteger(value.height) && Number(value.height) > 0 && Number(value.height) <= 2222,
  'invalid image dimensions');
}

function rectangle(value: Record<string, unknown>): void {
  dimensions(value);
  requireValue(Number.isSafeInteger(value.x) && Number(value.x) >= 0
    && Number.isSafeInteger(value.y) && Number(value.y) >= 0
    && Number(value.x) + Number(value.width) <= 3200
    && Number(value.y) + Number(value.height) <= 2222, 'rectangle outside canvas');
}

// BOARD25D.md coordinate contract; same filename policy as the offline verifier.
function safeFile(value: unknown): asserts value is string {
  requireValue(typeof value === 'string' && value.endsWith('.webp') && value.split('/').every(part =>
    /^[a-z0-9_-](?:[a-z0-9._-]*[a-z0-9_-])?$/i.test(part)
    && !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part)), 'unsafe relative asset filename');
}

function point(value: unknown): asserts value is [number, number] {
  requireValue(Array.isArray(value) && value.length === 2 && value.every(Number.isFinite)
    && value[0] >= 0 && value[0] <= 3200 && value[1] >= 0 && value[1] <= 2222, 'invalid point');
}

function validateManifest(value: unknown): { manifest: BoardManifest; files: Map<string, Rect> } {
  object(value);
  requireValue(value.schemaVersion === 1 && value.width === 3200 && value.height === 2222,
    'expected schema 1, 3200x2222');
  requireValue(Number.isFinite(value.pxPerMm) && Number(value.pxPerMm) > 0
    && Number.isFinite(value.pxPerMmY) && Number(value.pxPerMmY) > 0, 'invalid projection dimensions');
  const files = new Map<string, Rect>();
  function image(sprite: Record<string, unknown>, field = 'file'): void {
    safeFile(sprite[field]);
    const file = sprite[field];
    const previous = files.get(file);
    requireValue(!previous || (previous.width === sprite.width && previous.height === sprite.height),
      `conflicting dimensions: ${file}`);
    files.set(file, { x: 0, y: 0, width: Number(sprite.width), height: Number(sprite.height) });
  }
  object(value.base);
  dimensions(value.base); // Base x/y are intentionally absent in the shipped manifest.
  requireValue(value.base.width === 3200 && value.base.height === 2222, 'invalid base dimensions');
  image(value.base);
  requireValue(Array.isArray(value.base.variants) && value.base.variants.length === 2, 'expected both base variants');
  const widths = new Set<number>();
  for (const variant of value.base.variants) {
    object(variant);
    dimensions(variant);
    requireValue((variant.width === 1600 || variant.width === 3200)
      && variant.height === Number(variant.width) * 2222 / 3200 && !widths.has(variant.width), 'invalid base variant');
    requireValue(variant.width !== 3200 || variant.file === value.base.file, 'duplicate full-size base');
    widths.add(variant.width);
    image(variant);
  }

  const required = new Map<string, Part['type']>([
    ['S1', 'key'], ['S2', 'power'], ...KEY_MAP.map(key => [key.v2, 'key'] as [string, 'key']),
    ...Object.keys(DEFAULT_JUMPERS).map(id => [id, 'jumper'] as [string, 'jumper']),
    ...['RB1', 'RB2', 'RB3', 'RB4'].map(id => [id, 'dial'] as [string, 'dial']),
    ['RD1', 'ldr'], ['U5', 'thermo'], ['JS1', 'ultrasonic'], ['JS2', 'ultrasonic'], ['obstacle', 'obstacle'],
  ]);
  for (let digit = 1; digit <= 8; digit++) {
    for (const seg of [...'abcdefg', 'dp']) required.set(`digit${digit}.${seg}`, 'segment');
  }
  for (let led = 1; led <= 12; led++) required.set(`L${led}`, 'led');
  requireValue(Array.isArray(value.items), 'expected items array');
  for (const item of value.items) {
    object(item);
    requireValue(typeof item.id === 'string' && required.has(item.id) && required.get(item.id) === item.type,
      'missing, duplicate or invalid item type/ID');
    required.delete(item.id);
    requireValue(typeof item.label === 'string' && typeof item.engineId === 'string', 'invalid item labels');
    rectangle(item);
    if (item.hitPolygon !== undefined) {
      requireValue(Array.isArray(item.hitPolygon) && item.hitPolygon.length >= 3, 'invalid hit polygon');
      item.hitPolygon.forEach(point);
    }
    if (item.file !== undefined) image(item);
    if (item.pressedFile !== undefined) image(item, 'pressedFile');
    if (item.type === 'key' || item.type === 'power') safeFile(item.pressedFile);
    if (['dial', 'obstacle', 'segment', 'led'].includes(String(item.type))) safeFile(item.file);
    if (item.type === 'jumper') {
      object(item.positions);
      const positions = JUMPER_POSITIONS[item.id as keyof typeof JUMPER_POSITIONS];
      requireValue(Object.keys(item.positions).length === positions.length, 'invalid jumper positions');
      for (const position of positions) {
        const sprite = item.positions[position];
        object(sprite);
        rectangle(sprite);
        image(sprite);
      }
    }
    if (item.type === 'dial' || item.type === 'obstacle') {
      point([item.centerX, item.centerY]);
      requireValue(Number(item.centerX) >= Number(item.x) && Number(item.centerX) <= Number(item.x) + Number(item.width)
        && Number(item.centerY) >= Number(item.y) && Number(item.centerY) <= Number(item.y) + Number(item.height),
      'sprite center outside rectangle');
    }
    if (item.type === 'dial') {
      requireValue([item.minDeg, item.maxDeg, item.modelDeg].every(Number.isFinite)
        && Number(item.minDeg) < Number(item.maxDeg) && item.spriteDeg === 0, 'invalid dial angles');
    }
    if (item.type === 'segment') {
      const match = /^digit([1-8])\.(a|b|c|d|e|f|g|dp)$/.exec(item.id)!;
      requireValue(item.digit === Number(match[1]) && item.bit === [...'abcdefg', 'dp'].indexOf(match[2]),
        'invalid segment binding');
    }
    if (item.type === 'led') requireValue(item.index === Number(item.id.slice(1)) - 1, 'invalid LED binding');
    if (item.type === 'obstacle') {
      object(item.track);
      point(item.track.start);
      point(item.track.end);
      requireValue(Math.hypot(item.track.end[0] - item.track.start[0], item.track.end[1] - item.track.start[1]) > 0,
        'empty obstacle track');
    }
  }
  requireValue(required.size === 0, `missing required items: ${[...required.keys()].join(', ')}`);
  return { manifest: value as unknown as BoardManifest, files };
}

function surface(width: number, height: number): { canvas: HTMLCanvasElement; context: CanvasRenderingContext2D } {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Canvas 2D is unavailable');
  return { canvas, context };
}

function loadImage(url: URL, size: Rect, signal: AbortSignal): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    signal.throwIfAborted();
    const image = new Image();
    image.crossOrigin = 'anonymous';
    const abort = () => {
      image.removeAttribute('src');
      reject(signal.reason);
    };
    signal.addEventListener('abort', abort, { once: true });
    image.src = url.href;
    image.decode().then(() => {
      signal.throwIfAborted();
      requireValue(image.naturalWidth === size.width && image.naturalHeight === size.height,
        `decoded dimensions disagree: ${url.pathname}`);
      resolve(image);
    }).catch(reject).finally(() => signal.removeEventListener('abort', abort));
  });
}

export async function loadBoard(url: string, signal: AbortSignal): Promise<LoadedBoard> {
  signal.throwIfAborted();
  const controller = new AbortController();
  const abort = () => controller.abort(signal.reason);
  signal.addEventListener('abort', abort, { once: true });
  try {
    const response = await fetch(url, { signal: controller.signal, cache: 'no-cache' });
    if (!response.ok) throw new Error(`Board manifest HTTP ${response.status}`);
    const { manifest, files } = validateManifest(await response.json());
    const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(JSON.stringify(manifest)));
    const version = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
    controller.signal.throwIfAborted();
    const manifestUrl = new URL(response.url || url, document.baseURI);
    const images = new Map<string, CanvasImageSource>();
    await Promise.all([...files].map(async ([file, size]) => {
      const imageUrl = new URL(file, manifestUrl);
      imageUrl.searchParams.set('v', version);
      images.set(file, await loadImage(imageUrl, size, controller.signal));
    }));
    signal.throwIfAborted();
    // Preload only: turn the L1..L8 masks red without changing their packed alpha/bloom.
    // P3 extends the tint to all discrete LEDs. Warm red retains luminance headroom;
    // source-over lens compositing avoids additive channel clipping at high PWM duty.
    const tinted = new Set<string>();
    for (const part of manifest.items) {
      if (part.type !== 'led' || tinted.has(part.file!)) continue;
      const { canvas, context } = surface(part.width, part.height);
      context.drawImage(images.get(part.file!)!, 0, 0);
      const rgba = context.getImageData(0, 0, part.width, part.height);
      for (let i = 0; i < rgba.data.length; i += 4) {
        const peak = Math.max(rgba.data[i], rgba.data[i + 1], rgba.data[i + 2]);
        // Additive assets contain opaque black. Move intensity into coverage so those
        // pixels become transparent under source-over rather than black package-sized boxes.
        rgba.data[i + 3] *= peak / 255;
        rgba.data[i] = 240;
        rgba.data[i + 1] = 92;
        rgba.data[i + 2] = 56;
      }
      context.putImageData(rgba, 0, 0);
      images.set(part.file!, canvas);
      tinted.add(part.file!);
    }
    signal.throwIfAborted(); // An obsolete caller must never receive a completed board.
    return { manifest, images };
  } catch (error) {
    controller.abort(error); // Cancel sibling decodes on failure as well as caller abort.
    throw error;
  } finally {
    signal.removeEventListener('abort', abort);
  }
}

interface RenderCache {
  loaded: LoadedBoard;
  key: string;
  entity: ReturnType<typeof surface>;
}
const caches = new WeakMap<HTMLCanvasElement, RenderCache>();

function drawSprite(context: CanvasRenderingContext2D, loaded: LoadedBoard, file: string, rect: Rect): void {
  const image = loaded.images.get(file);
  if (!image) throw new Error(`Board image is missing: ${file}`);
  context.drawImage(image, rect.x, rect.y, rect.width, rect.height);
}

function drawEntities(context: CanvasRenderingContext2D, loaded: LoadedBoard, state: BoardRenderState,
  resolutionWidth: number): void {
  const { manifest } = loaded;
  const variant = manifest.base.variants.find(base => base.width === (resolutionWidth <= 1600 ? 1600 : 3200))!;
  drawSprite(context, loaded, variant.file, { x: 0, y: 0, width: manifest.width, height: manifest.height });
  const jumpers = state.board?.jumpers ?? DEFAULT_JUMPERS;
  for (const part of manifest.items.filter(part => part.type === 'jumper')) {
    const position = jumpers[part.id as keyof typeof jumpers];
    const sprite = part.positions?.[position];
    if (!sprite) throw new Error(`Invalid board jumper position: ${part.id}/${position}`);
    // Short lift is purely mechanical feedback; electrical state still comes from the engine.
    const lift = state.jumperMotion === part.id ? -6 : 0;
    context.clearRect(sprite.x, sprite.y + lift, sprite.width, sprite.height);
    drawSprite(context, loaded, sprite.file, { ...sprite, y: sprite.y + lift });
  }
  for (const part of manifest.items.filter(part => part.type === 'key' || part.type === 'power')) {
    // pressed stores local V2 IDs, not engine IDs (S6 and engine S5 share one physical key).
    if (!state.pressed.has(part.id)) continue;
    context.clearRect(part.x, part.y, part.width, part.height);
    drawSprite(context, loaded, part.pressedFile!, part);
  }
  for (const part of manifest.items.filter(part => part.type === 'dial')) {
    const position = part.id === 'RB2' ? state.board?.dials.RB2 ?? 0
      : part.id === 'RB3' ? state.board?.dials.RB3 ?? 0.5 : null;
    const degrees = position === null ? part.modelDeg! : dialAngle(position, part.minDeg, part.maxDeg);
    context.save();
    context.translate(part.centerX!, part.centerY!);
    // BOARD25D.md: world +Z opposes image rotation; use source origin, not crop midpoint.
    context.rotate(-degrees * Math.PI / 180);
    drawSprite(context, loaded, part.file!, { ...part, x: part.x - part.centerX!, y: part.y - part.centerY! });
    context.restore();
  }
  const obstacle = boardLayout(manifest).obstacle;
  if (state.distance !== null) {
    const rect = obstacleRect(obstacle, state.distance);
    context.drawImage(loaded.images.get(obstacle.file!)!, obstacleCrop.x, obstacleCrop.y,
      obstacleCrop.width, obstacleCrop.height, rect.x, rect.y, rect.width, rect.height);
  }
}

function drawTrack(context: CanvasRenderingContext2D, obstacle: Part, color: string): void {
  const { start, end } = obstacle.track!;
  context.save();
  context.strokeStyle = color;
  context.fillStyle = color;
  context.lineWidth = 1.5;
  context.font = '28px system-ui, sans-serif';
  context.textAlign = 'center';
  context.textBaseline = 'middle';
  context.beginPath();
  const rulerY = obstacle.y + obstacle.height + 16;
  context.moveTo(start[0], rulerY);
  context.lineTo(end[0], rulerY);
  // Visual ruler only: linear 0..50cm occupies 55%, log 50..400cm the remaining 45%.
  // BOARD25D.md forbids a world-scale claim; the transducers point out of this image plane.
  for (const cm of [0, 25, 50, 100, 200, 400]) {
    const t = distancePosition(cm);
    const x = start[0] + (end[0] - start[0]) * t;
    context.moveTo(x, rulerY);
    context.lineTo(x, rulerY + 8);
    context.fillText(`${cm} cm`, x, rulerY + 26);
  }
  context.stroke();
  context.restore();
}

export function drawBoard(canvas: HTMLCanvasElement, loaded: LoadedBoard, state: BoardRenderState,
  resolutionWidth: number): void {
  requireValue(Number.isSafeInteger(resolutionWidth) && resolutionWidth > 0,
    'resolutionWidth must be a positive integer');
  const layout = boardLayout(loaded.manifest);
  const height = Math.round(resolutionWidth * layout.height / layout.width);
  if (canvas.width !== resolutionWidth) canvas.width = resolutionWidth;
  if (canvas.height !== height) canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Canvas 2D is unavailable');
  let cache = caches.get(canvas);
  if (!cache || cache.loaded !== loaded || cache.entity.canvas.width !== canvas.width
    || cache.entity.canvas.height !== canvas.height) {
    cache = { loaded, key: '', entity: surface(canvas.width, canvas.height) };
    caches.set(canvas, cache);
  }
  const key = JSON.stringify([state.board?.jumpers ?? DEFAULT_JUMPERS, state.board?.dials ?? [0, 0.5],
    [...state.pressed].sort(), state.distance, state.jumperMotion]);
  const scaleX = canvas.width / loaded.manifest.width;
  const scaleY = canvas.height / layout.height;
  if (cache.key !== key) {
    const entity = cache.entity.context;
    entity.setTransform(1, 0, 0, 1, 0, 0);
    entity.clearRect(0, 0, canvas.width, canvas.height);
    entity.setTransform(scaleX, 0, 0, scaleY, 0, layout.offsetY * scaleY);
    drawEntities(entity, loaded, state, resolutionWidth);
    cache.key = key;
  }
  const snapshot = state.snapshot;
  const powered = snapshot != null && snapshot.state !== 'empty';
  const digits = digitOpacities(powered ? snapshot.brightness : undefined,
    powered ? snapshot.digits : Array(8).fill(null));
  const leds = ledOpacities(powered ? snapshot.brightness : undefined,
    powered ? snapshot.leds : Array(8).fill(false));
  canvas.dataset.segmentPeak = String(Math.max(0, ...digits.flat()));
  canvas.dataset.ledPeak = String(Math.max(0, ...leds));
  context.save();
  const frame = context;
  frame.setTransform(1, 0, 0, 1, 0, 0);
  frame.globalAlpha = 1;
  frame.globalCompositeOperation = 'source-over';
  frame.clearRect(0, 0, canvas.width, canvas.height);
  frame.drawImage(cache.entity.canvas, 0, 0);
  frame.setTransform(scaleX, 0, 0, scaleY, 0, layout.offsetY * scaleY);
  const cssScale = (canvas.getBoundingClientRect().width || canvas.width) / loaded.manifest.width;
  const emissions: { part: Part; opacity: number; haloRadius: number }[] = [];
  // Measured on-time stays authoritative; model.ts applies perceptual curves without temporal smoothing.
  for (const part of loaded.manifest.items) {
    if (!powered) continue;
    let opacity = 0;
    if (part.type === 'segment') opacity = digits[part.digit! - 1]?.[part.bit!] ?? 0;
    if (part.type === 'led' && part.index! < 8) opacity = leds[part.index!] ?? 0;
    // L9 is the always-on supply indicator.
    if (part.id === 'L9') opacity = Number(state.board !== null && powered);
    // L10.2=VB; L10.1 -> R24 -> M2 -> K1 contact/GND (706..714, 1104..1123, 1748..1756).
    // R3 is RST/GND, not N_RELAY. The relay snapshot alone drives this contact indicator.
    // VB is present only with J6 ON; see G1-board-controls.md (J6.1=VB, J6.2=VCC).
    if (part.id === 'L10') opacity = Number(snapshot?.relay && state.board?.jumpers.J6 === 'ON');
    // L11/L12 lack snapshot bindings. Motor and buzzer get no invented board emission.
    if (opacity <= 0) continue;
    emissions.push({ part, opacity,
      haloRadius: part.type === 'led' ? Math.max(part.width * 0.7, 6 / cssScale) : 0 });
  }
  frame.save();
  frame.beginPath();
  // Only mask emission rectangles. Remasking the whole board would square its antialiased edge alpha.
  for (const { part, haloRadius } of emissions) {
    frame.rect(part.x, part.y, part.width, part.height);
    if (haloRadius) frame.rect(part.x + part.width / 2 - haloRadius, part.y + part.height / 2 - haloRadius,
      haloRadius * 2, haloRadius * 2);
  }
  frame.clip();
  frame.globalCompositeOperation = 'lighter';
  for (const { part, opacity, haloRadius } of emissions) {
    frame.globalCompositeOperation = 'lighter';
    frame.globalAlpha = opacity;
    if (haloRadius) {
      const x = part.x + part.width / 2, y = part.y + part.height / 2;
      // P3 visual policy: >=6 CSS px soft spill, proportional to the same measured opacity.
      // Edge alpha falls to zero before adjacent silkscreen labels; no temporal smoothing.
      const glow = frame.createRadialGradient(x, y, 0, x, y, haloRadius);
      // Leave the lens core to the source-over sprite; additive spill must not lift
      // its dark baseline and flatten the 90%..100% contrast again.
      glow.addColorStop(0, 'rgba(255, 48, 24, 0)');
      glow.addColorStop(0.3, 'rgba(255, 48, 24, 0)');
      glow.addColorStop(0.5, 'rgba(255, 48, 24, 0.65)');
      glow.addColorStop(0.75, 'rgba(255, 48, 24, 0.18)');
      glow.addColorStop(1, 'rgba(255, 48, 24, 0)');
      frame.fillStyle = glow;
      frame.fillRect(x - haloRadius, y - haloRadius, haloRadius * 2, haloRadius * 2);
      frame.globalCompositeOperation = 'source-over';
      // Linear PWM opacity drives the halo. Squared lens coverage expands high-end
      // contrast without V1's compressed low-duty mapping or saturated additive red.
      frame.globalAlpha = opacity ** 2;
    }
    drawSprite(frame, loaded, part.file!, part);
  }
  frame.globalAlpha = 1;
  frame.setTransform(1, 0, 0, 1, 0, 0);
  frame.globalCompositeOperation = 'destination-in';
  frame.drawImage(cache.entity.canvas, 0, 0); // Mask includes movables and the translated obstacle, not just base.
  frame.restore();
  // Put annotations behind entity alpha without another full-frame intermediate/copy.
  frame.globalCompositeOperation = 'destination-over';
  context.setTransform(scaleX, 0, 0, scaleY, 0, layout.offsetY * scaleY);
  const trackColor = typeof getComputedStyle === 'function'
    ? getComputedStyle(canvas).getPropertyValue('--b25-muted').trim() : '';
  drawTrack(context, layout.obstacle, trackColor || '#c2ced8');
  context.restore();
  // Tag the completed draw, not the independently committed toolbar DOM time.
  canvas.dataset.timeNs = String(snapshot?.timeNs ?? 0);
}
