import type { Snapshot } from '../sim/types';

const digitCodes = [0xc0, 0xf9, 0xa4, 0xb0, 0x99, 0x92, 0x82, 0xf8, 0x80, 0x90];
export function displayText(digits: Snapshot['digits']) {
  return digits.map((raw, index) => {
    if (raw === null || raw === 0xff) return `–${index === 3 ? ' ' : ''}`;
    const digit = digitCodes.indexOf(raw | 0x80);
    return `${digit >= 0 ? digit : (raw | 0x80) === 0xbf ? '-' : '?'}${!(raw & 0x80) ? '.' : ''}${index === 3 ? ' ' : ''}`;
  }).join('');
}
