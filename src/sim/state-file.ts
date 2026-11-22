// Field order follows engine/crates/core/src/persistent_state.rs.
const format = 'stcjudge_persistent_v1';
const fields = {
  ds18b20: { rom: 16, eeprom_th: 2, eeprom_tl: 2, eeprom_config: 2 },
  ds1302: { write_protect: 1, trickle_charge: 2, hour_mode_12: 1, hour: 2,
    minute: 2, second: 2, day_of_week: 2, date: 2, month: 2, year: 2, halted: 1, sub_ns: 16 },
};

function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('状态文件字段必须是对象。');
  return value as Record<string, unknown>;
}

function hex(value: unknown, length: number): string {
  if (typeof value !== 'string' || !new RegExp(`^[0-9a-fA-F]{${length}}$`).test(value)
    || (length === 1 && value !== '0' && value !== '1')) throw new Error('状态文件包含缺失或无效的十六进制字段。');
  return value.toUpperCase();
}

function address(offset: number): string { return `0x${offset.toString(16).padStart(2, '0').toUpperCase()}`; }

function rows(text: string): Record<string, string> {
  return Object.fromEntries(Array.from({ length: Math.ceil(text.length / 32) }, (_, row) =>
    [address(row * 16), text.slice(row * 32, row * 32 + 32).match(/../g)!.join(' ')]));
}

function readRows(value: unknown, bytes: number): string {
  const data = object(value);
  if (Object.keys(data).length !== Math.ceil(bytes / 16)) throw new Error('状态文件的存储器行数不正确。');
  return Array.from({ length: Math.ceil(bytes / 16) }, (_, row) => {
    const text = data[address(row * 16)];
    if (typeof text !== 'string') throw new Error('状态文件缺少存储器地址行。');
    const count = Math.min(16, bytes - row * 16);
    if (!new RegExp(`^[0-9a-fA-F]{2}( [0-9a-fA-F]{2}){${count - 1}}$`).test(text)) {
      throw new Error('存储器每字节需为两位十六进制，以空格分隔。');
    }
    return text.replaceAll(' ', '').toUpperCase();
  }).join('');
}

export function exportStateFile(text: string): string {
  if (!text.startsWith(`${format}|`)) return JSON.stringify(JSON.parse(text), null, 2) + '\n';
  const sections = Object.fromEntries(text.split('|').slice(1).map(section => section.split('=')));
  const data: Record<string, unknown> = { format, encoding: 'hex' };
  for (const [chip, schema] of Object.entries(fields)) {
    const values = sections[chip].split(',');
    data[chip] = Object.fromEntries(Object.keys(schema).map((name, i) => [name, values[i]]));
  }
  data.ds1302_ram = rows(sections.ds1302_ram);
  data.at24c02 = rows(sections.at24c02);
  const result = JSON.stringify(data, null, 2) + '\n';
  importStateFile(result);
  return result;
}

export function importStateFile(text: string): string {
  text = text.trim();
  if (text.startsWith(`${format}|`)) return text;
  const data = object(JSON.parse(text));
  // Demo controllers retain their existing JSON format and validate it themselves.
  if (!('format' in data)) return text;
  if (data.format !== format || data.encoding !== 'hex') throw new Error('不支持的状态文件版本或编码。');
  const sections = [format];
  for (const [chip, schema] of Object.entries(fields)) {
    const values = object(data[chip]);
    sections.push(`${chip}=${Object.entries(schema).map(([name, length]) => hex(values[name], length)).join(',')}`);
  }
  sections.push(`ds1302_ram=${readRows(data.ds1302_ram, 31)}`, `at24c02=${readRows(data.at24c02, 256)}`);
  return sections.join('|');
}
