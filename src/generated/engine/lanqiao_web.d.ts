/* tslint:disable */
/* eslint-disable */

export class WebSim {
    free(): void;
    [Symbol.dispose](): void;
    boardState(): string;
    debugState(): string;
    exportState(): string;
    importState(text: string): void;
    loadHex(text: string): void;
    constructor();
    peekSnapshot(): string;
    reset(mode: string): void;
    runUs(us: number): void;
    setDial(id: string, position: number): void;
    setDistanceCm(value: number): void;
    setFrequencyHz(value: number): void;
    setJumper(id: string, position: string): void;
    setKey(key: string, pressed: boolean): void;
    setKeyMode(mode: string): void;
    setLight(level: number): void;
    setNe555Jumper(installed: boolean): void;
    setRtc(hour: number, minute: number, second: number): void;
    setTemperatureC(value: number): void;
    setVoltage(channel: string, value: number): void;
    set_irc_hz(hz: number): void;
    set_uart_terminal_baud(baud: number): void;
    snapshot(): string;
    step(): void;
    takeBrightness(): string;
    uartTake(): string;
    uartWrite(text: string): void;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_websim_free: (a: number, b: number) => void;
    readonly websim_boardState: (a: number) => [number, number];
    readonly websim_debugState: (a: number) => [number, number];
    readonly websim_exportState: (a: number) => [number, number];
    readonly websim_importState: (a: number, b: number, c: number) => [number, number];
    readonly websim_loadHex: (a: number, b: number, c: number) => [number, number];
    readonly websim_new: () => number;
    readonly websim_peekSnapshot: (a: number) => [number, number];
    readonly websim_reset: (a: number, b: number, c: number) => [number, number];
    readonly websim_runUs: (a: number, b: number) => void;
    readonly websim_setDial: (a: number, b: number, c: number, d: number) => [number, number];
    readonly websim_setDistanceCm: (a: number, b: number) => [number, number];
    readonly websim_setFrequencyHz: (a: number, b: number) => [number, number];
    readonly websim_setJumper: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly websim_setKey: (a: number, b: number, c: number, d: number) => [number, number];
    readonly websim_setKeyMode: (a: number, b: number, c: number) => [number, number];
    readonly websim_setLight: (a: number, b: number) => [number, number];
    readonly websim_setNe555Jumper: (a: number, b: number) => [number, number];
    readonly websim_setRtc: (a: number, b: number, c: number, d: number) => [number, number];
    readonly websim_setTemperatureC: (a: number, b: number) => [number, number];
    readonly websim_setVoltage: (a: number, b: number, c: number, d: number) => [number, number];
    readonly websim_set_irc_hz: (a: number, b: number) => [number, number];
    readonly websim_set_uart_terminal_baud: (a: number, b: number) => [number, number];
    readonly websim_snapshot: (a: number) => [number, number];
    readonly websim_step: (a: number) => void;
    readonly websim_takeBrightness: (a: number) => [number, number];
    readonly websim_uartTake: (a: number) => [number, number, number, number];
    readonly websim_uartWrite: (a: number, b: number, c: number) => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
