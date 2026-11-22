import type { BoardState, Brightness, DebugState, DialId, Env, JumperId, KeyId, KeyMode, RelayEvent } from './types';
export type { BoardState, DialId, JumperId } from './types';
import { parseDebugState } from './debug';
import { DEFAULT_IRC_HZ, validateIrcHz } from './irc-clock';
import wasmUrl from '../generated/engine/lanqiao_web_bg.wasm?url';

export interface EngineSnapshot {
  ircHz: number;
  timeNs: number; pc: number; digits: (number | null)[]; leds: boolean[];
  relay: boolean; motor: boolean; buzzer: boolean; diagnostic: string | null;
  relayEvents?: RelayEvent[]; relayEventsDropped?: number;
  uart?: { fwBaud: number | null; termBaud: number };
}

export interface EngineApi {
  setIrcHz(hz: number): void;
  loadHex(text: string): void;
  reset(kind: 'mcu' | 'power'): void;
  runUs(us: number): void;
  step(): void;
  snapshot(drainRelayEvents?: boolean): EngineSnapshot; // Defaults to draining; false peeks without consuming relay events.
  debugState(): DebugState;
  takeBrightness(): Brightness;
  setKey(id: KeyId, down: boolean): void;
  setKeyMode(mode: KeyMode): void;
  setTemperatureC(v: number): void;
  setVoltage(channel: 'AIN1' | 'AIN3', v: number): void;
  setFrequencyHz(v: number): void;
  setDistanceCm(v: number): void;
  setNe555Jumper(on: boolean): void;
  setJumper(id: JumperId, position: string): void;
  setDial(id: DialId, pos01: number): void;
  setLight(level01: number): void;
  boardState(): string;
  setRtc(h: number, m: number, s: number): void;
  uartWrite(text: string): void;
  uartTake(): string;
  setUartBaud(baud: number): void;
  exportState(): string;
  importState(text: string): void;
  free?(): void;
}

interface WebSim extends Omit<EngineApi, 'snapshot' | 'takeBrightness' | 'debugState' | 'setIrcHz' | 'setUartBaud'> {
  snapshot(): string; peekSnapshot(): string; takeBrightness(): string; debugState(): string;
  set_irc_hz(hz: number): void;
  set_uart_terminal_baud(baud: number): void;
}
interface WasmModule { WebSim: new () => WebSim }

export async function loadWasmEngine(bytes?: BufferSource | WebAssembly.Module): Promise<EngineApi> {
  const module = await import('../generated/engine/lanqiao_web.js');
  if (bytes === undefined) await module.default({ module_or_path: wasmUrl });
  else module.initSync({ module: bytes });
  return adaptWasmEngine(module);
}

export function adaptWasmEngine(module: Pick<WasmModule, 'WebSim'>): EngineApi {
  let sim = new module.WebSim();
  const inputs = new Map<string, (target: WebSim) => void>();
  const input = (key: string, apply: (target: WebSim) => void) => {
    apply(sim);
    // Reinsert successful setters so overlapping physical/direct inputs replay chronologically.
    inputs.delete(key);
    inputs.set(key, apply);
  };
  // Validate loads on a candidate instance so a rejected firmware keeps the old board intact.
  return {
    setIrcHz: (hz) => { validateIrcHz(hz); input('irc', target => target.set_irc_hz(hz)); },
    loadHex(text) {
      const candidate = new module.WebSim();
      try {
        candidate.importState(sim.exportState());
        candidate.loadHex(text);
        for (const apply of inputs.values()) apply(candidate);
      } catch (error) { candidate.free?.(); throw error; }
      sim.free?.();
      sim = candidate;
    },
    reset: (kind) => sim.reset(kind), runUs: (us) => sim.runUs(us), step: () => sim.step(),
    snapshot: (drainRelayEvents = true) => JSON.parse(drainRelayEvents ? sim.snapshot() : sim.peekSnapshot()) as EngineSnapshot,
    debugState: () => parseDebugState(sim.debugState()),
    takeBrightness: () => JSON.parse(sim.takeBrightness()) as Brightness,
    setKey: (id, down) => input(id, target => target.setKey(id, down)),
    setKeyMode: (mode) => input('mode', target => target.setKeyMode(mode)),
    setTemperatureC: (v) => input('temperature', target => target.setTemperatureC(v)),
    setVoltage: (channel, v) => input(channel, target => target.setVoltage(channel, v)),
    setFrequencyHz: (v) => input('frequency', target => target.setFrequencyHz(v)),
    setDistanceCm: (v) => input('distance', target => target.setDistanceCm(v)),
    setNe555Jumper: (v) => input('jumper', target => target.setNe555Jumper(v)),
    setJumper: (id, position) => input(id === 'J5' ? 'mode' : id === 'J3' ? 'jumper' : id,
      target => target.setJumper(id, position)),
    setDial: (id, position) => input(id, target => target.setDial(id, position)),
    setLight: (level) => input('light', target => target.setLight(level)),
    boardState: () => sim.boardState(),
    setRtc: (h, m, s) => sim.setRtc(h, m, s),
    uartWrite: (text) => sim.uartWrite(text), uartTake: () => sim.uartTake(),
    setUartBaud: (baud) => { validateUartBaud(baud); input('uartBaud', target => target.set_uart_terminal_baud(baud)); },
    exportState: () => sim.exportState(), importState: (text) => sim.importState(text),
    free: () => sim.free?.(),
  };
}

