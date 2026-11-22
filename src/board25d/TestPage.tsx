import { useEffect, useRef, useState } from 'react';
import type { FileInfo, SimController, Snapshot } from '../sim/types';
import { displayText } from '../ui/display';
import { Board25D } from './Board25D';
import { distanceInput } from './model';
import './test-page.css';

const roomTempC = 25;
const ambientLight = 0.65;
const stateLabels = { empty: '等待固件', paused: '已暂停', running: '运行中', error: '已停止' };

export function TestPage({ controller }: { controller: SimController }) {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [selected, setSelected] = useState<File | null>(null);
  const [info, setInfo] = useState<FileInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [temperature, setTemperature] = useState(roomTempC);
  const [light, setLight] = useState(ambientLight);
  const [distance, setDistance] = useState<number | null>(100);
  const initialized = useRef<SimController | null>(null);

  useEffect(() => {
    const off = controller.onSnapshot(setSnapshot);
    if (initialized.current !== controller) {
      initialized.current = controller;
      controller.setLight(ambientLight);
    }
    return off;
  }, [controller]);

  async function load() {
    if (!selected || loading) return;
    setLoading(true);
    setError('');
    try {
      if (!/\.hex$/i.test(selected.name)) throw new Error('请选择 .hex 固件文件。');
      if (selected.size > 2_000_000) throw new Error('HEX 文件超过 2 MB。');
      setInfo(await controller.load(selected.name, await selected.arrayBuffer()));
    } catch (cause) {
      setError(`${cause instanceof Error ? cause.message : String(cause)} 当前会话保持不变。`);
    } finally { setLoading(false); }
  }

  const running = snapshot?.state === 'running';
  const disabled = !info || loading || snapshot?.state === 'error';
  return <main className="b25-test">
    <header className="b25-test-header">
      <h1>2.5D 实验板测试</h1>
      <div className="b25-test-controls">
        <label>选择 HEX 文件<input type="file" accept=".hex" aria-label="选择 HEX 文件" disabled={loading}
          onChange={event => { setSelected(event.target.files?.[0] ?? null); setError(''); }} /></label>
        <button disabled={!selected || loading} onClick={() => void load()}>{loading ? '正在装载…' : '装载 HEX'}</button>
        <button disabled={disabled || running} onClick={() => controller.run()}>运行</button>
        <button disabled={disabled || !running} onClick={() => controller.pause()}>暂停</button>
      </div>
      <p className="b25-test-session">
        <output data-testid="run-state">{stateLabels[snapshot?.state ?? 'empty']}</output>
        <span>会话 {snapshot?.session ?? 0}</span>
        <span>{info?.name ?? '未装载固件'}</span>
        <span>虚拟时间 <output data-testid="virtual-time" data-ns={snapshot?.timeNs ?? 0}>
          {((snapshot?.timeNs ?? 0) / 1e9).toFixed(3)} s</output></span>
      </p>
    </header>
    {error && <p role="alert">{error}</p>}
    {snapshot?.state === 'error' && <p role="alert">{snapshot.diagnostic}</p>}
    <Board25D snapshot={snapshot} board={snapshot?.board ?? null} roomTempC={roomTempC} ambientLight={ambientLight}
      onKey={(id, down) => controller.setKey(id, down)}
      onJumper={(id, position) => controller.setJumper(id, position)}
      onDial={(id, position) => controller.setDial(id, position)}
      onLight={value => { controller.setLight(value); setLight(value); }}
      onTemperature={value => { controller.setEnv({ temperatureC: value }); setTemperature(value); }}
      onDistance={value => {
        // A missing obstacle must keep RX waiting; numeric zero instead produces an immediate echo.
        controller.setEnv({ distanceCm: distanceInput(value) });
        setDistance(value);
      }}
      onMcuReset={() => controller.reset('mcu')} />
    <section className="b25-test-values" aria-label="当前会话数值">
      <dl>
        <div><dt>数码管</dt><dd><output data-testid="display">{displayText(snapshot?.digits ?? Array(8).fill(null))}</output></dd></div>
        <div><dt>温度 (°C)</dt><dd><output data-testid="sensor-temperature" data-value={temperature}>{temperature}</output></dd></div>
        <div><dt>距离 (cm)</dt><dd><output data-testid="sensor-distance" data-value={distance ?? 0}>{distance ?? '无障碍物'}</output></dd></div>
        <div><dt>光照 (0–1)</dt><dd><output data-testid="sensor-light" data-value={light}>{light}</output></dd></div>
      </dl>
      <details><summary>引擎板级状态</summary><pre data-testid="board-state">{JSON.stringify(snapshot?.board ?? null, null, 2)}</pre></details>
    </section>
  </main>;
}
