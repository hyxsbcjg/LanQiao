import { validateDial, validateEnv, validateJumper, validateRange, validateUartBaud, type EngineApi } from './engine';
import type { BoardState, DebugState, Env, RunState, Snapshot } from './types';
import { ircDownloadNotice, validateIrcHz } from './irc-clock';

export type SchedulerEvent =
  | { type: 'snapshot'; snapshot: Snapshot }
  | { type: 'uart'; text: string };

interface Clock {
  now(): number;
  setTimeout(callback: () => void, delay: number): unknown;
  clearTimeout(handle: unknown): void;
}

const defaultClock: Clock = {
  now: () => performance.now(),
  setTimeout: (callback, delay) => globalThis.setTimeout(callback, delay),
  clearTimeout: (handle) => globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>),
};

export class Scheduler {
  public session = 0;
  public state: RunState = 'empty';
  private speed = 1;
  private timer: unknown;
  private scheduled = false;
  private disposed = false;
  private loaded = false;
  private inputs: { action: () => void; retainOnLoad: boolean }[] = [];
  private wall = 0;
  private targetNs = 0;
  private startedAt = 0;
  private startedNs = 0;
  private lastSnapshot = -Infinity;
  private lastBoard: string | undefined;
  private lastDebug = -Infinity;
  private debug?: DebugState;
  private ircNotice?: string;

  constructor(
    private readonly engine: EngineApi,
    private readonly emit: (event: SchedulerEvent) => void,
    private readonly clock: Clock = defaultClock,
  ) {}

  load(text: string, session: number): void {
    this.engine.loadHex(text);
    this.cancel();
    // Applied controls persist in the engine; pending controls belong to the replacement board too.
    const pending = this.inputs;
    this.inputs = [];
    for (const input of pending) if (input.retainOnLoad) input.action();
    this.loaded = true;
    this.session = session;
    this.state = 'paused';
    this.lastBoard = undefined;
    this.rebase();
    this.publish();
  }

  run(): void {
    if (this.disposed || this.state !== 'paused') return;
    if (this.engine.snapshot(false).diagnostic) {
      this.publish();
      return;
    }
    this.state = 'running';
    this.rebase();
    this.publish();
    this.schedule();
  }

  pause(): void {
    if (this.state !== 'running') return;
    this.cancel();
    this.state = 'paused';
    while (this.inputs.length) this.inputs.shift()!.action();
    this.publish();
  }

  step(): void {
    if (this.disposed || this.state !== 'paused') return;
    this.engine.step();
    this.rebase();
    this.publish();
  }

  reset(...args: Parameters<EngineApi['reset']>): void {
    while (this.inputs.length) this.inputs.shift()!.action();
    this.engine.reset(...args);
    this.cancel();
    this.state = this.loaded ? 'paused' : 'empty';
    this.rebase();
    this.publish();
  }

  setSpeed(multiple: number): void {
    if (!Number.isFinite(multiple) || multiple < 0) throw new Error('Speed must be nonnegative and finite');
    if (this.state === 'running') this.accrue();
    if (this.speed === 0 || multiple === 0) this.rebase();
    this.speed = multiple;
    this.publish();
  }

  setIrcHz(hz: number): void {
    validateIrcHz(hz);
    while (this.inputs.length) this.inputs.shift()!.action();
    this.engine.setIrcHz(hz);
    this.cancel();
    this.state = this.loaded ? 'paused' : 'empty';
    this.rebase();
    this.ircNotice = ircDownloadNotice(hz, this.loaded);
    this.publish();
    this.ircNotice = undefined;
  }

  setKey(...args: Parameters<EngineApi['setKey']>): void {
    this.input(() => this.engine.setKey(...args));
  }

  setKeyMode(...args: Parameters<EngineApi['setKeyMode']>): void {
    validateJumper('J5', args[0]);
    this.input(() => this.engine.setKeyMode(...args), true);
  }

  setJumper(...args: Parameters<EngineApi['setJumper']>): void {
    validateJumper(...args);
    this.input(() => this.engine.setJumper(...args), true);
  }

  setDial(...args: Parameters<EngineApi['setDial']>): void {
    validateDial(...args);
    this.input(() => this.engine.setDial(...args), true);
  }

  setLight(...args: Parameters<EngineApi['setLight']>): void {
    validateRange(args[0], 1);
    this.input(() => this.engine.setLight(...args), true);
  }

  setEnv(patch: Partial<Env>): void {
    const values = { ...patch };
    validateEnv(values);
    this.input(() => {
      // Object order also preserves overlapping light/AIN1 precedence during recovery replay.
      for (const key of Object.keys(values) as (keyof Env)[]) {
        if (values[key] === undefined) continue;
        switch (key) {
          case 'temperatureC': this.engine.setTemperatureC(values.temperatureC!); break;
          case 'ain1V': this.engine.setVoltage('AIN1', values.ain1V!); break;
          case 'ain3V': this.engine.setVoltage('AIN3', values.ain3V!); break;
          case 'ne555Hz': this.engine.setFrequencyHz(values.ne555Hz!); break;
          case 'distanceCm': this.engine.setDistanceCm(values.distanceCm!); break;
          case 'ne555Jumper': this.engine.setNe555Jumper(values.ne555Jumper!); break;
          case 'lightLevel': this.engine.setLight(values.lightLevel!); break;
        }
      }
    }, true);
  }

