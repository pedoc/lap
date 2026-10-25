export const REASONING_LEVELS = ['none', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra'] as const;
export interface ModelReasoning { levels: string[]; defaultEffort: string | null }

export function reasoningLevelError(value: unknown): 'reasoning_level_invalid' | null {
  return typeof value === 'string' && /^[A-Za-z0-9_-]{1,64}$/.test(value) ? null : 'reasoning_level_invalid';
}

export function normalizeModelReasoning(value?: unknown): ModelReasoning {
  const raw = value && typeof value === 'object' && !Array.isArray(value) ? value as Partial<ModelReasoning> : {};
  const levels = [...new Set((Array.isArray(raw.levels) ? raw.levels : [])
    .filter((level): level is string => typeof level === 'string').map(level => level.trim()).filter(level => !reasoningLevelError(level)))];
  const requested = typeof raw.defaultEffort === 'string' ? raw.defaultEffort.trim() : null;
  return { levels, defaultEffort: requested && levels.includes(requested) ? requested : null };
}

export function modelReasoningError(value?: unknown): 'reasoning_invalid' | 'reasoning_default_invalid' | null {
  if (value === undefined || value === null) return null;
  if (typeof value !== 'object' || Array.isArray(value)) return 'reasoning_invalid';
  const raw = value as Partial<ModelReasoning>;
  if (!Array.isArray(raw.levels) || raw.levels.some(level => typeof level !== 'string' || reasoningLevelError(level.trim()))) return 'reasoning_invalid';
  if (raw.defaultEffort === undefined || raw.defaultEffort === null) return null;
  if (typeof raw.defaultEffort !== 'string' || !normalizeModelReasoning(value).levels.includes(raw.defaultEffort.trim())) return 'reasoning_default_invalid';
  return null;
}
