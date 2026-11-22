import { MAX_HEX_BYTES, parseHex } from './hex';
import { validateDial, validateEnv, validateJumper, validateRange, validateUartBaud } from './engine';
import { DEFAULT_IRC_HZ, readIrcHz, saveIrcHz, validateIrcHz } from './irc-clock';
import type { WorkerCommand, WorkerEvent } from './protocol';
import type { DialId, Env, FileInfo, JumperId, KeyId, KeyMode, SimController, Snapshot } from './types';

export interface WorkerPort {
  postMessage(message: WorkerCommand): void;
  addEventListener(type: 'message', listener: (event: MessageEvent<WorkerEvent>) => void): void;
  removeEventListener(type: 'message', listener: (event: MessageEvent<WorkerEvent>) => void): void;
  terminate?(): void;
}

type CommandBody<C = WorkerCommand> = C extends WorkerCommand ? Omit<C, 'id' | 'session'> : never;
type Pending = { session: number } & (
  | { type: 'load'; resolve: (info: FileInfo) => void; reject: (error: Error) => void }
  | { type: 'export'; resolve: (text: string) => void; reject: (error: Error) => void }
  | { type: 'import'; resolve: () => void; reject: (error: Error) => void }
  | { type: 'command' }
);

export class WorkerSimController implements SimController {
  private worker: WorkerPort;
  private attached = false;
  private id = 0;
  private session = 0;
  private nextSession = 0;
  private loadQueue: Promise<unknown> = Promise.resolve();
  private queuedOperations = 0;
  private readonly pending = new Map<number, Pending>();
  private readonly snapshots = new Set<(snapshot: Snapshot) => void>();
  private readonly uart = new Set<(text: string) => void>();
  private latest?: Snapshot;
  private failure?: Error;
  private disposed = false;
  private nativeEvents?: Pick<Worker, 'addEventListener' | 'removeEventListener'>;
  private recovering = false;
  private generation = 0;
  private firmware?: { name: string; bytes: ArrayBuffer };
  private readonly inputs = new Map<string, CommandBody>();
  private speed = 1;
  private recoveryNotice?: string;
  private ircHz = readIrcHz();
  private readonly visibilityDocument = typeof document === 'undefined' ? undefined : document;

  constructor(
    worker?: WorkerPort,
    private readonly createWorker: () => WorkerPort = () => new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' }),
  ) {
    this.worker = worker ?? createWorker();
    this.attachWorker(this.worker);
    if (this.ircHz !== DEFAULT_IRC_HZ) this.remember('irc', { type: 'irc', hz: this.ircHz });
    this.visibilityDocument?.addEventListener('visibilitychange', this.onVisibilityChange);
  }

  private attachWorker(worker: WorkerPort): void {
    this.worker = worker;
    this.attached = true;
    worker.addEventListener('message', this.onMessage);
    // Native workers expose these properties; minimal structural test ports need not.
    if ('onerror' in worker) {
      this.nativeEvents = worker as unknown as Worker;
      this.nativeEvents.addEventListener('error', this.onWorkerError);
      this.nativeEvents.addEventListener('messageerror', this.onMessageError);
    }
  }

  private detachWorker(): void {
    if (!this.attached) return;
    this.attached = false;
    this.worker.removeEventListener('message', this.onMessage);
    this.nativeEvents?.removeEventListener('error', this.onWorkerError);
    this.nativeEvents?.removeEventListener('messageerror', this.onMessageError);
    this.nativeEvents = undefined;
    this.worker.terminate?.();
  }

  load(name: string, bytes: ArrayBuffer): Promise<FileInfo> {
    if (this.recovering) return Promise.reject(new Error('正在恢复 Worker，请稍后重新选择固件'));
    return this.loadFirmware(name, bytes);
  }

  private loadFirmware(name: string, bytes: ArrayBuffer): Promise<FileInfo> {
    const generation = this.generation;
    this.queuedOperations++;
    const task = this.loadQueue.then(async () => {
      this.assertAvailable();
      if (generation !== this.generation) throw new Error('Worker 已更换，请重新操作');
      if (bytes.byteLength > MAX_HEX_BYTES) throw new Error('HEX 文件超过 2 MiB');
      let text: string;
      try {
        text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
      } catch {
        throw new Error('HEX 文件不是有效的 UTF-8 文本');
      }
      const image = parseHex(text);
      const savedBytes = bytes.slice(0);
      const digest = await crypto.subtle.digest('SHA-256', savedBytes);
      this.assertAvailable();
      if (generation !== this.generation) throw new Error('Worker 已更换，请重新操作');
      const fileInfo: FileInfo = {
        name, bytes: bytes.byteLength, programBytes: image.programBytes,
        minAddress: image.minAddress, maxAddress: image.maxAddress,
        sha256: Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join(''),
      };
      const session = ++this.nextSession;
      const result = await new Promise<FileInfo>((resolve, reject) => {
        this.post({ type: 'load', text, fileInfo }, { type: 'load', session, resolve, reject });
      });
      this.firmware = { name, bytes: savedBytes };
      return result;
    }).finally(() => { this.queuedOperations--; });
    this.loadQueue = task.catch(() => {});
    return task;
  }