  setRtc(...args: Parameters<EngineApi['setRtc']>): void {
    this.input(() => this.engine.setRtc(...args));
  }

  uartSend(text: string): void {
    this.input(() => this.engine.uartWrite(text));
  }

  setUartBaud(baud: number): void {
    validateUartBaud(baud);
    this.input(() => this.engine.setUartBaud(baud), true);
  }

  exportState(): string {
    const text = this.engine.exportState();
    this.drain();
    return text;
  }

  importState(text: string): void {
    this.engine.importState(text);
    this.publish();
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.cancel();
    this.inputs.length = 0;
    this.engine.free?.();
  }

  private input(action: () => void, retainOnLoad = false): void {
    if (this.disposed) return;
    if (this.state === 'running') {
      this.inputs.push({ action, retainOnLoad });
    } else {
      action();
      this.publish();
    }
  }

  private rebase(): void {
    this.wall = this.startedAt = this.clock.now();
    this.targetNs = this.startedNs = this.engine.snapshot(false).timeNs;
  }

  private accrue(): void {
    const now = this.clock.now();
    this.targetNs += Math.max(0, now - this.wall) * this.speed * 1_000_000;
    // Rebase the wall anchor with at most 200 ms of virtual debt; never advance the engine here.
    this.targetNs = Math.min(this.targetNs, this.engine.snapshot(false).timeNs + 200_000_000);
    this.wall = now;
  }

  private schedule(): void {
    if (this.disposed || this.state !== 'running' || this.scheduled) return;
    this.scheduled = true;
    this.timer = this.clock.setTimeout(() => {
      this.scheduled = false;
      this.tick();
    }, 1);
  }

  private cancel(): void {
    if (this.scheduled) this.clock.clearTimeout(this.timer);
    this.scheduled = false;
  }

  private tick(): void {
    if (this.disposed || this.state !== 'running') return;
    const start = this.clock.now();
    this.accrue();
    let snapshot = this.engine.snapshot(false);
    let maxCostPerUs = 0;
    // The call cap also bounds work when a coarse/injected clock does not advance.
    for (let calls = 0; calls < 100 && this.clock.now() - start < 5; calls++) {
      const remaining = 5 - (this.clock.now() - start);
      const budgetUs = maxCostPerUs > 0 ? Math.floor(remaining / maxCostPerUs) : 1000;
      const us = Math.min(1000, budgetUs, this.speed === 0 ? 1000 : Math.floor((this.targetNs - snapshot.timeNs) / 1000));
      if (us <= 0 || snapshot.diagnostic) break;
      this.inputs.shift()?.action();
      const before = snapshot.timeNs;
      const runStart = this.clock.now();
      this.engine.runUs(us);
      // Calls cannot be preempted; the first call can exceed the wall budget.
      maxCostPerUs = Math.max(maxCostPerUs, (this.clock.now() - runStart) / us);
      snapshot = this.engine.snapshot(false);
      if (snapshot.timeNs <= before) break;
    }
    if (snapshot.diagnostic) this.state = 'error';
    if (this.state !== 'running' || this.clock.now() - this.lastSnapshot >= 1000 / 30) {
      this.publish();
    } else {
      this.drain();
    }
    this.schedule();
  }

  private drain(): void {
    const text = this.engine.uartTake();
    if (text) this.emit({ type: 'uart', text });
  }

  private publish(): void {
    const now = this.clock.now();
    const snapshot = this.engine.snapshot();
    if (snapshot.diagnostic && this.loaded) {
      this.state = 'error';
      this.cancel();
    }
    const elapsed = now - this.startedAt;
    this.lastSnapshot = now;
    const board = this.engine.boardState();
    const boardDelta = board === this.lastBoard ? {} : { board: JSON.parse(board) as BoardState };
    this.lastBoard = board;
    if (this.state !== 'running' || now - this.lastDebug >= 200) {
      this.debug = this.engine.debugState();
      this.lastDebug = now;
    }
    this.emit({
      type: 'snapshot',
      snapshot: {
        ...(this.ircNotice ? { notice: this.ircNotice } : {}),
        ...snapshot,
        ...boardDelta,
        brightness: this.engine.takeBrightness(),
        debug: this.debug,
        session: this.session,
        state: this.state,
        speed: this.state === 'running' && elapsed > 0
          ? (snapshot.timeNs - this.startedNs) / (elapsed * 1_000_000) : 0,
      },
    });
    this.drain();
  }
}
