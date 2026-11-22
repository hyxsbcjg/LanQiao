import { useEffect, useState } from 'react';
import { WorkerSimController } from './sim/controller';
import { DEFAULT_ENV, MockSim } from './sim/mock';
import type { SimController } from './sim/types';
import { SimulatorApp } from './ui/App';
import { TestPage } from './board25d/TestPage';

export function App() {
  const [mock] = useState(() => new URLSearchParams(window.location.search).get('mock') === '1');
  const [board25d] = useState(() => new URLSearchParams(window.location.search).get('board25d') === '1');
  const [controller, setController] = useState<SimController | null>(null);
  useEffect(() => {
    const next = mock ? new MockSim() : new WorkerSimController();
    next.setEnv({ temperatureC: DEFAULT_ENV.temperatureC, distanceCm: DEFAULT_ENV.distanceCm });
    next.setDial('RB2', 0.5);
    next.setDial('RB3', 0.5);
    next.setLight(DEFAULT_ENV.lightLevel ?? 0.5);
    next.setKeyMode('KBD');
    const now = new Date();
    next.setRtc(now.getHours(), now.getMinutes(), now.getSeconds());
    setController(next);
    return () => {
      next.pause();
      if (next instanceof WorkerSimController) next.dispose();
    };
  }, [mock]);
  if (!controller) return <main className="app" aria-busy="true">正在连接模拟器…</main>;
  if (board25d) return <TestPage controller={controller} />;
  return <SimulatorApp controller={controller} initialEnv={DEFAULT_ENV} mode={mock ? 'mock' : 'real'} />;
}
