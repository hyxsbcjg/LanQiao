export type SerialMode = 'text' | 'hex';
export type SerialEnding = 'none' | 'CR' | 'LF' | 'CRLF';
export interface SerialChunk { data: string; receivedAt: number }
export type SerialSendResult = { ok: true; data: string } | { ok: false; error: string };

export const SERIAL_RETENTION_BYTES = 32 * 1024;
const endings: Record<SerialEnding, string> = { none: '', CR: '\r', LF: '\n', CRLF: '\r\n' };

// UART strings carry one byte per code unit, not UTF-8 encoded text.
export function formatSerialBytes(data: string, mode: SerialMode): string {
  return mode === 'text' ? data : Array.from(data, byte => byte.charCodeAt(0).toString(16).padStart(2, '0').toUpperCase()).join(' ');
}

export function formatSerialTimestamp(receivedAt: number): string {
  const date = new Date(receivedAt);
  const time = [date.getHours(), date.getMinutes(), date.getSeconds()].map(value => String(value).padStart(2, '0')).join(':');
  return `${time}.${String(date.getMilliseconds()).padStart(3, '0')}`;
}

export function formatSerialReceive(chunks: readonly SerialChunk[], mode: SerialMode, timestamps: boolean): string {
  let output = '';
  for (const chunk of chunks) {
    if (!chunk.data) continue;
    if (timestamps) {
      if (output && !output.endsWith('\n')) output += '\n';
      output += `[${formatSerialTimestamp(chunk.receivedAt)}] `;
    } else if (mode === 'hex' && output) output += ' ';
    output += formatSerialBytes(chunk.data, mode);
  }
  return output;
}

export function appendSerialReceive(chunks: readonly SerialChunk[], data: string, receivedAt: number): SerialChunk[] {
  const pending = data ? [...chunks, { data, receivedAt }] : [...chunks];
  const retained: SerialChunk[] = [];
  let remaining = SERIAL_RETENTION_BYTES;
  for (let index = pending.length - 1; index >= 0 && remaining > 0; index--) {
    const chunk = pending[index];
    if (!chunk.data) continue;
    const tail = chunk.data.slice(-remaining);
    retained.push({ data: tail, receivedAt: chunk.receivedAt });
    remaining -= tail.length;
  }
  return retained.reverse();
}

export function prepareSerialSend(input: string, mode: SerialMode, ending: SerialEnding = 'LF'): SerialSendResult {
  let data = input;
  if (mode === 'hex') {
    const trimmed = input.trim();
    if (!trimmed) return { ok: false, error: '请输入要发送的 HEX 字节。' };
    const groups = trimmed.split(/\s+/);
    if (groups.some(group => !/^(?:[0-9a-f]{2})+$/i.test(group))) {
      return { ok: false, error: 'HEX 格式无效，请使用完整的两位十六进制字节。' };
    }
    data = groups.join('').replace(/[0-9a-f]{2}/gi, byte => String.fromCharCode(Number.parseInt(byte, 16)));
  } else {
    if (!data) return { ok: false, error: '请输入要发送的文本。' };
    if (/[^\x00-\xff]/.test(data)) return { ok: false, error: '文本包含非单字节字符，请改用 HEX 发送目标字节。' };
  }
  return { ok: true, data: data + endings[ending] };
}
