import { loadWasmEngine, type EngineApi } from './engine';
import type { WorkerCommand, WorkerEvent } from './protocol';
import { Scheduler } from './scheduler';
import type { Snapshot } from './types';

// Keep the message boundary injectable; production and fake-engine tests use the same path.
export function createWorkerRuntime(
  post: (event: WorkerEvent) => void,
  loadEngine: () => Promise<EngineApi> = loadWasmEngine,
  clock?: ConstructorParameters<typeof Scheduler>[2],
) {
  let scheduler: Scheduler | undefined;
  let failure: string | undefined;
  let disposed = false;
  let loadingEvents: WorkerEvent[] | undefined;
  const emit = (event: WorkerEvent) => {
    if (loadingEvents) loadingEvents.push(event);
    else post(event);
  };
  const ready = loadEngine().then(engine => {
    if (disposed) { engine.free?.(); return; }
    scheduler = new Scheduler(engine, event => {
      if (event.type === 'snapshot') emit({ ...event, session: event.snapshot.session });
      else emit({ ...event, session: scheduler!.session });
    }, clock);
  }).catch(cause => {
    failure = cause instanceof Error ? cause.message : String(cause);
    if (!disposed) {
      const snapshot: Snapshot = {
        ircHz: 12_000_000,
        session: 0, state: 'error', timeNs: 0, speed: 0, pc: 0,
        digits: Array(8).fill(null), leds: Array(8).fill(false),
        relay: false, motor: false, buzzer: false, diagnostic: failure, recovery: 'wasm',
      };
      post({ type: 'snapshot', session: 0, snapshot });
    }
  });
  let queue = ready;

  const execute = (command: WorkerCommand): void => {
    if (disposed) return;
    const { id, session } = command;
    try {
      if (!scheduler) throw new Error(failure ?? '仿真引擎不可用');
      if (command.type === 'load') {
        if (session <= scheduler.session) return;
        loadingEvents = [];
        try {
          scheduler.load(command.text, session);
          // The controller must commit the session before receiving its first snapshot.
          post({ type: 'fileInfo', id, session, fileInfo: command.fileInfo });
          for (const event of loadingEvents) post(event);
        } finally { loadingEvents = undefined; }
        return;
      }
      if (session !== scheduler.session) return;
      switch (command.type) {
        case 'irc': scheduler.setIrcHz(command.hz); break;
        case 'run': scheduler.run(); break;
        case 'pause': scheduler.pause(); break;
        case 'step': scheduler.step(); break;
        case 'reset': scheduler.reset(command.kind); break;
        case 'speed': scheduler.setSpeed(command.multiple); break;
        case 'key': scheduler.setKey(command.key, command.down); break;
        case 'keyMode': scheduler.setKeyMode(command.mode); break;
        case 'jumper': scheduler.setJumper(command.jumper, command.position); break;
        case 'dial': scheduler.setDial(command.dial, command.pos01); break;
        case 'light': scheduler.setLight(command.level01); break;
        case 'env': scheduler.setEnv(command.patch); break;
        case 'rtc': scheduler.setRtc(command.h, command.m, command.s); break;
        case 'uart': scheduler.uartSend(command.text); break;
        case 'uartBaud': scheduler.setUartBaud(command.baud); break;
        case 'import': scheduler.importState(command.text); break;
        case 'export':
          post({ type: 'state', id, session, text: scheduler.exportState() });
          return;
      }
      post({ type: 'ack', id, session });
    } catch (cause) {
      post({ type: 'error', id, session, message: cause instanceof Error ? cause.message : String(cause) });
    }
  };
  return {
    handle(command: WorkerCommand): Promise<void> {
      queue = queue.then(() => execute(command));
      return queue;
    },
    dispose(): void { disposed = true; scheduler?.dispose(); },
  };
}

// This file can also be imported in Node tests without registering a worker listener.
if (typeof globalThis.postMessage === 'function' && typeof document === 'undefined') {
  const runtime = createWorkerRuntime(event => globalThis.postMessage(event));
  globalThis.addEventListener('message', (event: MessageEvent<WorkerCommand>) => {
    void runtime.handle(event.data);
  });
}
