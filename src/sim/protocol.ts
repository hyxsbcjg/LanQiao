import type { DialId, Env, FileInfo, JumperId, KeyId, KeyMode, Snapshot } from './types';

type CommandBody =
  | { type: 'irc'; hz: number }
  | { type: 'load'; text: string; fileInfo: FileInfo }
  | { type: 'run' | 'pause' | 'step' | 'export' }
  | { type: 'reset'; kind: 'mcu' | 'power' }
  | { type: 'speed'; multiple: number }
  | { type: 'key'; key: KeyId; down: boolean }
  | { type: 'keyMode'; mode: KeyMode }
  | { type: 'jumper'; jumper: JumperId; position: string }
  | { type: 'dial'; dial: DialId; pos01: number }
  | { type: 'light'; level01: number }
  | { type: 'env'; patch: Partial<Env> }
  | { type: 'rtc'; h: number; m: number; s: number }
  | { type: 'uartBaud'; baud: number }
  | { type: 'uart' | 'import'; text: string };
export type WorkerCommand = CommandBody & { id: number; session: number };
export type WorkerEvent =
  | { type: 'ack'; id: number; session: number }
  | { type: 'error'; id: number; session: number; message: string }
  | { type: 'fileInfo'; id: number; session: number; fileInfo: FileInfo }
  | { type: 'state'; id: number; session: number; text: string }
  | { type: 'snapshot'; session: number; snapshot: Snapshot }
  | { type: 'uart'; session: number; text: string };
