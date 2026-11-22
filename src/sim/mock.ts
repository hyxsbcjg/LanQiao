import { FakeEngine } from './engine';
import { readIrcHz, saveIrcHz, validateIrcHz } from './irc-clock';
import type { BoardState, DialId, Env, FileInfo, JumperId, KeyId, KeyMode, SimController, Snapshot } from './types';

export const DEFAULT_ENV: Env = {
  temperatureC: 25, ain1V: 2.5, ain3V: 2.5, ne555Hz: 1000, distanceCm: 100, ne555Jumper: true,
};
// Segment polarity/order: SCH_V31.pdf p. 3, DS1/DS2 and U7 wiring.
export const SEGMENTS = [0xc0, 0xf9, 0xa4, 0xb0, 0x99, 0x92, 0x82, 0xf8, 0x80, 0x90];
const DEMO = '演示模式：不执行 HEX 指令；左四位为温度（°C），右四位为计数；按键临时改变末位。';

export function blankSnapshot(): Snapshot {
  return { ircHz: readIrcHz(), session: 0, state: 'empty', timeNs: 0, speed: 0, pc: 0, digits: Array(8).fill(null),
    leds: Array(8).fill(false), relay: false, motor: false, buzzer: false, diagnostic: DEMO };
}

// A standalone mock validator: R0 removes the old src/simulator parser.
function inspectHex(bytes: ArrayBuffer) {
  if (bytes.byteLength > 2_000_000) throw new Error('HEX 文件超过 2 MB，请选择较小的固件。');
  const lines = new TextDecoder('utf-8', { fatal: true }).decode(bytes).trim().split(/\r?\n/);
  const data = new Map<number, number>();
  let base = 0;
  let eof = false;
  for (const [index, raw] of lines.entries()) {
    const line = raw.trim();
    if (!line) continue;
    const fail = () => { throw new Error(`HEX 第 ${index + 1} 行格式、校验和或地址无效，请重新导出固件。`); };
    if (eof || !/^:(?:[\da-f]{2}){5,}$/i.test(line)) fail();
    const record = line.slice(1).match(/../g)!.map(byte => parseInt(byte, 16));
    const [length, hi, lo, type] = record;
    const address = hi * 256 + lo;
    const body = record.slice(4, -1);
    if (record.length !== length + 5 || (record.reduce((sum, byte) => sum + byte, 0) & 255)) fail();
    if (type === 0) {
      if (address + length > 0x10000) fail();
      for (const [offset, byte] of body.entries()) {
        const target = base + address + offset;
        if (target > 0xffff || (data.has(target) && data.get(target) !== byte)) fail();
        data.set(target, byte);
      }
    } else if (type === 1) {
      if (length !== 0 || address !== 0) fail();
      eof = true;
    } else if (type === 2 || type === 4) {
      if (length !== 2 || address !== 0) fail();
      base = (body[0] * 256 + body[1]) * (type === 2 ? 16 : 65536);
    } else if (type === 3 || type === 5) {
      if (length !== 4 || address !== 0) fail();
    } else fail();
  }
  if (!eof || !data.size) throw new Error('HEX 缺少程序数据或 EOF 结束记录，请重新导出固件。');
  const addresses = [...data.keys()];
  return { programBytes: data.size, minAddress: Math.min(...addresses), maxAddress: Math.max(...addresses) };
}

function validRtc(h: number, m: number, s: number) {
  return [h, m, s].every(Number.isInteger) && h >= 0 && h < 24 && m >= 0 && m < 60 && s >= 0 && s < 60;
}

/** UI demo only. None of the animation below models CPU or peripheral timing. */
export class MockSim implements SimController {
  private snapshot = blankSnapshot();
  private readonly board = new FakeEngine();
  private env = this.board.env;
  private targetSpeed = 1;
  private keys = new Set<KeyId>();
  private timer: ReturnType<typeof setInterval> | undefined;
  private lastTick = 0;
  private rtcSeconds = 12 * 3600;
  private eeprom = Array<number>(256).fill(255);
  private snapshots = new Set<(snapshot: Snapshot) => void>();
  private uart = new Set<(text: string) => void>();
  private loadRequest = 0;

