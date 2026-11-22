export const FLASH_SIZE = 61 * 1024;
export const MAX_HEX_BYTES = 2 * 1024 * 1024;

export interface HexImage {
  image: Uint8Array;
  programBytes: number;
  minAddress: number;
  maxAddress: number;
  startMetadata: { type: number; bytes: number[] }[];
}

export function parseHex(text: string): HexImage {
  if (text.length > MAX_HEX_BYTES) throw new Error('HEX 文本超过 2 MiB');
  const image = new Uint8Array(FLASH_SIZE).fill(0xff);
  const seen = new Uint8Array(FLASH_SIZE);
  const startMetadata: HexImage['startMetadata'] = [];
  let base = 0, segmented = false, ended = false, programBytes = 0;
  let minAddress = FLASH_SIZE, maxAddress = -1;
  const lines = text.split(/\r?\n/);
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index].trim();
    if (!line) continue;
    const fail = (message: string): never => { throw new Error(`第 ${index + 1} 行：${message}`); };
    if (ended) fail('EOF 后存在记录');
    if (!/^:[\da-fA-F]+$/.test(line) || line.length % 2 !== 1 || line.length < 11) fail('不是有效的 HEX 记录');
    const bytes = Uint8Array.from(line.slice(1).match(/../g)!, (pair) => Number.parseInt(pair, 16));
    const count = bytes[0], offset = bytes[1] * 256 + bytes[2], type = bytes[3];
    if (bytes.length !== count + 5) fail('字节数与记录长度不一致');
    if (bytes.reduce((sum, byte) => sum + byte, 0) % 256 !== 0) fail('校验和错误');
    if (type === 0) {
      for (let i = 0; i < count; i++) {
        const address = base + (segmented ? (offset + i) % 65536 : offset + i);
        if (address >= FLASH_SIZE) fail(`地址 0x${address.toString(16).toUpperCase()} 超出用户 Flash`);
        const value = bytes[4 + i];
        if (seen[address] && image[address] !== value) fail('同一地址出现冲突数据');
        if (!seen[address]) programBytes++;
        seen[address] = 1; image[address] = value;
        minAddress = Math.min(minAddress, address); maxAddress = Math.max(maxAddress, address);
      }
    } else {
      if (offset !== 0) fail('非数据记录的地址字段必须为 0000');
      if (type === 1) {
        if (count !== 0) fail('EOF 长度必须为零');
        ended = true;
      } else if (type === 2 || type === 4) {
        if (count !== 2) fail('扩展地址记录必须包含两个字节');
        segmented = type === 2;
        base = (bytes[4] * 256 + bytes[5]) * (segmented ? 16 : 65536);
      } else if (type === 3 || type === 5) {
        if (count !== 4) fail('入口元数据必须包含四个字节');
        startMetadata.push({ type, bytes: Array.from(bytes.slice(4, 8)) });
      } else fail(`不支持记录类型 ${type}`);
    }
  }
  if (!ended) throw new Error('缺少 EOF 记录');
  if (!programBytes) throw new Error('HEX 不包含程序数据');
  return { image, programBytes, minAddress, maxAddress, startMetadata };
}
