import { useState } from 'react';
import type { SimController, Snapshot } from '../sim/types';
import { Board25D } from '../board25d';
import { distanceInput } from '../board25d/model';

export interface BoardViewProps {
  controller: SimController;
  snapshot: Snapshot;
  roomTempC: number;
  ambientLight: number;
  onFile: (file: File) => void;
  inputEpoch?: number;
  targetSpeed?: number;
}

export function BoardView({ controller, snapshot, roomTempC, ambientLight, onFile, inputEpoch = 0, targetSpeed = 1 }: BoardViewProps) {
  const [dragging, setDragging] = useState(false);
  return <div className={`board-view${dragging ? ' dragging' : ''}`}
    onDragOver={event => {
      if (!event.dataTransfer.types.includes('Files')) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = 'copy';
      setDragging(true);
    }}
    onDragLeave={event => {
      if (!(event.relatedTarget instanceof Node) || !event.currentTarget.contains(event.relatedTarget)) setDragging(false);
    }}
    onDragEnd={() => setDragging(false)}
    onDrop={event => {
      event.preventDefault();
      setDragging(false);
      const file = event.dataTransfer.files[0];
      if (file) onFile(file);
    }}>
    <Board25D snapshot={snapshot} board={snapshot.board ?? null} inputEpoch={inputEpoch} targetSpeed={targetSpeed}
      roomTempC={roomTempC} ambientLight={ambientLight}
      onKey={(id, down) => controller.setKey(id, down)}
      onJumper={(id, position) => controller.setJumper(id, position)}
      onDial={(id, position) => controller.setDial(id, position)}
      onLight={value => controller.setLight(value)}
      onTemperature={temperatureC => controller.setEnv({ temperatureC })}
      onDistance={value => controller.setEnv({ distanceCm: distanceInput(value) })}
      onMcuReset={() => controller.reset('mcu')} />
    {snapshot.state === 'empty' && <p className="board-empty">拖入 HEX 文件，或 Ctrl+O 打开</p>}
    {dragging && <div className="board-drop-overlay" aria-hidden="true"><span>松开以打开 HEX 文件</span></div>}
  </div>;
}