  constructor() {
    // Keep demo defaults as direct overrides of the shared G1 physical controls.
    Object.assign(this.env, DEFAULT_ENV);
    this.board.setNe555Jumper(DEFAULT_ENV.ne555Jumper);
    this.emit();
  }

  async load(name: string, bytes: ArrayBuffer): Promise<FileInfo> {
    const request = ++this.loadRequest;
    if (!/\.hex$/i.test(name)) throw new Error('请选择 .hex 固件文件。');
    let info: FileInfo;
    try {
      const parsed = inspectHex(bytes);
      const hash = await crypto.subtle.digest('SHA-256', bytes);
      info = { name, bytes: bytes.byteLength, ...parsed,
        sha256: [...new Uint8Array(hash)].map(byte => byte.toString(16).padStart(2, '0')).join('') };
    } catch (error) {
      throw new Error(error instanceof TypeError ? 'HEX 不是有效文本，请选择 Intel HEX 文件。' : String((error as Error).message));
    }
    if (request !== this.loadRequest) throw new Error('已取消较早的文件装载，请使用最新选择。');
    this.stopTimer();
    this.keys.clear();
    this.snapshot = { ...blankSnapshot(), session: this.snapshot.session + 1, state: 'paused' };
    this.emit();
    return info;
  }

  run() {
    if (this.snapshot.state === 'empty' || this.snapshot.state === 'running') return;
    this.snapshot.state = 'running';
    this.lastTick = performance.now();
    this.timer = setInterval(() => {
      const now = performance.now();
      // Demo animation has no CPU throughput; its fastest setting is an 8x preview.
      const elapsed = (now - this.lastTick) * (this.targetSpeed || 8);
      this.lastTick = now;
      this.advance(Math.round(elapsed * 1_000_000));
    }, 1000 / 30);
    this.emit();
  }

  pause() {
    this.stopTimer();
    if (this.snapshot.state !== 'empty') this.snapshot.state = 'paused';
    this.emit();
  }

  step() {
    if (this.snapshot.state === 'empty' || this.snapshot.state === 'running') return;
    this.advance(1_000_000);
  }

  reset(kind: 'mcu' | 'power') {
    if (this.snapshot.state === 'empty') return;
    this.stopTimer();
    this.keys.clear();
    this.snapshot = { ...blankSnapshot(), session: this.snapshot.session, state: 'paused',
      diagnostic: `${DEMO} ${kind === 'mcu' ? 'MCU 已复位' : '开发板已重新上电'}；EEPROM / RTC 保留。` };
    this.emit();
  }

  setSpeed(multiple: number) {
    if (!Number.isFinite(multiple) || multiple < 0) return;
    this.targetSpeed = multiple;
    this.emit();
  }

  setIrcHz(hz: number) {
    validateIrcHz(hz);
    saveIrcHz(hz);
    this.reset('power');
    this.snapshot.ircHz = hz;
    this.emit();
  }

  setKey(id: KeyId, down: boolean) {
    // BTN grounds column 1 only: SCH_V31.pdf p. 4, J5. Other columns remain available in KBD.
    if (this.board.keyMode === 'BTN' && Number(id.slice(1)) > 7) return;
    if (down) this.keys.add(id);
    else this.keys.delete(id);
    this.emit();
  }

  setKeyMode(mode: KeyMode) {
    this.setJumper('J5', mode);
  }

  setJumper(id: JumperId, position: string) {
    this.board.setJumper(id, position);
    if (id === 'J5') this.keys.clear();
    this.emit();
  }

  setDial(id: DialId, pos01: number) {
    this.board.setDial(id, pos01);
    this.emit();
  }

  setLight(level01: number) {
    this.board.setLight(level01);
    this.emit();
  }

  setEnv(patch: Partial<Env>) {
    const limits = { temperatureC: [-55, 125], ain1V: [0, 5], ain3V: [0, 5],
      ne555Hz: [1, 100000], distanceCm: [2, 400] } as const;
    for (const key of Object.keys(patch) as (keyof Env)[]) {
      const value = patch[key];
      if (key === 'ne555Jumper') {
        if (typeof value === 'boolean') this.env.ne555Jumper = value;
      } else if (typeof value === 'number' && Number.isFinite(value)) {
        if (key === 'lightLevel') {
          // Same nominal G1 LDR curve as FakeEngine; board_hardware.md, ADC section.
          this.board.setLight(Math.min(1, Math.max(0, value)));
        } else {
          this.env[key] = Math.min(limits[key][1], Math.max(limits[key][0], value));
        }
      }
    }
    if (typeof patch.ne555Jumper === 'boolean') this.board.setNe555Jumper(patch.ne555Jumper);
    this.emit();
  }