  run(): void { if (!this.visibilityDocument?.hidden) this.send({ type: 'run' }); }
  pause(): void { this.send({ type: 'pause' }); }
  step(): void { this.send({ type: 'step' }); }
  reset(kind: 'mcu' | 'power'): void { this.send({ type: 'reset', kind }); }
  setIrcHz(hz: number): void {
    validateIrcHz(hz);
    this.ircHz = hz;
    saveIrcHz(hz);
    this.remember('irc', { type: 'irc', hz });
  }
  setSpeed(multiple: number): void {
    if (Number.isFinite(multiple) && multiple >= 0) this.speed = multiple;
    this.send({ type: 'speed', multiple });
  }
  setKey(key: KeyId, down: boolean): void { this.send({ type: 'key', key, down }); }
  setKeyMode(mode: KeyMode): void {
    validateJumper('J5', mode);
    this.remember('J5', { type: 'keyMode', mode });
  }
  setJumper(jumper: JumperId, position: string): void {
    validateJumper(jumper, position);
    this.remember(jumper, { type: 'jumper', jumper, position });
  }
  setDial(dial: DialId, pos01: number): void {
    validateDial(dial, pos01);
    this.remember(dial, { type: 'dial', dial, pos01 });
  }
  setLight(level01: number): void {
    validateRange(level01, 1);
    this.remember('light', { type: 'light', level01 });
  }
  setEnv(patch: Partial<Env>): void {
    const values = { ...patch };
    validateEnv(values);
    for (const key of Object.keys(values) as (keyof Env)[]) {
      if (values[key] !== undefined) this.cacheInput(key === 'ne555Jumper' ? 'J3' : key === 'lightLevel' ? 'light' : key,
        { type: 'env', patch: { [key]: values[key] } });
    }
    this.send({ type: 'env', patch: values });
  }
  setRtc(h: number, m: number, s: number): void { this.send({ type: 'rtc', h, m, s }); }
  uartSend(text: string): void { this.send({ type: 'uart', text }); }
  setUartBaud(baud: number): void {
    validateUartBaud(baud);
    this.remember('uartBaud', { type: 'uartBaud', baud });
  }

  exportState(): Promise<string> {
    if (this.recovering) return Promise.reject(new Error('正在恢复 Worker，请稍后导出状态'));
    return this.readState();
  }

  private readState(): Promise<string> {
    return new Promise((resolve, reject) => {
      this.afterLoads(() => this.post({ type: 'export' }, { type: 'export', session: this.session, resolve, reject }));
    });
  }

  importState(text: string): Promise<void> {
    if (this.recovering) return Promise.reject(new Error('正在恢复 Worker，请稍后导入状态'));
    return new Promise((resolve, reject) => {
      this.afterLoads(() => this.post({ type: 'import', text }, { type: 'import', session: this.session, resolve, reject }));
    });
  }

  onSnapshot(cb: (snapshot: Snapshot) => void): () => void {
    if (this.disposed) return () => {};
    this.snapshots.add(cb);
    if (this.latest) cb(this.latest);
    return () => { this.snapshots.delete(cb); };
  }

  onUart(cb: (text: string) => void): () => void {
    if (this.disposed) return () => {};
    this.uart.add(cb);
    return () => { this.uart.delete(cb); };
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.detachWorker();
    this.visibilityDocument?.removeEventListener('visibilitychange', this.onVisibilityChange);
    this.rejectPending(new Error('仿真控制器已销毁'));
    this.snapshots.clear();
    this.uart.clear();
  }

  private assertAvailable(): void {
    if (this.disposed) throw new Error('仿真控制器已销毁');
    if (this.failure) throw this.failure;
  }

  private send(command: CommandBody): void {
    if (this.disposed || this.failure) return;
    if (this.recovering && !['env', 'keyMode', 'jumper', 'dial', 'light', 'speed', 'irc', 'uartBaud'].includes(command.type)) return;
    const generation = this.generation;
    this.afterLoads(() => {
      if (this.disposed || generation !== this.generation || (command.type === 'run' && this.visibilityDocument?.hidden)) return;
      this.post(command, { type: 'command', session: this.session });
    });
  }

  private cacheInput(key: string, command: CommandBody): void {
    if (this.disposed) return;
    // Reinsert each field independently: unrelated env patches must not reorder older overrides.
    this.inputs.delete(key);
    this.inputs.set(key, command);
  }

  private remember(key: string, command: CommandBody): void {
    this.cacheInput(key, command);
    this.send(command);
  }

  private afterLoads(action: () => void): void {
    if (this.queuedOperations === 0) {
      action();
      return;
    }
    // Keep commands in call order across hashing, load replies, and subsequent loads.
    this.queuedOperations++;
    const task = this.loadQueue.then(action).finally(() => { this.queuedOperations--; });
    this.loadQueue = task.catch(() => {});
  }

