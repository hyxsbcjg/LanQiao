import { Fragment, useId } from 'react';
import type { Snapshot } from '../sim/types';

const hex = (value: number, width = 2) => value.toString(16).toUpperCase().padStart(width, '0');

export function DebugPanel({ snapshot }: { snapshot: Snapshot }) {
  const id = useId();
  const debug = snapshot.debug;
  const registers = debug ? [
    ['PC', hex(snapshot.pc, 4)], ['A', hex(debug.a)], ['B', hex(debug.b)], ['PSW', hex(debug.psw)],
    ['SP', hex(debug.sp)], ['DPTR', hex(debug.dptr, 4)], ['Bank', String(debug.bank)],
    ...debug.r.map((value, index) => [`R${index}`, hex(value)]),
  ] : [];
  return <section className="debug-panel"><details>
    <summary>调试 · 寄存器与内存</summary>
    {snapshot.uart && <p>UART1 固件波特率 <code>{snapshot.uart.fwBaud === null ? '—' : Math.round(snapshot.uart.fwBaud)}</code></p>}
    {!debug ? <p className="unavailable">暂无调试数据</p> : <>
      <div className="debug-registers"><dl>
        {registers.map(([name, value]) => <Fragment key={name}><dt>{name}</dt><dd>
          <code data-testid={name === 'PC' ? 'debug-pc' : undefined}>{value}</code>
        </dd></Fragment>)}
      </dl></div>
      <div className="debug-flags" aria-label="PSW 标志位">
        {Object.entries(debug.flags).map(([name, value]) => <span key={name} className={value ? 'active' : undefined}>
          {name.toUpperCase()} <code>{Number(value)}</code>
        </span>)}
      </div>
      <div className="debug-ports"><table aria-labelledby={`${id}-ports`}>
        <caption id={`${id}-ports`}>端口 · 锁存值与引脚</caption>
        <thead><tr>{['端口', 'Latch', 'Pins'].map(label => <th key={label} scope="col">{label}</th>)}</tr></thead>
        <tbody>{debug.ports.map(port => <tr key={port.name}><th scope="row">{port.name}</th>
          <td>{hex(port.latch)}</td><td>{hex(port.pins)}</td></tr>)}</tbody>
      </table></div>
      <div className="debug-memory" role="region" aria-labelledby={`${id}-iram`} tabIndex={0}>
        <table aria-labelledby={`${id}-iram`}>
          <caption id={`${id}-iram`}>IRAM · 256 字节</caption>
          <thead><tr><th scope="col">地址</th>{Array.from({ length: 16 }, (_, index) =>
            <th scope="col" key={index}>+{hex(index, 1)}</th>)}</tr></thead>
          <tbody>{Array.from({ length: 16 }, (_, row) => <tr key={row}><th scope="row">{hex(row * 16)}</th>
            {debug.iram.slice(row * 16, row * 16 + 16).map((value, column) => <td key={column}
              title={`IRAM ${hex(row * 16 + column)}${row * 16 + column >= debug.bank * 8 && row * 16 + column < debug.bank * 8 + 8 ? ` · Bank ${debug.bank} R${row * 16 + column - debug.bank * 8}` : ''}`}>{hex(value)}</td>)}
          </tr>)}</tbody>
        </table>
      </div>
      <details>
        <summary id={`${id}-sfr`}>SFR · 命名寄存器（{debug.sfr.length}）</summary>
        <div className="debug-sfr"><table aria-labelledby={`${id}-sfr`}>
          <thead><tr>{['名称', '地址', '值'].map(label => <th key={label} scope="col">{label}</th>)}</tr></thead>
          <tbody>{debug.sfr.map(sfr => <tr key={sfr.address}><th scope="row">{sfr.name}</th>
            <td>{hex(sfr.address)}</td><td>{hex(sfr.value)}</td></tr>)}</tbody>
        </table></div>
      </details>
    </>}
  </details></section>;
}
