import type { KeyId } from './types';

// V2 key labels differ from CT107D (SCH_V31, pp. 1, 4).
// J5 joins V2 $1N21686 to P4.4 in KBD mode; BTN grounds that column.
// CT107D rows P3.3..P3.0 cross columns P4.4, P4.2, P3.5, P3.4.
export const KEY_MAP = [
  { v2: 'S4',  engine: 'S4',  row: 'P3.3', column: 'P4.4' },
  { v2: 'S6',  engine: 'S5',  row: 'P3.2', column: 'P4.4' },
  { v2: 'S7',  engine: 'S6',  row: 'P3.1', column: 'P4.4' },
  { v2: 'S8',  engine: 'S7',  row: 'P3.0', column: 'P4.4' },
  { v2: 'S9',  engine: 'S8',  row: 'P3.3', column: 'P4.2' },
  { v2: 'S10', engine: 'S9',  row: 'P3.2', column: 'P4.2' },
  { v2: 'S11', engine: 'S10', row: 'P3.1', column: 'P4.2' },
  { v2: 'S12', engine: 'S11', row: 'P3.0', column: 'P4.2' },
  { v2: 'S13', engine: 'S12', row: 'P3.3', column: 'P3.5' },
  { v2: 'S14', engine: 'S13', row: 'P3.2', column: 'P3.5' },
  { v2: 'S15', engine: 'S14', row: 'P3.1', column: 'P3.5' },
  { v2: 'S16', engine: 'S15', row: 'P3.0', column: 'P3.5' },
  { v2: 'S17', engine: 'S16', row: 'P3.3', column: 'P3.4' },
  { v2: 'S18', engine: 'S17', row: 'P3.2', column: 'P3.4' },
  { v2: 'S19', engine: 'S18', row: 'P3.1', column: 'P3.4' },
  { v2: 'S20', engine: 'S19', row: 'P3.0', column: 'P3.4' },
] as const satisfies readonly { v2: string; engine: KeyId; row: string; column: string }[];

export type V2Key = typeof KEY_MAP[number]['v2'];