const jumperPositions: Record<JumperId, readonly string[]> = {
  J5: ['KBD', 'BTN'], J13: ['IO', 'MM'], J6: ['ON', 'OFF'],
  J2: ['US', 'IR'], J3: ['NE555_OFF', 'NE555_ON'],
};

export function validateRange(value: number, max: number, min = 0): void {
  if (!Number.isFinite(value) || value < min || value > max) throw new Error(`Value must be finite and within ${min}..${max}`);
}

export function validateUartBaud(baud: number): void {
  if (!Number.isInteger(baud) || baud < 1 || baud > 0xffffffff) throw new Error('UART baud must be an integer within 1..4294967295');
}

export function validateJumper(id: JumperId, position: string): void {
  if (typeof id !== 'string' || !Object.hasOwn(jumperPositions, id) || !jumperPositions[id].includes(position)) throw new Error('Invalid jumper position');
}

export function validateDial(id: DialId, pos01: number): void {
  if (id !== 'RB2' && id !== 'RB3') throw new Error('Unknown dial');
  validateRange(pos01, 1);
}

export function validateEnv(patch: Partial<Env>): void {
  // Match the existing WebSim setter bounds in engine/crates/web/src/lib.rs.
  if (patch.temperatureC !== undefined) validateRange(patch.temperatureC, 125, -55);
  if (patch.ain1V !== undefined) validateRange(patch.ain1V, 5);
  if (patch.ain3V !== undefined) validateRange(patch.ain3V, 5);
  if (patch.ne555Hz !== undefined) validateRange(patch.ne555Hz, 3.4028234663852886e38);
  if (patch.distanceCm !== undefined) validateRange(patch.distanceCm, 3.4028234663852886e38);
  if (patch.ne555Jumper !== undefined && typeof patch.ne555Jumper !== 'boolean') throw new Error('Invalid NE555 jumper value');
  if (patch.lightLevel !== undefined) validateRange(patch.lightLevel, 1);
}

