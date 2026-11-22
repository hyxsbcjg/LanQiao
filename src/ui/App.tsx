import { useEffect, useRef, useState, type CSSProperties } from 'react';
import type { Env, FileInfo, SimController, Snapshot } from '../sim/types';
import { BoardView } from './BoardView';
import { DebugPanel } from './DebugPanel';
import { UartPanel } from './UartPanel';
import { SupportPanel } from './SupportPanel';
import { DownloadSettings } from './DownloadSettings';
import { ircDownloadNotice } from '../sim/irc-clock';
import { exportStateFile, importStateFile } from '../sim/state-file';
import { handleShortcut } from './shortcuts';
import './styles.css';

const stateLabels = { empty: '等待固件', paused: '已暂停', running: '运行中', error: '已停止' };
const address = (value: number) => `0x${value.toString(16).toUpperCase().padStart(4, '0')}`;
const sourceUrl = (() => {
  try {
    const url = new URL(import.meta.env.VITE_SOURCE_URL?.trim() ?? '');
    return ['https:', 'http:'].includes(url.protocol) ? url.href : null;
  } catch { return null; }
})();

export function SimulatorApp({ controller, initialEnv, mode }: {
  controller: SimController; initialEnv: Env; mode: 'mock' | 'real';
}) {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [roomTempC, setRoomTempC] = useState(initialEnv.temperatureC);
  const [ambientLight, setAmbientLight] = useState(initialEnv.lightLevel ?? 0.5);
  const [current, setCurrent] = useState<File | null>(null);
  const [recent, setRecent] = useState<File[]>([]);
  const [info, setInfo] = useState<FileInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const loadingRef = useRef(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [inputEpoch, setInputEpoch] = useState(0);
  const [speed, setSpeed] = useState(1);
  const [settings, setSettings] = useState(false);
  const [dockOpen, setDockOpen] = useState(true);
  const [theme, setTheme] = useState<'dark' | 'light'>(() => {
    try { return localStorage.getItem('lanqiao-theme') === 'light' ? 'light' : 'dark'; }
    catch { return 'dark'; }
  });
  const [dockWidth, setDockWidth] = useState(() => {
    try {
      const width = Number(localStorage.getItem('lanqiao-dock-width'));
      return Number.isFinite(width) && width >= 260 && width <= 720 ? width : 360;
    } catch { return 360; }
  });
  const [rtc, setRtc] = useState(() => {
    const now = new Date();
    return [now.getHours(), now.getMinutes(), now.getSeconds()].map(value => String(value).padStart(2, '0')).join(':');
  });
  const firmwareInput = useRef<HTMLInputElement>(null);
  const importInput = useRef<HTMLInputElement>(null);
  const settingsButton = useRef<HTMLButtonElement>(null);
  const workspace = useRef<HTMLDivElement>(null);
  const fileMenu = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const off = controller.onSnapshot(setSnapshot);
    return () => { off(); controller.pause(); };
  }, [controller]);

  const loaded = info !== null;
  const running = snapshot?.state === 'running';
  const stopped = snapshot?.state === 'error';
  const recovering = snapshot?.recovery === 'recovering';
  const canExecute = loaded && !stopped && !loading && !recovering;

  function openFile() {
    if (!loadingRef.current && !recovering) firmwareInput.current?.click();
    if (fileMenu.current) fileMenu.current.open = false;
  }
  async function load(file: File) {
    if (loadingRef.current || recovering) return;
    if (!/\.hex$/i.test(file.name) || file.size > 2_000_000) {
      setError('请选择不超过 2 MB 的 .hex 固件文件。'); return;
    }
    loadingRef.current = true;
    setLoading(true); setError(''); setNotice('');
    if (fileMenu.current) fileMenu.current.open = false;
    try {
      const result = await controller.load(file.name, await file.arrayBuffer());
      setInfo(result); setCurrent(file);
      setRecent(previous => [file, ...previous.filter(item => item.name !== file.name)].slice(0, 8));
      setInputEpoch(value => value + 1); controller.run();
    } catch (failure) { setError(`${(failure as Error).message} 当前会话保持不变。`); }
    finally { loadingRef.current = false; setLoading(false); }
  }
  function reset(kind: 'mcu' | 'power') {
    if (!loaded || recovering || loading) return;
    setInputEpoch(value => value + 1); setError(''); setNotice(''); controller.reset(kind);
  }
  function closeSettings() { setSettings(false); settingsButton.current?.focus(); }
  function toggleTheme() {
    const next = theme === 'dark' ? 'light' : 'dark';
    setTheme(next);
    try { localStorage.setItem('lanqiao-theme', next); } catch { /* Storage may be blocked. */ }
  }
  function resizeDock(value: number) {
    const next = Math.max(260, Math.min(value, Math.min(720, window.innerWidth / 2)));
    setDockWidth(next);
    try { localStorage.setItem('lanqiao-dock-width', String(next)); } catch { /* Storage may be blocked. */ }
  }
  async function exportState() {
    try {
      const text = exportStateFile(await controller.exportState());
      const url = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
      const link = document.createElement('a');
      link.href = url; link.download = 'lanqiao-eeprom-rtc.json'; link.click();
      setTimeout(() => URL.revokeObjectURL(url), 0);
      setNotice('EEPROM / RTC 状态已导出。'); setError('');
    } catch (failure) { setError((failure as Error).message); }
  }
  async function importState(file: File | undefined) {
    if (!file) return;
    try {
      if (file.size > 65536) throw new Error('状态文件超过 64 KiB，请检查文件。');
      await controller.importState(importStateFile(await file.text()));
      setNotice('EEPROM / RTC 状态已导入。'); setError('');
    } catch (failure) { setError((failure as Error).message); }
  }
  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      handleShortcut(event, {
        run: () => { if (canExecute && !running) controller.run(); },
        pause: () => {
          if (running) controller.pause();
          if (settings) closeSettings();
          if (fileMenu.current) fileMenu.current.open = false;
        },
        step: () => { if (canExecute && !running) controller.step(); },
        reset: () => reset('mcu'), open: openFile,
      });
    };
    window.addEventListener('keydown', listener);
    return () => window.removeEventListener('keydown', listener);
  });

  if (!snapshot) return <main className="app" data-theme={theme} aria-busy="true">正在连接模拟器…</main>;
  const diagnostic = error || (stopped ? `${snapshot.recovery === 'wasm' ? 'WASM 引擎加载失败 · ' : ''}${snapshot.diagnostic ?? '引擎未提供诊断信息。'}` : '');
  return <main className="app" data-theme={theme}>
    <header className="app-toolbar">
      <div className="brand"><svg viewBox="0 0 32 32" aria-hidden="true"><rect x="7" y="7" width="18" height="18" rx="3" />
        <path d="M11 2v5m10-5v5M11 25v5m10-5v5M2 11h5m-5 10h5m18-10h5m-5 10h5M12 12h8v8h-8z" /></svg><h1>蓝桥仿真</h1></div>
      <div className="toolbar-group">
        <button onClick={openFile} disabled={loading || recovering} title="Ctrl+O">{loading ? '正在装载…' : '打开 HEX'}</button>
        <details className="open-menu" ref={fileMenu}><summary aria-label="固件菜单" title="固件菜单"><svg viewBox="0 0 20 20" aria-hidden="true"><path d="m5 7 5 5 5-5" /></svg></summary>
          <div className="menu-content"><button disabled={!current || loading || recovering} onClick={() => { if (current) void load(current); }}>重新加载当前文件</button>
            {recent.length > 0 && <hr />}{recent.map(file => <button key={file.name} title={file.name} disabled={loading || recovering} onClick={() => void load(file)}>{file.name}</button>)}</div>
        </details>
        <span className="control-divider" aria-hidden="true" />
        <button className="primary" disabled={!canExecute || running} onClick={() => controller.run()} title="F5"><svg viewBox="0 0 20 20" aria-hidden="true"><path d="m6 3 10 7-10 7Z" /></svg>{loaded ? '继续' : '运行'}</button>
        <button disabled={!canExecute || !running} onClick={() => controller.pause()} title="Esc / Shift+F5"><svg viewBox="0 0 20 20" aria-hidden="true"><path d="M6 3v14M14 3v14" /></svg>暂停</button>
        <button disabled={!canExecute || running} onClick={() => controller.step()} title="F10">单步</button>
        <span className="control-divider" aria-hidden="true" />
        <button disabled={!loaded || recovering || loading} onClick={() => reset('mcu')} title="Ctrl+F5">MCU 复位</button>
        <button disabled={!loaded || recovering || loading} onClick={() => reset('power')}>重新上电</button>
        <label className="speed-select"><span>速度</span><select aria-label="目标倍速" value={speed} onChange={event => {
          const next = Number(event.target.value); setSpeed(next); controller.setSpeed(next);
        }}>{[.25, .5, 1, 2, 4, 8].map(value => <option key={value} value={value}>{value}×</option>)}<option value={0}>最快</option></select></label>
      </div>
      <div className="toolbar-group">
        <button onClick={() => setDockOpen(value => !value)} aria-pressed={dockOpen} aria-label={dockOpen ? '收起工具面板' : '展开工具面板'}>工具</button>
        <button onClick={toggleTheme} aria-label={theme === 'dark' ? '切换浅色主题' : '切换深色主题'} title="切换主题"><svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="4" /><path d="M10 0v3m0 14v3M0 10h3m14 0h3M3 3l2 2m10 10 2 2M3 17l2-2M15 5l2-2" /></svg></button>
        <button ref={settingsButton} aria-label="设置" aria-expanded={settings} onClick={() => {
          setSettings(value => !value);
        }}><svg viewBox="0 0 20 20" aria-hidden="true"><path d="m8 2 4 0 .5 3 2 1 2.5-1 2 3-2 2v2l2 2-2 3-2.5-1-2 1-.5 3H8l-.5-3-2-1L3 17l-2-3 2-2v-2L1 8l2-3 2.5 1 2-1Z" transform="translate(0 -1) scale(.95)" /><circle cx="9.5" cy="9" r="3" /></svg></button>
      </div>
    </header>
    <input className="sr-only" type="file" accept=".hex" aria-label="选择 HEX 文件" ref={firmwareInput} disabled={loading || recovering}
      onChange={event => { const file = event.target.files?.[0]; if (file) void load(file); event.target.value = ''; }} />
    <div className={`shell-workspace${dockOpen ? '' : ' dock-collapsed'}`} ref={workspace} style={{ '--dock-width': `${dockWidth}px` } as CSSProperties}>
      <BoardView controller={controller} snapshot={loaded ? snapshot : { ...snapshot, state: 'empty' }} inputEpoch={inputEpoch} targetSpeed={speed}
        roomTempC={roomTempC} ambientLight={ambientLight} onFile={file => void load(file)} />
      <div className="dock-splitter" role="separator" aria-label="工具面板宽度" aria-orientation="vertical" aria-valuemin={260} aria-valuemax={720} aria-valuenow={Math.round(dockWidth)} tabIndex={0}
        onPointerDown={event => { if (event.button === 0) { event.preventDefault(); event.currentTarget.setPointerCapture(event.pointerId); } }}
        onPointerMove={event => { if (event.currentTarget.hasPointerCapture(event.pointerId) && workspace.current) resizeDock(workspace.current.getBoundingClientRect().right - event.clientX); }}
        onPointerUp={event => { if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); }}
        onKeyDown={event => { if (['ArrowLeft', 'ArrowRight', 'Home'].includes(event.key)) {
          event.preventDefault(); resizeDock(event.key === 'Home' ? 360 : dockWidth + (event.key === 'ArrowLeft' ? 20 : -20));
        } }} />
      <aside className="tool-dock" aria-label="工具面板"><div className="dock-heading"><h2>工具面板</h2><span className={`mode-badge ${mode}`} data-testid="sim-mode">{mode === 'mock' ? '演示模式' : '真实仿真'}</span></div>
        <UartPanel key={snapshot.session} controller={controller} loaded={loaded && !stopped && !recovering} />
        <DebugPanel snapshot={snapshot} />
      </aside>
    </div>
    <footer className="status-bar">
      <span className={`state ${snapshot.state}`} data-testid="run-state"><i aria-hidden="true" />{stateLabels[snapshot.state]}</span>
      <span>虚拟时间 <output data-testid="virtual-time" title={`${snapshot.timeNs} ns`}>{(snapshot.timeNs / 1e9).toFixed(3)} s</output></span>
      <span>IRC <output data-testid="irc-clock" title={`${snapshot.ircHz} Hz`}>{(snapshot.ircHz / 1e6).toFixed(3)} MHz</output></span>
      <span>{mode === 'mock' ? '演示倍速' : '实际倍速'} <output>{snapshot.speed.toFixed(2)}×</output></span>
      <span>PC <output data-testid="pc">{address(snapshot.pc)}</output></span>
      <div className={`status-diagnostic${diagnostic ? ' error' : ''}`} role={diagnostic ? 'alert' : 'status'} title={diagnostic || snapshot.notice || notice}>
        {diagnostic || (loading ? '正在装载…' : snapshot.notice || notice)}
      </div>
      {snapshot.recovery && controller.retry && <button disabled={recovering || loading} onClick={() => controller.retry?.()}>{recovering ? '恢复中…' : '重试'}</button>}
      {stopped && loaded && <div className="status-recovery"><button aria-label="诊断 MCU 复位" disabled={recovering || loading} onClick={() => reset('mcu')}>复位</button><button aria-label="诊断重新上电" disabled={recovering || loading} onClick={() => reset('power')}>上电</button></div>}
      {diagnostic && !stopped && <button aria-label="关闭错误提示" onClick={() => setError('')}>关闭</button>}
      <span className="status-file" title={info ? `${info.name}\nSHA-256 ${info.sha256}\n${info.programBytes} B · ${address(info.minAddress)}–${address(info.maxAddress)}` : ''}>{info ? `${info.name} · ${info.bytes.toLocaleString()} B` : '无固件'}</span>
    </footer>
    {settings && <aside className="settings-drawer" aria-label="设置面板">
      <div className="drawer-heading"><h2>设置</h2><button onClick={closeSettings} aria-label="关闭设置">关闭</button></div>
      <DownloadSettings controller={controller} ircHz={snapshot.ircHz} disabled={loading || recovering} onApply={hz => { setInputEpoch(value => value + 1); setError(''); setNotice(ircDownloadNotice(hz, loaded)); }} />
      <section className="settings-section"><h2>环境</h2>
        <label className="settings-field"><span>室温 · DS18B20 / °C</span><input aria-label="室温" type="number" min={-55} max={125} step={.1} value={roomTempC} onChange={event => {
          const value = event.target.valueAsNumber; if (Number.isFinite(value) && value >= -55 && value <= 125) setRoomTempC(value);
        }} /></label>
        <label className="settings-field"><span>环境光照</span><input aria-label="环境光照" type="range" min={0} max={1} step={.01} value={ambientLight} onChange={event => setAmbientLight(event.target.valueAsNumber)} /></label>
        <form onSubmit={event => {
          event.preventDefault(); const [h, m, s = 0] = rtc.split(':').map(Number);
          try { controller.setRtc(h, m, s); setNotice(`RTC 已设为 ${rtc}`); setError(''); }
          catch (failure) { setError((failure as Error).message); }
        }}><label className="settings-field"><span>RTC · DS1302</span><input aria-label="RTC · DS1302" type="time" step="1" value={rtc} required onChange={event => setRtc(event.target.value)} /></label><button>设置时间</button></form>
      </section>
      <section className="settings-section"><h2>EEPROM / RTC</h2><div className="persistent-actions"><button disabled={recovering} onClick={exportState}>导出 JSON</button><button disabled={recovering} onClick={() => importInput.current?.click()}>导入 JSON</button></div>
        <input className="sr-only" type="file" accept=".json,application/json" aria-label="导入状态 JSON" ref={importInput} disabled={recovering}
          onChange={event => { void importState(event.target.files?.[0]); event.target.value = ''; }} /></section>
      <section className="settings-section"><h2>外观</h2><label className="settings-field"><span>主题</span><select aria-label="主题" value={theme} onChange={toggleTheme}><option value="dark">深色</option><option value="light">浅色</option></select></label></section>
      <section className="settings-section"><h2>支持范围</h2><SupportPanel />{sourceUrl && <a href={sourceUrl} target="_blank" rel="noopener noreferrer">项目源码（AGPL-3.0-or-later）</a>}</section>
    </aside>}
  </main>;
}
