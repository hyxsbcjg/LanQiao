import { useEffect, useId, useMemo, useRef, useState } from 'react';
import type { SimController } from '../sim/types';
import {
  appendSerialReceive, formatSerialReceive, prepareSerialSend,
  type SerialChunk, type SerialEnding, type SerialMode,
} from './serial';

export interface UartPanelProps { controller: SimController; loaded: boolean }

const baudRates = [1200, 2400, 4800, 9600, 14400, 19200, 38400, 57600, 115200];
const baudStorageKey = 'lanqiao.uartBaud';
let sessionBaud: number | undefined;
function storedBaud(): number {
  if (sessionBaud !== undefined) return sessionBaud;
  sessionBaud = 9600;
  try {
    const baud = Number(localStorage.getItem(baudStorageKey));
    if (baudRates.includes(baud)) return sessionBaud = baud;
  } catch { /* Storage can be unavailable in private or restricted browsers. */ }
  return sessionBaud;
}

function SerialModeToggle({ label, value, onChange, disabled = false }: {
  label: string; value: SerialMode; onChange: (mode: SerialMode) => void; disabled?: boolean;
}) {
  return <div className="serial-format" role="group" aria-label={`${label}格式`}>
    <span className="serial-label" aria-hidden="true">{label}</span>
    {(['text', 'hex'] as const).map(mode => <button key={mode} type="button" className="serial-format-button"
      aria-pressed={value === mode} disabled={disabled} onClick={() => onChange(mode)}>
      {mode === 'text' ? '文本' : 'HEX'}
    </button>)}
  </div>;
}

export function UartPanel({ controller, loaded }: UartPanelProps) {
  const id = useId();
  const [received, setReceived] = useState<SerialChunk[]>([]);
  const [receiveMode, setReceiveMode] = useState<SerialMode>('text');
  const [sendMode, setSendMode] = useState<SerialMode>('text');
  const [timestamps, setTimestamps] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [ending, setEnding] = useState<SerialEnding>('LF');
  const [text, setText] = useState('');
  const [sendError, setSendError] = useState('');
  const [baud, setBaud] = useState(storedBaud);
  const log = useRef<HTMLPreElement>(null);
  const display = useMemo(() => formatSerialReceive(received, receiveMode, timestamps), [received, receiveMode, timestamps]);
  const prepared = prepareSerialSend(text, sendMode, ending);
  const error = sendError || (text && !prepared.ok ? prepared.error : '');

  useEffect(() => controller.onUart(data => {
    if (!data) return;
    const receivedAt = Date.now();
    setReceived(previous => appendSerialReceive(previous, data, receivedAt));
  }), [controller]);
  useEffect(() => { controller.setUartBaud(baud); }, [controller, baud]);
  useEffect(() => {
    if (autoScroll && log.current) log.current.scrollTop = log.current.scrollHeight;
  }, [display, autoScroll]);

  return <section className="uart-panel serial-panel" aria-labelledby={`${id}-heading`}>
    <div className="section-heading serial-heading"><h2 id={`${id}-heading`}>串口助手</h2>
      <button type="button" className="text-button serial-clear" onClick={() => setReceived([])}
        disabled={!received.length}>清空接收</button></div>
    <label className="serial-ending">波特率<select value={baud} onChange={event => {
      const value = Number(event.target.value);
      sessionBaud = value;
      setBaud(value);
      try { localStorage.setItem(baudStorageKey, String(value)); } catch { /* Keep session selection. */ }
    }}>{baudRates.map(value => <option key={value} value={value}>{value}</option>)}</select></label>
    <div className="serial-toolbar">
      <SerialModeToggle label="接收" value={receiveMode} onChange={setReceiveMode} />
      <div className="serial-options">
        <label className="serial-option"><input type="checkbox" checked={timestamps}
          onChange={event => setTimestamps(event.target.checked)} />时间戳</label>
        <label className="serial-option"><input type="checkbox" checked={autoScroll}
          onChange={event => setAutoScroll(event.target.checked)} />自动滚动</label>
      </div>
    </div>
    <pre className="serial-log" role="log" aria-label="串口接收" aria-live="off" ref={log} tabIndex={0}>
      {display || <span className="serial-empty">{loaded ? '暂无接收数据' : '未装载固件'}</span>}</pre>
    <form className="serial-send" onSubmit={event => {
      event.preventDefault();
      if (!loaded || !prepared.ok) return;
      try {
        controller.uartSend(prepared.data);
        setText(''); setSendError('');
      } catch (cause) {
        setSendError(cause instanceof Error ? cause.message : '发送失败，请重试。');
      }
    }}>
      <div className="serial-toolbar serial-send-toolbar">
        <SerialModeToggle label="发送" value={sendMode} disabled={!loaded} onChange={mode => {
          setSendMode(mode); setSendError('');
        }} />
        <label className="serial-ending">行尾<select value={ending} disabled={!loaded}
          onChange={event => { setEnding(event.target.value as SerialEnding); setSendError(''); }}>
          <option value="none">无</option><option value="CR">CR</option>
          <option value="LF">LF</option><option value="CRLF">CRLF</option>
        </select></label>
      </div>
      <div className="inline-form serial-send-row">
        <input className="serial-input" aria-label={sendMode === 'text' ? '串口发送文本' : '串口发送 HEX'}
          aria-invalid={!!error} aria-describedby={error ? `${id}-error` : undefined}
          value={text} maxLength={4096} disabled={!loaded} autoComplete="off" spellCheck={false}
          onChange={event => { setText(event.target.value); setSendError(''); }} />
        <button type="submit" className="serial-submit" disabled={!loaded || !prepared.ok}>发送</button>
      </div>
      {error && <p className="serial-error" id={`${id}-error`} role="alert">{error}</p>}
    </form>
  </section>;
}
