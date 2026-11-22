import { useState } from 'react';
import { IRC_PRESETS_MHZ, validateIrcHz } from '../sim/irc-clock';
import type { SimController } from '../sim/types';

export function DownloadSettings({ controller, ircHz, disabled, onApply }: {
  controller: SimController; ircHz: number; disabled: boolean; onApply: (hz: number) => void;
}) {
  const [choice, setChoice] = useState(() => IRC_PRESETS_MHZ.some(mhz => Math.round(mhz * 1e6) === ircHz) ? String(ircHz) : 'custom');
  const [custom, setCustom] = useState((ircHz / 1e6).toFixed(3));
  const [error, setError] = useState('');
  return <section className="settings-section"><h2>下载设置 (STC-ISP)</h2>
    <form onSubmit={event => {
      event.preventDefault();
      try {
        const hz = choice === 'custom' ? Math.round(Number(custom) * 1e6) : Number(choice);
        validateIrcHz(hz);
        controller.setIrcHz(hz);
        setError(''); onApply(hz);
      } catch (failure) { setError((failure as Error).message); }
    }}>
      <label className="settings-field"><span>IRC 频率</span><select aria-label="IRC 频率" value={choice} disabled={disabled}
        onChange={event => { setChoice(event.target.value); setError(''); }}>
        {IRC_PRESETS_MHZ.map(mhz => <option key={mhz} value={Math.round(mhz * 1e6)}>{mhz} MHz{mhz === 12 ? '（默认）' : ''}</option>)}
        <option value="custom">自定义</option>
      </select></label>
      {choice === 'custom' && <label className="settings-field"><span>自定义 / MHz</span><input aria-label="自定义 IRC 频率 (MHz)"
        type="number" min={5} max={35} step={0.001} required value={custom} disabled={disabled}
        onChange={event => { setCustom(event.target.value); setError(''); }} /></label>}
      <p>应用后重新上电，保留固件与 EEPROM / RTC。</p>
      <button disabled={disabled}>应用 IRC 频率</button>
      {error && <p role="alert">{error}</p>}
    </form>
  </section>;
}
