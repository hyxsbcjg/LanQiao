import type { DebugState } from './types';

const record = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null && !Array.isArray(value);
const integer = (value: unknown, max: number): value is number => Number.isInteger(value) && Number(value) >= 0 && Number(value) <= max;
const bytes = (value: unknown, length: number): value is number[] => Array.isArray(value) && value.length === length && value.every(byte => integer(byte, 255));
const flagNames = ['cy', 'ac', 'f0', 'rs1', 'rs0', 'ov', 'f1', 'p'] as const;

export function parseDebugState(text: string): DebugState {
  const value: unknown = JSON.parse(text);
  if (!record(value) || !integer(value.pc, 65535) || !integer(value.dptr, 65535)
    || !['a', 'b', 'psw', 'sp'].every(key => integer(value[key], 255)) || !integer(value.bank, 3)
    || !record(value.flags) || !flagNames.every(name => typeof (value.flags as Record<string, unknown>)[name] === 'boolean')
    || !bytes(value.r, 8) || !bytes(value.iram, 256)
    || !Array.isArray(value.sfr) || value.sfr.length === 0 || !value.sfr.every(sfr => record(sfr)
      && integer(sfr.address, 255) && sfr.address >= 128 && typeof sfr.name === 'string' && sfr.name.length > 0 && integer(sfr.value, 255))
    || !Array.isArray(value.ports) || value.ports.length !== 5 || !value.ports.every((port, index) => record(port)
      && port.name === `P${index}` && integer(port.latch, 255) && integer(port.pins, 255))) {
    throw new Error('Invalid debug state');
  }
  const state = value as unknown as DebugState;
  if (state.bank !== ((state.psw >> 3) & 3)
    || !flagNames.every((name, index) => state.flags[name] === Boolean(state.psw & (128 >> index)))
    || !state.r.every((byte, index) => byte === state.iram[state.bank * 8 + index])) {
    throw new Error('Inconsistent debug state');
  }
  return state;
}
