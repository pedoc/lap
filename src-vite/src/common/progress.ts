/** Unknown totals stay indeterminate; unfinished work must not round up to 100%. */
export function progressPercent(current: unknown, total: unknown): number | null {
  if (typeof current !== 'number' || typeof total !== 'number' || !Number.isFinite(current) || !Number.isFinite(total) || total <= 0) return null;
  return Math.floor(Math.max(0, Math.min(1, current / total)) * 1000) / 10;
}
