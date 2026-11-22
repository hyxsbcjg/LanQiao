import type { BoardState, KeyId, Snapshot } from '../sim/types';

export interface Board25DProps {
  manifestUrl?: string;
  inputEpoch?: number;
  targetSpeed?: number;
  snapshot: Snapshot | null;
  board: BoardState | null;
  roomTempC: number;
  ambientLight: number;
  onKey(id: KeyId, down: boolean): void;
  onJumper(id: 'J2' | 'J3' | 'J5' | 'J6' | 'J13', position: string): void;
  onDial(id: 'RB2' | 'RB3', position01: number): void;
  onLight(level01: number): void;
  onTemperature(c: number): void;
  onDistance(cm: number | null): void;
  onMcuReset(): void;
}

export interface Rect { x: number; y: number; width: number; height: number }
export interface Sprite extends Rect { file: string }
export interface Part extends Rect {
  id: string; label: string; engineId: string;
  type: 'key' | 'power' | 'jumper' | 'dial' | 'segment' | 'led' | 'ldr' | 'thermo' | 'ultrasonic' | 'obstacle';
  file?: string; pressedFile?: string;
  hitPolygon?: [number, number][];
  positions?: Record<string, Sprite>;
  defaultPosition?: string;
  centerX?: number; centerY?: number;
  minDeg?: number; maxDeg?: number; modelDeg?: number; spriteDeg?: number;
  digit?: number; bit?: number; index?: number; role?: string;
  simulationBinding?: string;
  track?: { start: [number, number]; end: [number, number] };
}

export interface BoardManifest {
  schemaVersion: number; width: number; height: number;
  pxPerMm: number; pxPerMmY: number;
  base: Sprite & { variants: { file: string; width: number; height: number }[] };
  items: Part[];
}

export interface BoardRenderState {
  board: BoardState | null;
  snapshot: Snapshot | null;
  pressed: ReadonlySet<string>;
  distance: number | null;
  light: number;
  warming: boolean;
  temperature: number;
  roomTempC: number;
  jumperMotion: string | null;
}
