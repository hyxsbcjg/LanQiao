export type KeyId = 'S4'|'S5'|'S6'|'S7'|'S8'|'S9'|'S10'|'S11'|'S12'|'S13'|'S14'|'S15'|'S16'|'S17'|'S18'|'S19'; // engine = official CT107D ids
export type KeyMode = 'KBD' | 'BTN';
export type JumperId = 'J5' | 'J13' | 'J6' | 'J2' | 'J3';
export type DialId = 'RB2' | 'RB3';
export interface BoardState {
  jumpers: Record<JumperId, string>;
  dials: Record<DialId, number>;
  light: number;
  derived: {
    rb3ResistanceOhm: number; ldrResistanceOhm: number;
    ain1V: number; ain3V: number; ne555Hz: number;
  };
  ain1V: number; ain3V: number; ne555Hz: number;
}
export type RunState = 'empty' | 'paused' | 'running' | 'error';
export interface FileInfo { name: string; bytes: number; programBytes: number; minAddress: number; maxAddress: number; sha256: string }
export interface Brightness { windowNs: number; seg: number[][]; led: number[] }
export interface RelayEvent { tNs: number; on: boolean }   // tNs: absolute virtual time, same clock as Snapshot.timeNs
export interface DebugState {
  pc: number; a: number; b: number; psw: number; sp: number; dptr: number; bank: number;
  flags: { cy: boolean; ac: boolean; f0: boolean; rs1: boolean; rs0: boolean; ov: boolean; f1: boolean; p: boolean };
  r: number[];
  iram: number[];
  sfr: { address: number; name: string; value: number }[];
  ports: { name: string; latch: number; pins: number }[];
}
export interface Snapshot {
  ircHz: number;
  session: number; state: RunState; timeNs: number; speed: number; pc: number;
  digits: (number | null)[];   // 8 raw segment bytes, common anode: bit0..7 = a,b,c,d,e,f,g,dp, bit=0 lit; null = blank
  leds: boolean[];             // L1..L8
  relay: boolean; motor: boolean; buzzer: boolean;
  relayEvents?: RelayEvent[];      // Every raw U9/Y5 Q4 change in [previous timeNs, timeNs], ordered by tNs (paused reset can equal previous).
  relayEventsDropped?: number;     // Further events beyond the first 256 in this snapshot.
  diagnostic: string | null;
  brightness?: Brightness;
  board?: BoardState;          // Worker sends changes; controllers retain the latest board within a session.
  debug?: DebugState;
  uart?: { fwBaud: number | null; termBaud: number };
  recovery?: 'wasm' | 'worker' | 'recovering';
  notice?: string;
}
export interface Env { temperatureC: number; ain1V: number; ain3V: number; ne555Hz: number; distanceCm: number; ne555Jumper: boolean; lightLevel?: number }
export interface SimController {
  setIrcHz(hz: number): void;
  load(name: string, bytes: ArrayBuffer): Promise<FileInfo>;   // rejects with a user-facing message; keeps old session on failure
  run(): void; pause(): void; step(): void; reset(kind: 'mcu' | 'power'): void;
  setSpeed(multiple: number): void;                                // 1 = realtime; 0 = fastest, wall-budget bounded
  setKey(id: KeyId, down: boolean): void; setKeyMode(mode: KeyMode): void;
  setJumper(id: JumperId, position: string): void;
  setDial(id: DialId, pos01: number): void; setLight(level01: number): void;
  setEnv(patch: Partial<Env>): void; setRtc(h: number, m: number, s: number): void;
  uartSend(text: string): void;                                   // Byte string: U+0000..U+00FF, one character per byte
  setUartBaud(baud: number): void;
  exportState(): Promise<string>; importState(text: string): Promise<void>;   // EEPROM + RTC, JSON text
  onSnapshot(cb: (s: Snapshot) => void): () => void;              // ~30 Hz while running
  onUart(cb: (text: string) => void): () => void;
  retry?(): void;
}
