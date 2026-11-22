import type { RelayEvent, Snapshot } from '../sim/types';

export type AudioSnapshot = Pick<Snapshot, 'session' | 'timeNs' | 'state' | 'speed' | 'relay' | 'buzzer'
  | 'recovery' | 'board' | 'relayEvents' | 'relayEventsDropped'>;
export type SoundKind = 'pull-in' | 'release' | 'key-press' | 'key-release' | 'rattle';
export interface AudioLogEntry { kind: SoundKind | 'buzzer'; when: number; tNs?: number }
export type AudioContextLike = Pick<BaseAudioContext,
  'currentTime' | 'sampleRate' | 'destination' | 'createBuffer' | 'createBufferSource' | 'createOscillator' | 'createGain'>
  & { resume?(): Promise<void>; close?(): Promise<void> };

declare global { interface Window { __lqAudioLog?: AudioLogEntry[] } }

export const AUDIO_LATENCY = 0.07;
export const BUZZER_GAIN = 0.025;
const TAU = 2 * Math.PI;

// Perceptual synthesis, not a calibrated recording. Gains are documented in docs/A2/report.md.
// M3 key levels and OfflineAudioContext measurements are in docs/M3/report.md.
// Bounded PCM voices also keep fastest-mode chatter independent of raw event count.
export function soundBuffer(context: AudioContextLike, kind: SoundKind, duration = 0.3, rate = 80): AudioBuffer {
  const seconds = kind === 'rattle' ? Math.max(0.015, Math.min(0.3, duration))
    : kind === 'pull-in' ? 0.055 : kind === 'release' ? 0.035 : 0.018;
  const buffer = context.createBuffer(1, Math.ceil(seconds * context.sampleRate), context.sampleRate);
  const data = buffer.getChannelData(0);
  let seed = 0x12345678;
  let previous = 0;
  for (let i = 0; i < data.length; i++) {
    const t = i / context.sampleRate;
    seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5;
    const noise = (seed >>> 0) / 0x80000000 - 1;
    const bright = (noise - previous) * 0.5;
    previous = noise;
    const attack = Math.min(1, t / 0.0005);
    let value: number;
    if (kind === 'pull-in') {
      const bounce = Math.max(0, t - 0.003);
      value = 0.17 * Math.sin(TAU * 210 * t) * Math.exp(-t / 0.009)
        + 0.16 * bright * Math.exp(-t / 0.002)
        + (t >= 0.003 ? 0.07 * bright * Math.exp(-bounce / 0.0015) : 0);
    } else if (kind === 'release') {
      value = 0.10 * Math.sin(TAU * 720 * t) * Math.exp(-t / 0.004)
        + 0.12 * bright * Math.exp(-t / 0.0015);
    } else if (kind === 'rattle') {
      // Event-rate AM becomes a capped mechanical buzz above 200 edges/s.
      const modulation = 0.25 + 0.75 * (0.5 + 0.5 * Math.cos(TAU * Math.min(200, Math.max(40, rate)) * t)) ** 4;
      value = (0.17 * bright + 0.065 * Math.sin(TAU * 230 * t)) * modulation
        * Math.min(1, (seconds - t) / 0.004);
    } else {
      value = 0.80 * bright * Math.exp(-t / 0.0018)
        + 0.35 * Math.sin(TAU * 650 * t) * Math.exp(-t / 0.003);
    }
    data[i] = value * attack;
  }
  if (kind === 'key-press' || kind === 'key-release') {
    const peak = data.reduce((peak, value) => Math.max(peak, Math.abs(value)), 0);
    const scale = (kind === 'key-press' ? 0.09 : 0.065) / peak;
    for (let i = 0; i < data.length; i++) data[i] *= scale;
  }
  return buffer;
}

export class BoardAudio {
  private readonly master: GainNode;
  private readonly buzzerGain: GainNode;
  private readonly buzzer: OscillatorNode;
  private readonly voices = new Set<AudioBufferSourceNode>();
  private readonly clicks = new Map<SoundKind, AudioBuffer>();
  private snapshot: AudioSnapshot | null = null;
  private enabled = false;
  private hidden = false;
  private audible = false;
  private buzzing = false;
  private boardPower = false;
  private drive = false;
  private anchor: { tNs: number; wall: number } | null = null;
  private ratio = 1;
  private lastEventNs: number | undefined;
  private lastWhen = 0;
  private closed = false;

  constructor(private readonly context: AudioContextLike,
    private readonly log?: (entry: AudioLogEntry) => void,
    private readonly wallTime: () => number = () => performance.now() / 1000) {
    this.master = context.createGain(); this.master.gain.value = 0;
    this.master.connect(context.destination);
    this.buzzerGain = context.createGain(); this.buzzerGain.gain.value = 0;
    this.buzzerGain.connect(this.master);
    this.buzzer = context.createOscillator();
    this.buzzer.type = 'square'; this.buzzer.frequency.value = 2000;
    this.buzzer.connect(this.buzzerGain); this.buzzer.start();
  }

  async setEnabled(enabled: boolean): Promise<void> {
    if (this.closed) return;
    // Called directly by the sound button, while browser user activation is still live.
    if (enabled) await this.context.resume?.();
    if (this.closed) return;
    this.enabled = enabled;
    this.gate();
  }

  setHidden(hidden: boolean): void { this.hidden = hidden; this.gate(); }

  private cancelVoices(): void {
    for (const source of this.voices) { source.stop(this.context.currentTime); source.disconnect(); }
    this.voices.clear(); this.lastWhen = 0;
  }