// Deterministic test double, not a hardware model and never a production fallback.
export class FakeEngine implements EngineApi {
  ircHz = DEFAULT_IRC_HZ;
  setIrcHz(hz: number): void { validateIrcHz(hz); this.ircHz = hz; this.reset('power'); }
  timeNs = 0;
  pc = 0;
  diagnostic: string | null = null;
  keys = new Map<KeyId, boolean>();
  keyMode: KeyMode = 'KBD';
  private jumpers: BoardState['jumpers'] = { J5: 'KBD', J13: 'IO', J6: 'ON', J2: 'US', J3: 'NE555_OFF' };
  private dials: BoardState['dials'] = { RB2: 0, RB3: 0.5 };
  private light = 0;
  env: Env = {
    temperatureC: 25, ain1V: this.derived.ain1V, ain3V: 0,
    ne555Hz: this.derived.ne555Hz, distanceCm: 100, ne555Jumper: false,
  };
  calls: string[] = [];
  private uart = '';
  private uartBaud = 9600;
  private brightnessNs = 0;
  private persistent = { eeprom: [] as number[], rtc: [0, 0, 0] };
  loadHex(text: string): void {
    if (!text.startsWith(':')) throw new Error('Invalid HEX');
    this.calls.push('load');
    this.reset('power');
  }
  reset(kind: 'mcu' | 'power'): void {
    this.calls.push(`reset:${kind}`);
    this.timeNs = this.pc = 0; this.diagnostic = null; this.uart = '';
    this.brightnessNs = 0;
  }
  runUs(us: number): void {
    if (this.diagnostic) return;
    this.calls.push(`run:${us}`);
    this.timeNs += us * 1000; this.pc = (this.pc + us) % 65536;
  }
  step(): void { this.calls.push('step'); this.runUs(1); }
  snapshot(): EngineSnapshot {
    const counter = Math.floor(this.timeNs / 1_000_000);
    return {
      ircHz: this.ircHz,
      timeNs: this.timeNs, pc: this.pc,
      uart: { fwBaud: null, termBaud: this.uartBaud },
      digits: Array.from({ length: 8 }, (_, i) => Math.floor(counter / 10 ** i) % 10),
      leds: Array.from({ length: 8 }, (_, i) => this.keys.get(`S${i + 4}` as KeyId) ?? false),
      relay: this.jumpers.J6 === 'ON' && this.env.ne555Jumper, motor: this.env.ain1V > 0,
      buzzer: this.jumpers.J6 === 'ON' && this.env.temperatureC > 30, diagnostic: this.diagnostic,
    };
  }
  debugState(): DebugState {
    return {
      pc: this.pc, a: 0, b: 0, psw: 0, sp: 7, dptr: 0, bank: 0,
      flags: { cy: false, ac: false, f0: false, rs1: false, rs0: false, ov: false, f1: false, p: false },
      r: Array(8).fill(0), iram: Array(256).fill(0),
      sfr: [{ name: 'ACC', address: 0xE0, value: 0 }],
      ports: Array.from({ length: 5 }, (_, index) => ({ name: `P${index}`, latch: 255, pins: 255 })),
    };
  }
  setKey(id: KeyId, down: boolean): void {
    if (!/^S(?:[4-9]|1[0-9])$/.test(id)) throw new Error('Unknown key');
    this.keys.set(id, down); this.calls.push(`key:${id}:${down}`);
  }
  takeBrightness(): Brightness {
    const windowNs = this.timeNs - this.brightnessNs;
    this.brightnessNs = this.timeNs;
    return {
      windowNs,
      seg: Array.from({ length: 8 }, (_, digit) => Array.from({ length: 8 }, (_, segment) => windowNs > 0 && segment === digit ? 0.125 : 0)),
      led: Array.from({ length: 8 }, (_, i) => windowNs > 0 && this.keys.get(`S${i + 4}` as KeyId) ? 1 : 0),
    };
  }
  setKeyMode(mode: KeyMode): void { this.setJumper('J5', mode); }
  setTemperatureC(v: number): void { this.env.temperatureC = v; }
  setVoltage(channel: 'AIN1' | 'AIN3', v: number): void {
    if (channel !== 'AIN1' && channel !== 'AIN3') throw new Error('Unknown voltage channel');
    validateRange(v, 5);
    this.env[channel === 'AIN1' ? 'ain1V' : 'ain3V'] = v;
  }
  setFrequencyHz(v: number): void { validateRange(v, 3.4028234663852886e38); this.env.ne555Hz = v; }
  setDistanceCm(v: number): void { this.env.distanceCm = v; }
  setNe555Jumper(on: boolean): void { this.setJumper('J3', on ? 'NE555_ON' : 'NE555_OFF'); }
  setJumper(id: JumperId, position: string): void {
    validateJumper(id, position);
    this.jumpers[id] = position;
    if (id === 'J5') this.keyMode = position as KeyMode;
    if (id === 'J3') this.env.ne555Jumper = position === 'NE555_ON';
  }
  setDial(id: DialId, pos01: number): void {
    validateDial(id, pos01);
    this.dials[id] = pos01;
    if (id === 'RB2') this.env.ain3V = this.derived.ain3V;
    else this.env.ne555Hz = this.derived.ne555Hz;
  }
  setLight(level01: number): void {
    validateRange(level01, 1);
    this.light = level01;
    this.env.lightLevel = level01;
    this.env.ain1V = this.derived.ain1V;
  }
  private get derived(): BoardState['derived'] {
    // Nominal G1 LDR curve; divider evidence: docs/research/notes/board_hardware.md, ADC section.
    const ldrResistanceOhm = this.light === 0 ? 500_000 : Math.min(500_000, 7500 * (10 / (500 * this.light)) ** 0.7);
    // V2 output-feedback RC: engine/crates/core/src/peripherals/board_inputs.rs documents drawing evidence.
    const rb3ResistanceOhm = 10_000 * this.dials.RB3;
    return {
      rb3ResistanceOhm, ldrResistanceOhm,
      ain1V: 5 * 10_000 / (10_000 + ldrResistanceOhm), ain3V: 5 * this.dials.RB2,
      ne555Hz: 1 / (2 * Math.LN2 * (100 + rb3ResistanceOhm) * 100e-9),
    };
  }
  boardState(): string {
    return JSON.stringify({
      jumpers: this.jumpers, dials: this.dials, light: this.light, derived: this.derived,
      ain1V: this.env.ain1V, ain3V: this.env.ain3V, ne555Hz: this.env.ne555Hz,
    } satisfies BoardState);
  }
  setRtc(h: number, m: number, s: number): void { this.persistent.rtc = [h, m, s]; }
  uartWrite(text: string): void {
    if ([...text].some(char => char.codePointAt(0)! > 255)) throw new Error('UART requires byte characters U+0000..U+00FF');
    this.calls.push(`uart:${text}`); this.uart += text;
  }
  uartTake(): string { const text = this.uart; this.uart = ''; return text; }
  setUartBaud(baud: number): void { validateUartBaud(baud); this.uartBaud = baud; }
  exportState(): string { return JSON.stringify(this.persistent); }
  importState(text: string): void {
    const value = JSON.parse(text) as typeof this.persistent;
    if (!Array.isArray(value.eeprom) || !Array.isArray(value.rtc) || value.rtc.length !== 3) throw new Error('Invalid persistent state');
    this.persistent = value;
  }
}
