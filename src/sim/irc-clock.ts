export const DEFAULT_IRC_HZ = 12_000_000;
export const IRC_PRESETS_MHZ = [11.0592, 12, 22.1184, 24, 27, 30, 33.1776, 35] as const;
const storageKey = 'lanqiao-irc-hz';

export function ircDownloadNotice(hz: number, loaded: boolean): string {
  return `IRC 已设为 ${(hz / 1e6).toFixed(3)} MHz，${loaded ? '程序已重新上电，点击继续运行' : '下次装载时生效'}。`;
}

export function validateIrcHz(hz: number): void {
  if (!Number.isInteger(hz) || hz < 5_000_000 || hz > 35_000_000) {
    throw new Error('IRC 频率须为 5.000–35.000 MHz（整数 Hz）。');
  }
}

export function readIrcHz(): number {
  try {
    const hz = Number(localStorage.getItem(storageKey));
    validateIrcHz(hz);
    return hz;
  } catch { return DEFAULT_IRC_HZ; }
}

export function saveIrcHz(hz: number): void {
  try { localStorage.setItem(storageKey, String(hz)); } catch { /* Storage may be blocked. */ }
}
