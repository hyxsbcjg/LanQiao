import { useCallback, useEffect, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from 'react';
import { KEY_MAP } from '../sim/keymap';
import type { DialId, JumperId } from '../sim/types';
import { BoardAudio } from '../audio';
import { clamp, dialAngle, positionDistance, rb3Frequency, rb3Position, thermalStep } from './model';
import { boardLayout, obstacleRect } from './layout';
import { KeyHoldTimer, type HoldReadout } from './holdTimer';
import { drawBoard, loadBoard, type LoadedBoard } from './render';
import type { Board25DProps, Part, Rect } from './types';
import './board25d.css';

type View = { scale: number; x: number; y: number };
type Gesture = { part: Part; x: number; y: number; started: number; moved: boolean; value: number; angle: number | null };
type Popup = { part: Part; value: string };
const defaultView: View = { scale: 1, x: 0, y: 0 };
const keyBinding = (id: string) => KEY_MAP.find(key => key.v2 === id)?.engine;
const isDial = (part: Part): part is Part & { id: DialId } => part.id === 'RB2' || part.id === 'RB3';
const label = (part: Part) => part.type === 'obstacle' || part.type === 'ultrasonic' ? '超声障碍物' : part.label;

export function Board25D(props: Board25DProps) {
  const latest = useRef(props);
  latest.current = props;
  const viewport = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const popupElement = useRef<HTMLDivElement>(null);
  const buttons = useRef(new Map<string, HTMLButtonElement>());
  const gestures = useRef(new Map<number, Gesture>());
  const holds = useRef(new Map<string, Set<string>>());
  const holdTimer = useRef(new KeyHoldTimer());
  const [holdReadouts, setHoldReadouts] = useState<HoldReadout[]>([]);
  const pan = useRef<{ id: number; x: number; y: number; view: View } | null>(null);
  const touches = useRef(new Map<number, { x: number; y: number }>());
  const pinch = useRef<{ distance: number; x: number; y: number; view: View } | null>(null);
  const physics = useRef({ temperature: clamp(props.roomTempC, -55, 125), light: clamp(props.board?.light ?? props.ambientLight),
    cover: false, warm: false, constant: false, lightMoving: false });
  const [loaded, setLoaded] = useState<LoadedBoard | null>(null);
  const [loadError, setLoadError] = useState('');
  const [attempt, setAttempt] = useState(0);
  const [pressed, setPressed] = useState(new Set<string>());
  const [temperature, setTemperature] = useState(physics.current.temperature);
  const [light, setLight] = useState(physics.current.light);
  const [constant, setConstant] = useState(false);
  const [distance, setDistance] = useState<number | null>(100);
  const distanceRef = useRef(distance);
  const [view, setView] = useState(defaultView);
  const viewRef = useRef(view);
  const [width, setWidth] = useState(0);
  const [popup, setPopup] = useState<Popup | null>(null);
  const [popupPosition, setPopupPosition] = useState({ left: 12, top: 12 });
  const [inputError, setInputError] = useState('');
  const [jumperMotion, setJumperMotion] = useState<string | null>(null);
  const motionTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [sound, setSound] = useState(false);
  const [soundError, setSoundError] = useState('');
  const audio = useRef<BoardAudio | null>(null);
  const manifestUrl = props.manifestUrl ?? `${import.meta.env.BASE_URL}board25d/manifest.json`;
  const layout = loaded ? boardLayout(loaded.manifest) : null;

  useEffect(() => {
    const abort = new AbortController();
    setLoaded(null); setLoadError('');
    void loadBoard(manifestUrl, abort.signal).then(value => {
      if (!abort.signal.aborted) setLoaded(value);
    }).catch(cause => {
      if (!abort.signal.aborted) setLoadError(`板图加载失败：${cause instanceof Error ? cause.message : String(cause)}`);
    });
    return () => abort.abort();
  }, [manifestUrl, attempt]);

  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setWidth(element.clientWidth));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const changeView = useCallback((next: View) => {
    setPopup(null);
    const bounds = viewport.current?.getBoundingClientRect();
    const scale = clamp(next.scale, 1, 5);
    const value = { scale, x: clamp(next.x, (bounds?.width ?? 0) * (1 - scale), 0),
      y: clamp(next.y, (bounds?.height ?? 0) * (1 - scale), 0) };
    viewRef.current = value; setView(value);
  }, []);
  useEffect(() => { changeView(viewRef.current); }, [width, changeView]);

  const release = useCallback((id: string, source?: string) => {
    const sources = holds.current.get(id);
    if (!sources) return;
    if (source) sources.delete(source); else sources.clear();
    if (sources.size) return;
    holds.current.delete(id);
    const engineKey = keyBinding(id);
    if (engineKey || id === 'S1' || id === 'S2') {
      audio.current?.update(latest.current.snapshot, latest.current.targetSpeed);
      audio.current?.key(false);
    }
    if (engineKey) latest.current.onKey(engineKey, false);
    if (engineKey) holdTimer.current.release(id, latest.current.snapshot, performance.now());
    if (id === 'RD1') { physics.current.cover = false; physics.current.lightMoving = true; }
    if (id === 'U5') physics.current.warm = false;
    setPressed(new Set(holds.current.keys()));
  }, []);
  const releaseAll = useCallback(() => {
    for (const id of holds.current.keys()) release(id);
    gestures.current.clear(); touches.current.clear(); pinch.current = null; pan.current = null;
  }, [release]);
  const press = (part: Part, source: string) => {
    const sources = holds.current.get(part.id) ?? new Set<string>();
    if (!sources.size) {
      const engineKey = keyBinding(part.id);
      if (part.type === 'key' || part.type === 'power') {
        audio.current?.update(latest.current.snapshot, latest.current.targetSpeed);
        audio.current?.key(true);
      }
      if (engineKey) latest.current.onKey(engineKey, true);
      if (engineKey) holdTimer.current.press(part.id, latest.current.snapshot);
      if (part.id === 'S1') latest.current.onMcuReset();
      // V2 S2 connects VCCA to Q1/Q3's power-control gate, not a matrix/RESET input.
      // ISP power switching is not modeled.
      if (part.id === 'RD1') { physics.current.cover = true; physics.current.lightMoving = true; }
      if (part.id === 'U5') physics.current.warm = true;
    }
    sources.add(source); holds.current.set(part.id, sources);
    setPressed(new Set(holds.current.keys()));
  };
  useEffect(() => {
    const blur = () => {
      audio.current?.setHidden(true);
      releaseAll();
    };
    const visible = () => {
      if (document.hidden) blur();
      else audio.current?.setHidden(false);
    };
    window.addEventListener('blur', blur);
    window.addEventListener('focus', visible);
    document.addEventListener('visibilitychange', visible);
    return () => {
      window.removeEventListener('blur', blur);
      window.removeEventListener('focus', visible);
      document.removeEventListener('visibilitychange', visible);
      releaseAll();
      clearTimeout(motionTimer.current);
    };
  }, [releaseAll]);
  useEffect(() => { releaseAll(); }, [props.snapshot?.session, props.snapshot?.recovery, props.inputEpoch, releaseAll]);
  useEffect(() => {
    holdTimer.current.clear(); setHoldReadouts([]);
  }, [props.snapshot?.session, props.snapshot?.recovery, props.inputEpoch]);
  useEffect(() => { holdTimer.current.advance(props.snapshot); }, [props.snapshot]);
  useEffect(() => {
    let frame = 0;
    let visible = false;
    const tick = () => {
      if (holdTimer.current.size || visible) {
        const values = holdTimer.current.read(performance.now());
        visible = values.length > 0; setHoldReadouts(values);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, []);
  useEffect(() => { if (props.snapshot?.state === 'error') releaseAll(); }, [props.snapshot?.state, releaseAll]);
  useEffect(() => { latest.current.onTemperature(physics.current.temperature); }, []);

  useEffect(() => {
    if (!physics.current.cover) physics.current.lightMoving = true;
  }, [props.ambientLight]);
  useEffect(() => {
    if (props.board && !physics.current.cover && !physics.current.lightMoving) {
      physics.current.light = props.board.light; setLight(props.board.light);
    }
  }, [props.board]);
  useEffect(() => {
    let previous = performance.now();
    const timer = setInterval(() => {
      const now = performance.now();
      const dt = Math.min((now - previous) / 1000, 0.25); previous = now;
      if (document.hidden) return;
      const state = physics.current;
      if (state.lightMoving) {
        const target = state.cover ? 0 : clamp(latest.current.ambientLight);
        const next = target + (state.light - target) * Math.exp(-dt / (state.cover ? 0.2 : 0.25));
        state.light = Math.abs(next - target) < 0.001 ? target : next;
        state.lightMoving = state.cover || state.light !== target;
        setLight(state.light); latest.current.onLight(state.light);
      }
      if (!state.constant) {
        const target = clamp(state.warm ? 33 : latest.current.roomTempC, -55, 125);
        const next = thermalStep(state.temperature, target, dt, state.warm && target > state.temperature);
        if (Math.abs(next - state.temperature) > 0.00001) {
          state.temperature = next; setTemperature(next); latest.current.onTemperature(next);
        }
      }
    }, 1000 / 30);
    return () => clearInterval(timer);
  }, []);

  const exactLight = (value: number) => {
    physics.current.light = clamp(value); physics.current.lightMoving = false;
    setLight(physics.current.light); latest.current.onLight(physics.current.light);
  };
  const exactTemperature = (value: number, fixed: boolean) => {
    const next = clamp(value, -55, 125);
    physics.current.temperature = next; physics.current.constant = fixed;
    setTemperature(next); setConstant(fixed); latest.current.onTemperature(next);
  };
  const changeDistance = (value: number | null) => {
    const next = value === null ? null : clamp(value, 0, 400);
    distanceRef.current = next; setDistance(next); latest.current.onDistance(next);
  };
  const toggleJumper = (part: Part) => {
    const positions = Object.keys(part.positions ?? {});
    const current = latest.current.board?.jumpers[part.id as JumperId];
    if (!positions.length || !current) return undefined;
    const next = positions[(positions.indexOf(current) + 1) % positions.length];
    latest.current.onJumper(part.id as JumperId, next);
    setJumperMotion(part.id); clearTimeout(motionTimer.current);
    motionTimer.current = setTimeout(() => setJumperMotion(null), 140);
    return next;
  };

  useEffect(() => {
    if (!loaded || !canvas.current || !width) return;
    drawBoard(canvas.current, loaded, { board: props.board, snapshot: props.snapshot, pressed, distance, light,
      warming: pressed.has('U5'), temperature, roomTempC: props.roomTempC, jumperMotion },
    // Match the fitted, transformed board rather than rounding the whole viewport up to 3200 px.
    Math.max(1600, Math.min(3200, Math.ceil(canvas.current.getBoundingClientRect().width * (window.devicePixelRatio || 1)))));
  }, [loaded, props.board, props.snapshot, props.roomTempC, pressed, distance, light, temperature, jumperMotion, width, view.scale]);

  const closePopup = (focus = false) => {
    if (focus && popup) buttons.current.get(popup.part.id)?.focus({ preventScroll: true });
    setPopup(null); setInputError('');
  };
  const openPopup = (part: Part, position?: string) => {
    const state = latest.current.board;
    const value = part.id === 'RB2' ? (state?.derived.ain3V ?? 0).toFixed(3)
      : part.id === 'RB3' ? (state?.derived.ne555Hz ?? rb3Frequency(0.5)).toFixed(2)
      : part.type === 'ldr' ? physics.current.light.toFixed(3)
      : part.type === 'thermo' ? physics.current.temperature.toFixed(2)
      : part.type === 'obstacle' || part.type === 'ultrasonic' ? String(distanceRef.current ?? 100)
      : position ?? state?.jumpers[part.id as JumperId] ?? '';
    setInputError(''); setPopup({ part, value });
  };
  useEffect(() => {
    if (!popup) return;
    const element = popupElement.current;
    if (!element) return;
    const reposition = () => {
      const anchor = buttons.current.get(popup.part.id)?.getBoundingClientRect();
      if (!anchor) return;
      const bounds = element.getBoundingClientRect();
      const left = clamp(anchor.left + anchor.width / 2 - bounds.width / 2, 12, Math.max(12, innerWidth - bounds.width - 12));
      const top = anchor.bottom + 8 + bounds.height < innerHeight - 12 ? anchor.bottom + 8 : anchor.top - bounds.height - 8;
      setPopupPosition({ left, top: clamp(top, 12, Math.max(12, innerHeight - bounds.height - 12)) });
    };
    // Open after the physical click finishes; opening during pointerup triggers native light-dismiss.
    const frame = requestAnimationFrame(() => {
      element.showPopover(); reposition();
      element.querySelector<HTMLInputElement | HTMLSelectElement>('input, select')?.focus({ preventScroll: true });
    });
    window.addEventListener('resize', reposition);
    window.addEventListener('scroll', reposition, true);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('resize', reposition);
      window.removeEventListener('scroll', reposition, true);
    };
  }, [popup?.part.id]);

  const zoom = useCallback((scale: number, clientX: number, clientY: number) => {
    const bounds = viewport.current?.getBoundingClientRect();
    if (!bounds) return;
    const old = viewRef.current;
    const ratio = clamp(scale, 1, 5) / old.scale;
    const x = clientX - bounds.left, y = clientY - bounds.top;
    changeView({ scale: scale, x: x - (x - old.x) * ratio, y: y - (y - old.y) * ratio });
  }, [changeView]);
  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    const wheel = (event: WheelEvent) => {
      event.preventDefault();
      const id = (event.target as Element).closest<HTMLElement>('[data-part]')?.dataset.part;
      const step = Math.sign(-event.deltaY) * (event.shiftKey ? 0.001 : 0.01);
      if (id === 'RB2' || id === 'RB3') {
        const value = latest.current.board?.dials[id];
        if (value !== undefined) latest.current.onDial(id, clamp(value + step));
      } else if (id === 'RD1') {
        const next = clamp(physics.current.light + step);
        physics.current.light = next; physics.current.lightMoving = false;
        setLight(next); latest.current.onLight(next);
      } else zoom(viewRef.current.scale * Math.exp(-event.deltaY * 0.0015), event.clientX, event.clientY);
    };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => element.removeEventListener('wheel', wheel);
  }, [zoom]);

  const boardPoint = (clientX: number, clientY: number) => {
    const bounds = canvas.current!.getBoundingClientRect();
    return { x: (clientX - bounds.left) / bounds.width * layout!.width,
      y: (clientY - bounds.top) / bounds.height * layout!.height - layout!.offsetY };
  };
  const obstacle = layout?.obstacle;
  const placeObstacle = (clientX: number, clientY: number) => {
    if (!obstacle?.track) return;
    const point = boardPoint(clientX, clientY);
    const { start, end } = obstacle.track;
    const dx = end[0] - start[0], dy = end[1] - start[1];
    const raw = ((point.x - start[0]) * dx + (point.y - start[1]) * dy) / (dx * dx + dy * dy);
    const along = clamp(raw);
    const perpendicular = Math.hypot(point.x - start[0] - along * dx, point.y - start[1] - along * dy);
    changeDistance(perpendicular > obstacle.width / 2 + 24 || raw < -0.45 || raw > 1.45 ? null : Math.round(positionDistance(along) * 10) / 10);
  };

  const beginPart = (event: ReactPointerEvent<HTMLButtonElement>, part: Part) => {
    if (event.button !== 0 || pinch.current) return;
    event.preventDefault(); event.currentTarget.focus({ preventScroll: true });
    event.currentTarget.setPointerCapture(event.pointerId);
    setPopup(null);
    const bounds = event.currentTarget.getBoundingClientRect();
    const dx = event.clientX - bounds.left - bounds.width / 2, dy = event.clientY - bounds.top - bounds.height / 2;
    gestures.current.set(event.pointerId, { part, x: event.clientX, y: event.clientY, started: performance.now(), moved: false,
      value: isDial(part) ? latest.current.board?.dials[part.id] ?? 0 : 0,
      angle: Math.hypot(dx, dy) > 8 ? Math.atan2(dy, dx) : null });
    if (['key', 'power', 'ldr', 'thermo'].includes(part.type)) press(part, `pointer:${event.pointerId}`);
  };
  const movePart = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const gesture = gestures.current.get(event.pointerId);
    if (!gesture || pinch.current) return;
    const { part } = gesture;
    const dx = event.clientX - gesture.x, dy = event.clientY - gesture.y;
    if (Math.hypot(dx, dy) > 4) gesture.moved = true;
    if (isDial(part) && gesture.moved) {
      let value = gesture.value + (dx - dy) / 180;
      if (gesture.angle !== null) {
        const bounds = event.currentTarget.getBoundingClientRect();
        const angle = Math.atan2(event.clientY - bounds.top - bounds.height / 2, event.clientX - bounds.left - bounds.width / 2);
        const delta = Math.atan2(Math.sin(angle - gesture.angle), Math.cos(angle - gesture.angle));
        value = gesture.value - delta * 180 / Math.PI / ((part.maxDeg ?? 135) - (part.minDeg ?? -135));
      }
      latest.current.onDial(part.id, clamp(value));
    } else if (part.type === 'obstacle' && gesture.moved) placeObstacle(event.clientX, event.clientY);
    else if (['key', 'power', 'ldr', 'thermo'].includes(part.type)) {
      const bounds = event.currentTarget.getBoundingClientRect();
      if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom)
        endPart(event, false);
    }
  };
  const endPart = (event: ReactPointerEvent<HTMLButtonElement>, activate: boolean) => {
    const gesture = gestures.current.get(event.pointerId);
    if (!gesture) return;
    gestures.current.delete(event.pointerId);
    release(gesture.part.id, `pointer:${event.pointerId}`);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (!activate || pinch.current) return;
    const { part } = gesture;
    if (part.type === 'jumper') { const position = toggleJumper(part); if (!gesture.moved) openPopup(part, position); }
    else if (!gesture.moved && !['key', 'power'].includes(part.type)
      && (!['ldr', 'thermo'].includes(part.type) || performance.now() - gesture.started < 350)) openPopup(part);
  };

  useEffect(() => {
    audio.current?.update(props.snapshot, props.targetSpeed);
  }, [props.snapshot, props.targetSpeed]);
  useEffect(() => () => {
    void audio.current?.close(); audio.current = null;
  }, []);
  const toggleSound = async () => {
    try {
      if (!audio.current) {
        audio.current = new BoardAudio(new AudioContext(), import.meta.env.DEV ? entry => {
          const log = window.__lqAudioLog ??= [];
          log.push(entry);
          if (log.length > 2048) log.splice(0, log.length - 2048);
        } : undefined);
        audio.current.update(latest.current.snapshot, latest.current.targetSpeed);
        audio.current.setHidden(document.hidden);
      }
      await audio.current.setEnabled(!sound); setSound(value => !value); setSoundError('');
    } catch { setSoundError('声音不可用'); }
  };

  const hitRect = (part: Part): Rect => {
    if (part.type === 'obstacle') return obstacleRect(obstacle!, distance);
    if (!part.hitPolygon?.length) return part;
    const xs = part.hitPolygon.map(point => point[0]), ys = part.hitPolygon.map(point => point[1]);
    return { x: Math.min(...xs), y: Math.min(...ys), width: Math.max(...xs) - Math.min(...xs), height: Math.max(...ys) - Math.min(...ys) };
  };
  const partStyle = (part: Part): CSSProperties => {
    const rect = hitRect(part);
    return { left: `${(rect.x + rect.width / 2) / layout!.width * 100}%`, top: `${(rect.y + rect.height / 2 + layout!.offsetY) / layout!.height * 100}%`,
      width: `${rect.width / layout!.width * 100}%`, height: `${rect.height / layout!.height * 100}%` };
  };
  const preciseLabel = popup?.part.id === 'RB2' ? 'AIN3 (V)' : popup?.part.id === 'RB3' ? 'NE555 (Hz)'
    : popup?.part.type === 'ldr' ? '光照 (0–1)' : popup?.part.type === 'thermo' ? '温度 (°C)' : '距离 (cm)';
  const timerBoxes: Rect[] = [];
  const keyRects = loaded?.manifest.items.filter(part => part.type === 'key').map(part => {
    const rect = hitRect(part), scale = width / layout!.width * view.scale;
    const keyWidth = Math.max(24 * view.scale, rect.width * scale), keyHeight = Math.max(24 * view.scale, rect.height * scale);
    return { id: part.id, x: (rect.x + rect.width / 2) * scale + view.x - keyWidth / 2,
      y: (rect.y + rect.height / 2 + layout!.offsetY) * scale + view.y - keyHeight / 2,
      width: keyWidth, height: keyHeight };
  }) ?? [];
  const timerPosition = (id: string): CSSProperties => {
    const key = keyRects.find(key => key.id === id);
    if (!key) return { display: 'none' };
    const w = 58, h = 18;
    const obstacles = [...keyRects.filter(rect => rect.id !== id).map(rect => ({ ...rect,
      y: rect.y - Math.max(6, width / 3200 * view.scale * 35), height: rect.height + Math.max(6, width / 3200 * view.scale * 35) })), ...timerBoxes];
    const candidates = [
      { x: key.x + key.width / 2 - w / 2, y: key.y + key.height + 2 },
      { x: key.x - w - 3, y: key.y + key.height / 2 - h / 2 },
      { x: key.x + key.width + 3, y: key.y + key.height / 2 - h / 2 },
    ];
    const right = Math.max(...keyRects.map(rect => rect.x + rect.width)) + 6;
    for (let row = 0; row <= holdReadouts.length; row++) candidates.push({ x: right, y: key.y + row * (h + 2) });
    const box = candidates.find(box => box.x >= 2 && box.x + w <= width - 2 && !obstacles.some(rect =>
      box.x < rect.x + rect.width && box.x + w > rect.x && box.y < rect.y + rect.height && box.y + h > rect.y)) ?? candidates.at(-1)!;
    timerBoxes.push({ ...box, width: w, height: h });
    return { left: box.x, top: box.y };
  };

  return <section className="b25" data-testid="board25d" aria-label="V2 交互实验板">
    <div className="b25-tools" role="group" aria-label="板图视图">
      <output aria-label="板图缩放">{Math.round(view.scale * 100)}%</output>
      <button type="button" aria-label="缩小板图" disabled={view.scale <= 1} onClick={() => {
        const box = viewport.current!.getBoundingClientRect(); zoom(view.scale / 1.25, box.x + box.width / 2, box.y + box.height / 2);
      }}>缩小</button>
      <button type="button" aria-label="放大板图" disabled={view.scale >= 5} onClick={() => {
        const box = viewport.current!.getBoundingClientRect(); zoom(view.scale * 1.25, box.x + box.width / 2, box.y + box.height / 2);
      }}>放大</button>
      <button type="button" onClick={() => { changeView(defaultView); setPopup(null); }}>复原视图</button>
      <button type="button" aria-pressed={sound} onClick={() => void toggleSound()}>声音{sound ? '开' : '关'}</button>
      {soundError && <span className="b25-sound-error" role="status">{soundError}</span>}
    </div>
    <div ref={viewport} className="b25-viewport" data-testid="board25d-viewport"
      style={{ aspectRatio: layout ? `${layout.width} / ${layout.height}` : '3200 / 2222',
        '--b25-aspect': layout ? layout.width / layout.height : 3200 / 2222 } as CSSProperties}
      onContextMenu={event => event.preventDefault()}
      onPointerDownCapture={event => {
        if (event.pointerType !== 'touch') return;
        touches.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
        if (touches.current.size === 2) {
          const [a, b] = [...touches.current.values()];
          for (const id of holds.current.keys()) release(id);
          gestures.current.clear(); pan.current = null;
          pinch.current = { distance: Math.hypot(a.x - b.x, a.y - b.y), x: (a.x + b.x) / 2, y: (a.y + b.y) / 2, view: viewRef.current };
          event.currentTarget.setPointerCapture(event.pointerId);
        }
      }}
      onPointerDown={event => {
        if (event.button !== 0 || pinch.current || (event.target as Element).closest('button')) return;
        event.preventDefault(); setPopup(null);
        event.currentTarget.setPointerCapture(event.pointerId);
        pan.current = { id: event.pointerId, x: event.clientX, y: event.clientY, view: viewRef.current };
      }}
      onPointerMoveCapture={event => {
        if (touches.current.has(event.pointerId)) touches.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
        if (pinch.current && touches.current.size >= 2) {
          event.preventDefault(); event.stopPropagation();
          const [a, b] = [...touches.current.values()], initial = pinch.current;
          const bounds = event.currentTarget.getBoundingClientRect();
          const ratio = clamp(initial.view.scale * Math.hypot(a.x - b.x, a.y - b.y) / Math.max(1, initial.distance), 1, 5) / initial.view.scale;
          changeView({ scale: initial.view.scale * ratio,
            x: (a.x + b.x) / 2 - bounds.left - (initial.x - bounds.left - initial.view.x) * ratio,
            y: (a.y + b.y) / 2 - bounds.top - (initial.y - bounds.top - initial.view.y) * ratio });
        } else if (pan.current?.id === event.pointerId) {
          const initial = pan.current;
          changeView({ scale: initial.view.scale, x: initial.view.x + event.clientX - initial.x, y: initial.view.y + event.clientY - initial.y });
        }
      }}
      onPointerUpCapture={event => {
        touches.current.delete(event.pointerId);
        if (touches.current.size < 2) pinch.current = null;
        if (pan.current?.id === event.pointerId) pan.current = null;
      }}
      onPointerCancelCapture={releaseAll}>
      <div className="b25-stage" style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})` }}>
        <canvas ref={canvas} data-testid="board25d-canvas" role="img"
          aria-label="V2 实验板，八位数码管、LED、跳线与传感器" />
        {loaded?.manifest.items.filter(part => !['segment', 'led'].includes(part.type) && part.simulationBinding !== 'visual-only').map(part => {
          const position = props.board?.jumpers[part.id as JumperId];
          const value = isDial(part) ? props.board?.dials[part.id] : part.type === 'ldr' ? light : part.type === 'thermo' ? temperature : distance;
          const key = part.type === 'key' || part.type === 'power';
          return <button key={part.id} type="button" ref={element => { if (element) buttons.current.set(part.id, element); else buttons.current.delete(part.id); }}
            className={`b25-part b25-part-${part.type}${pressed.has(part.id) ? ' b25-held' : ''}${part.type === 'obstacle' && distance === null ? ' b25-absent' : ''}`}
            style={partStyle(part)} data-part={part.id} data-value={value ?? undefined} data-position={position}
            role={isDial(part) ? 'slider' : undefined}
            aria-label={`板上${key ? '按键' : ''} ${part.type === 'ultrasonic' ? part.id : label(part)}`}
            aria-pressed={key || part.type === 'ldr' || part.type === 'thermo' ? pressed.has(part.id)
              : part.type === 'jumper' ? position !== Object.keys(part.positions ?? {})[0] : undefined}
            aria-description={position}
            aria-valuetext={isDial(part) ? `${part.id === 'RB2' ? props.board?.derived.ain3V.toFixed(3) : props.board?.derived.ne555Hz.toFixed(1)} ${part.id === 'RB2' ? 'V' : 'Hz'}` : undefined}
            aria-valuenow={isDial(part) ? value ?? 0 : undefined}
            aria-valuemin={isDial(part) ? 0 : undefined} aria-valuemax={isDial(part) ? 1 : undefined}
            aria-haspopup={!key ? 'dialog' : undefined} title={key ? part.id : `${label(part)}${position ? ` · ${position}` : ''}`}
            onPointerDown={event => beginPart(event, part)} onPointerMove={movePart}
            onPointerUp={event => endPart(event, true)} onPointerCancel={event => endPart(event, false)}
            onPointerLeave={event => { if (key || part.type === 'ldr' || part.type === 'thermo') endPart(event, false); }}
            onLostPointerCapture={event => endPart(event, false)}
            onBlur={() => release(part.id)}
            onContextMenu={event => { event.preventDefault(); if (!key) openPopup(part); }}
            onKeyDown={event => {
              if (event.key === 'Escape') { release(part.id); closePopup(); return; }
              if (isDial(part) && ['ArrowUp', 'ArrowRight', 'ArrowDown', 'ArrowLeft', 'Home', 'End'].includes(event.key)) {
                event.preventDefault(); setPopup(null);
                const current = props.board?.dials[part.id] ?? 0;
                const step = event.shiftKey ? 0.001 : 0.01;
                props.onDial(part.id, event.key === 'Home' ? 0 : event.key === 'End' ? 1
                  : clamp(current + (event.key === 'ArrowUp' || event.key === 'ArrowRight' ? step : -step)));
              } else if (part.type === 'obstacle' && ['ArrowUp', 'ArrowRight', 'ArrowDown', 'ArrowLeft', 'Home', 'End', 'Delete'].includes(event.key)) {
                event.preventDefault(); setPopup(null);
                changeDistance(event.key === 'Delete' ? null : event.key === 'Home' ? 0 : event.key === 'End' ? 400
                  : (distanceRef.current ?? 100) + (event.key === 'ArrowUp' || event.key === 'ArrowRight' ? 1 : -1));
              } else if ((event.key === ' ' && (key || part.type === 'ldr' || part.type === 'thermo')) || (event.key === 'Enter' && key)) {
                event.preventDefault(); if (!event.repeat) press(part, `keyboard:${event.key}`);
              } else if ((event.key === 'Enter' || event.key === ' ') && part.type !== 'jumper') {
                event.preventDefault(); if (!event.repeat) openPopup(part);
              }
            }}
            onKeyUp={event => { if (event.key === ' ' || event.key === 'Enter') { release(part.id, `keyboard:${event.key}`); } }}
            onClick={event => {
              if (event.detail !== 0) return;
              if (key) { if (holds.current.has(part.id)) release(part.id); else press(part, 'activation'); }
              else if (part.type === 'jumper') { toggleJumper(part); setPopup(null); }
              else openPopup(part);
            }} />;
        })}
      </div>
      {holdReadouts.map(value => <output key={value.id} className="b25-hold-time" data-testid={`hold-${value.id}`}
        data-held={value.held} data-seconds={value.seconds} aria-label={`${value.id} 按住时间`} aria-live="off"
        style={{ ...timerPosition(value.id), opacity: value.opacity }}>{value.text}</output>)}
      {!loaded && <div className="b25-loading" role={loadError ? 'alert' : 'status'}>
        {loadError || '正在加载板图…'}{loadError && <button type="button" onClick={() => setAttempt(value => value + 1)}>重试板图</button>}
      </div>}
    </div>
    {popup && <div key={popup.part.id} ref={popupElement} popover="auto" className="b25-popover" role="dialog" aria-label={`${label(popup.part)} 精确设置`}
      style={popupPosition} onToggle={event => { if (event.newState === 'closed' && popupElement.current === event.currentTarget) setPopup(null); }}>
      <header><h3>{label(popup.part)}</h3><button type="button" onClick={() => closePopup(true)}>关闭</button></header>
      <form noValidate onSubmit={event => {
        event.preventDefault();
        const part = popup.part;
        if (part.type === 'jumper') props.onJumper(part.id as JumperId, popup.value);
        else {
          const value = popup.value.trim() ? Number(popup.value) : NaN;
          if (!Number.isFinite(value)) { setInputError('请输入有效数值。'); return; }
          if (part.id === 'RB2') props.onDial('RB2', clamp(value / 5));
          else if (part.id === 'RB3') props.onDial('RB3', rb3Position(value));
          else if (part.type === 'ldr') exactLight(value);
          else if (part.type === 'thermo') exactTemperature(value, true);
          else changeDistance(value);
        }
        closePopup(true);
      }}>
        {popup.part.type === 'jumper' ? <label>位置<select value={popup.value} onChange={event => setPopup({ ...popup, value: event.target.value })}>
          {Object.keys(popup.part.positions ?? {}).map(value => <option key={value}>{value}</option>)}
        </select></label> : <label>{preciseLabel}<input type="number" step="any" value={popup.value}
          onChange={event => { setPopup({ ...popup, value: event.target.value }); setInputError(''); }} /></label>}
        {isDial(popup.part) && <output className="b25-precise-value">
          {popup.part.id === 'RB2' ? `${props.board?.derived.ain3V.toFixed(3) ?? '0'} V` : `${props.board?.derived.ne555Hz.toFixed(2) ?? '0'} Hz`}
          <span>{dialAngle(props.board?.dials[popup.part.id] ?? 0, popup.part.minDeg, popup.part.maxDeg).toFixed(1)}°</span>
        </output>}
        {popup.part.type === 'thermo' && <output>{constant ? '恒温' : pressed.has('U5') ? '接触' : '室温'}</output>}
        {inputError && <p role="alert">{inputError}</p>}
        <div className="b25-popover-actions"><button type="submit">应用</button>
          {popup.part.type === 'thermo' && <button type="button" onClick={() => { exactTemperature(props.roomTempC, false); closePopup(true); }}>恢复室温</button>}
          {(popup.part.type === 'obstacle' || popup.part.type === 'ultrasonic') && <button type="button" onClick={() => { changeDistance(null); closePopup(true); }}>无障碍物</button>}
        </div>
      </form>
    </div>}
  </section>;
}

export default Board25D;