  private post(command: CommandBody, pending: Pending): void {
    const id = ++this.id;
    try {
      this.assertAvailable();
      this.pending.set(id, pending);
      this.worker.postMessage({ ...command, id, session: pending.session });
    } catch (cause) {
      this.pending.delete(id);
      const error = cause instanceof Error ? cause : new Error(String(cause));
      if (pending.type === 'command') this.reportError(error.message);
      else pending.reject(error);
    }
  }

  private readonly onMessage = ({ data }: MessageEvent<WorkerEvent>): void => {
    if (this.disposed || this.failure) return;
    if (data.type === 'snapshot') {
      if (data.session !== this.session || data.snapshot.session !== this.session) return;
      this.publish(data.snapshot);
      return;
    }
    if (data.type === 'uart') {
      if (data.session === this.session) for (const cb of this.uart) cb(data.text);
      return;
    }
    const pending = this.pending.get(data.id);
    if (!pending || pending.session !== data.session) return;
    if (pending.type !== 'load' && data.session !== this.session) return;
    if (data.type === 'error') {
      this.pending.delete(data.id);
      if (pending.type === 'command') this.reportError(data.message);
      else pending.reject(new Error(data.message));
    } else if (data.type === 'fileInfo' && pending.type === 'load') {
      this.pending.delete(data.id);
      this.session = data.session;
      this.recoveryNotice = undefined;
      this.latest = undefined;
      this.rejectPending(new Error('仿真会话已更换'));
      pending.resolve(data.fileInfo);
    } else if (data.type === 'state' && pending.type === 'export') {
      this.pending.delete(data.id);
      pending.resolve(data.text);
    } else if (data.type === 'ack' && (pending.type === 'import' || pending.type === 'command')) {
      this.pending.delete(data.id);
      if (pending.type === 'import') pending.resolve();
    }
  };

  private publish(snapshot: Snapshot): void {
    if (snapshot.board === undefined && this.latest?.board) snapshot = { ...snapshot, board: this.latest.board };
    if (this.recoveryNotice) snapshot = { ...snapshot, notice: this.recoveryNotice };
    this.latest = snapshot;
    for (const cb of this.snapshots) cb(snapshot);
  }

  private reportError(diagnostic: string, recovery?: Snapshot['recovery']): void {
    this.publish({
      ircHz: this.ircHz,
      session: this.session, timeNs: 0, pc: 0,
      digits: Array(8).fill(null), leds: Array(8).fill(false),
      relay: false, motor: false, buzzer: false,
      ...this.latest, state: 'error', speed: 0, diagnostic,
      ...(recovery ? { recovery } : {}),
    });
  }

  private rejectPending(error: Error): void {
    const pending = [...this.pending.values()];
    this.pending.clear();
    for (const request of pending) if (request.type !== 'command') request.reject(error);
  }

  retry(): void { this.failWorker('正在重试仿真引擎', true); }

  private failWorker(message: string, retry = false): void {
    if (this.disposed || this.recovering || (this.failure && !retry)) return;
    this.failure = new Error(message);
    this.generation++;
    this.recoveryNotice = undefined;
    this.rejectPending(this.failure);
    this.detachWorker();
    this.recovering = true;
    this.reportError(`${message}。已停止，正在重建 Worker。`, 'recovering');
    // Let rejected work settle before opening a fresh queue. Never replay run or held keys.
    void this.loadQueue.then(async () => {
      if (this.disposed) return;
      const worker = this.createWorker();
      this.session = 0;
      this.failure = undefined;
      this.latest = undefined;
      this.attachWorker(worker);
      for (const command of this.inputs.values()) this.send(command);
      this.setSpeed(this.speed);
      await this.readState(); // Wait for WASM startup, even with no firmware loaded.
      if (this.firmware) await this.loadFirmware(this.firmware.name, this.firmware.bytes);
      if (this.disposed) return;
      this.recoveryNotice = this.firmware
        ? 'Worker 已重建，已重新装载上次固件。EEPROM / RTC 未保存的状态可能丢失。'
        : '仿真引擎已重新加载，请选择 HEX 固件。';
      if (this.latest) this.publish(this.latest);
    }).catch(cause => {
      if (this.disposed) return;
      this.failure = cause instanceof Error ? cause : new Error(String(cause));
      this.rejectPending(this.failure);
      this.detachWorker();
      this.reportError(`恢复失败：${this.failure.message}。请重试。`, this.latest?.recovery === 'wasm' ? 'wasm' : 'worker');
    }).finally(() => { this.recovering = false; });
  }

  private readonly onWorkerError = (event: ErrorEvent): void => {
    event.preventDefault();
    if (this.recovering) {
      this.failure = new Error(event.message || '仿真 Worker 执行失败');
      this.rejectPending(this.failure);
      return;
    }
    this.failWorker(event.message || '仿真 Worker 执行失败');
  };

  private readonly onMessageError = (): void => {
    if (this.recovering) {
      this.failure = new Error('无法读取仿真 Worker 消息');
      this.rejectPending(this.failure);
      return;
    }
    this.failWorker('无法读取仿真 Worker 消息');
  };

  private readonly onVisibilityChange = (): void => {
    if (this.visibilityDocument?.hidden && !this.failure) this.pause();
  };
}
