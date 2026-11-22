import type { Snapshot } from '../sim/types';

type Clock = Pick<Snapshot, 'timeNs'>;
type Hold = { before: Clock | null; startNs: number | null; elapsedNs: number; releasedAt: number | null };
export type HoldReadout = { id: string; text: string; seconds: number; opacity: number; held: boolean };

export class KeyHoldTimer {
  private readonly holds = new Map<string, Hold>();

  press(id: string, snapshot: Clock | null): void {
    this.holds.set(id, { before: snapshot, startNs: null, elapsedNs: 0, releasedAt: null });
  }

  advance(snapshot: Clock | null): void {
    if (!snapshot) return;
    for (const hold of this.holds.values()) {
      if (hold.releasedAt !== null || snapshot === hold.before) continue;
      // Start at the first received snapshot after the local press, never extrapolate wall time.
      hold.startNs ??= snapshot.timeNs;
      hold.elapsedNs = Math.max(0, snapshot.timeNs - hold.startNs);
    }
  }

  release(id: string, snapshot: Clock | null, wallMs: number): void {
    this.advance(snapshot);
    const hold = this.holds.get(id);
    if (hold && hold.releasedAt === null) hold.releasedAt = wallMs;
  }

  clear(): void { this.holds.clear(); }
  get size(): number { return this.holds.size; }

  read(wallMs: number): HoldReadout[] {
    const result: HoldReadout[] = [];
    for (const [id, hold] of this.holds) {
      const age = hold.releasedAt === null ? 0 : Math.max(0, wallMs - hold.releasedAt);
      if (age >= 3400) { this.holds.delete(id); continue; }
      const seconds = hold.elapsedNs / 1e9;
      result.push({ id, seconds, text: `${seconds.toFixed(seconds >= 10 ? 1 : 2)} s`,
        held: hold.releasedAt === null, opacity: age <= 3000 ? 1 : (3400 - age) / 400 });
    }
    return result;
  }
}