  setRtc(h: number, m: number, s: number) {
    if (!validRtc(h, m, s)) throw new Error('RTC 时间无效，请输入 00:00:00 至 23:59:59。');
    this.rtcSeconds = h * 3600 + m * 60 + s;
  }

  uartSend(text: string) {
    if (this.snapshot.state === 'empty') return;
    for (const callback of this.uart) callback(`[演示回显] ${text.slice(0, 4096)}`);
  }

  setUartBaud(baud: number) {
    this.board.setUartBaud(baud);
    this.emit();
  }

  async exportState() {
    const seconds = Math.floor(this.rtcSeconds) % 86400;
    return JSON.stringify({ version: 1, eeprom: this.eeprom,
      rtc: { h: Math.floor(seconds / 3600), m: Math.floor(seconds / 60) % 60, s: seconds % 60 } }, null, 2);
  }

  async importState(text: string) {
    try {
      if (text.length > 65536) throw new Error();
      const value = JSON.parse(text);
      if (value?.version !== 1 || !Array.isArray(value.eeprom) || value.eeprom.length !== 256
        || !value.eeprom.every((byte: unknown) => Number.isInteger(byte) && Number(byte) >= 0 && Number(byte) <= 255)
        || !value.rtc || !validRtc(value.rtc.h, value.rtc.m, value.rtc.s)) throw new Error();
      this.eeprom = [...value.eeprom];
      this.setRtc(value.rtc.h, value.rtc.m, value.rtc.s);
      this.emit();
    } catch {
      throw new Error('状态文件无效：需要 version=1、256 字节 EEPROM 和有效 RTC。原状态已保留。');
    }
  }

  onSnapshot(callback: (snapshot: Snapshot) => void) {
    this.snapshots.add(callback);
    callback(structuredClone(this.snapshot));
    return () => { this.snapshots.delete(callback); };
  }

  onUart(callback: (text: string) => void) {
    this.uart.add(callback);
    return () => { this.uart.delete(callback); };
  }

  private stopTimer() {
    clearInterval(this.timer);
    this.timer = undefined;
  }

  private advance(ns: number) {
    this.snapshot.timeNs += ns;
    this.snapshot.pc = (this.snapshot.pc + 1) & 0xffff;
    this.rtcSeconds = (this.rtcSeconds + ns / 1e9) % 86400;
    this.emit();
  }

  private emit() {
    this.snapshot.uart = this.board.snapshot().uart;
    this.snapshot.board = JSON.parse(this.board.boardState()) as BoardState;
    const loaded = this.snapshot.state !== 'empty';
    if (loaded) {
      const temperature = Math.abs(Math.round(this.env.temperatureC * 10)).toString().padStart(4, '0');
      const counter = Math.floor(this.snapshot.timeNs / 1e8).toString().padStart(4, '0').slice(-4);
      this.snapshot.digits = [...temperature + counter].map(digit => SEGMENTS[Number(digit)]);
      this.snapshot.digits[2] = this.snapshot.digits[2]! & 0x7f;
      if (this.env.temperatureC < 0) this.snapshot.digits[0] = 0xbf;
      const held = [...this.keys].at(-1);
      if (held) this.snapshot.digits[7] = SEGMENTS[Number(held.slice(1)) % 10];
      this.snapshot.leds = Array.from({ length: 8 }, (_, i) => i === Math.floor(this.snapshot.timeNs / 2e8) % 8);
    }
    this.snapshot.speed = this.snapshot.state === 'running' ? (this.targetSpeed || 8) : 0;
    this.snapshot.relay = loaded && this.env.ain3V > 3;
    this.snapshot.motor = loaded && this.env.distanceCm < 20;
    this.snapshot.buzzer = loaded && this.keys.size > 0;
    for (const callback of this.snapshots) callback(structuredClone(this.snapshot));
  }
}