  private gate(): void {
    const audible = !this.closed && this.enabled && !this.hidden && this.snapshot?.state === 'running'
      && this.snapshot.recovery !== 'recovering';
    if (!audible) {
      this.cancelVoices();
      this.lastEventNs = undefined;
    }
    this.master.gain.value = audible ? 1 : 0;
    const buzzing = audible && !!this.snapshot?.buzzer;
    this.buzzerGain.gain.setTargetAtTime(buzzing ? BUZZER_GAIN : 0, this.context.currentTime, 0.01);
    if (buzzing && !this.buzzing) this.log?.({ kind: 'buzzer', when: this.context.currentTime, tNs: this.snapshot?.timeNs });
    this.buzzing = buzzing;
    this.audible = audible;
  }

  update(snapshot: AudioSnapshot | null, targetSpeed = 1): void {
    if (this.closed) return;
    const previous = this.snapshot;
    this.snapshot = snapshot;
    const reset = !snapshot || !previous || snapshot.session !== previous.session
      || snapshot.timeNs < previous.timeNs || snapshot.recovery !== previous.recovery;
    if (reset) {
      this.cancelVoices(); this.anchor = null; this.lastEventNs = undefined;
      this.ratio = snapshot && snapshot.speed > 0 ? snapshot.speed : 1;
    }
    this.gate();
    if (!snapshot || snapshot === previous) return;
    const now = this.context.currentTime;
    const wall = this.wallTime();
    const anchor = this.anchor;
    if (anchor && snapshot.timeNs > anchor.tNs && wall > anchor.wall) {
      this.ratio = (snapshot.timeNs - anchor.tNs) / 1e9 / (wall - anchor.wall);
    }
    if (!anchor || snapshot.timeNs > anchor.tNs || !this.audible) this.anchor = { tNs: snapshot.timeNs, wall };
    const power = snapshot.board?.jumpers.J6 === 'ON';
    const powerChanged = !reset && this.boardPower !== power;
    let events: RelayEvent[];
    // J6.1=VB/J6.2=VCC (docs/G1-board-controls.md). Removing VB releases an energized coil.
    // Jumper transitions have snapshot precision; A1's event stream timestamps relay drive only.
    if (powerChanged) {
      events = (this.drive && this.boardPower) !== (snapshot.relay && power)
        ? [{ tNs: snapshot.timeNs, on: snapshot.relay && power }] : [];
    } else if (power) {
      // Inclusive [previous timeNs, timeNs]: a paused reset can emit without advancing virtual time.
      events = snapshot.relayEvents?.filter(event => Number.isFinite(event.tNs) && event.tNs >= 0 && event.tNs <= snapshot.timeNs
        && (reset || !previous || event.tNs >= previous.timeNs))
        ?? (!reset && snapshot.relay !== this.drive ? [{ tNs: snapshot.timeNs, on: snapshot.relay }] : []);
    } else events = [];
    this.boardPower = power; this.drive = snapshot.relay;
    if (!this.audible) return;
    const dropped = power && (snapshot.relayEventsDropped ?? 0) > 0;
    if (!events.length && !dropped) return;
    const first = dropped ? anchor?.tNs ?? snapshot.timeNs : events[0]?.tNs ?? snapshot.timeNs;
    const last = events.at(-1)?.tNs ?? snapshot.timeNs;
    const burstRate = events.length > 1 ? (events.length - 1) * 1e9 * this.ratio / (last - first) : 0;
    const streamRate = this.lastEventNs !== undefined && last > this.lastEventNs
      ? events.length * 1e9 * this.ratio / (last - this.lastEventNs) : 0;
    const rate = Math.max(burstRate, streamRate);
    // Each snapshot re-anchors. Historical events can only occupy the 70 ms look-ahead, never a drifting queue.
    const map = (tNs: number) => Math.max(now, now + AUDIO_LATENCY + (tNs - snapshot.timeNs) / 1e9 / this.ratio);
    // Several past edges would otherwise become simultaneous, potentially clipping clicks.
    const clampedBurst = events.length > 1 && map(events[1].tNs) === now;
    if (dropped || targetSpeed === 0 || rate > 40 || clampedBurst) {
      const when = Math.max(map(first), this.lastWhen);
      const duration = Math.max(0.015, map(last) - when + Math.min(0.025, 1 / Math.max(40, rate)));
      this.schedule('rattle', when, first, duration, dropped || targetSpeed === 0 ? Math.max(80, rate) : rate);
    } else {
      for (const event of events) this.schedule(event.on ? 'pull-in' : 'release', map(event.tNs), event.tNs);
    }
    this.lastEventNs = last;
  }

  private schedule(kind: SoundKind, when: number, tNs?: number, duration?: number, rate?: number): void {
    const relay = kind !== 'key-press' && kind !== 'key-release';
    when = Math.max(this.context.currentTime, when, relay ? this.lastWhen : 0);
    if (relay) this.lastWhen = when;
    const source = this.context.createBufferSource();
    let buffer = this.clicks.get(kind);
    if (kind === 'rattle') buffer = soundBuffer(this.context, kind, duration, rate);
    else if (!buffer) { buffer = soundBuffer(this.context, kind); this.clicks.set(kind, buffer); }
    source.buffer = buffer;
    source.connect(this.master);
    this.voices.add(source);
    source.onended = () => { this.voices.delete(source); source.disconnect(); };
    source.start(when);
    this.log?.({ kind, when, ...(tNs === undefined ? {} : { tNs }) });
  }

  key(down: boolean): void {
    if (!this.audible) return;
    // Local keys bypass the relay look-ahead and never wait for a worker snapshot.
    this.schedule(down ? 'key-press' : 'key-release', this.context.currentTime);
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true; this.gate();
    this.buzzer.stop(); this.buzzer.disconnect(); this.buzzerGain.disconnect(); this.master.disconnect();
    await this.context.close?.();
  }
}
