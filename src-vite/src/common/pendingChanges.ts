export interface PendingChangeGuard {
  scope?: string;
  dirty(): boolean;
  busy?(): boolean;
  revision(): string;
  confirm(): boolean | Promise<boolean>;
}
type Entry = { guard: PendingChangeGuard; pending?: Promise<boolean> };
export function createPendingChangesRegistry() {
  const guards = new Map<string, Entry>();
  function register(id: string, guard: PendingChangeGuard) {
    const entry = { guard }; guards.set(id, entry);
    return () => { if (guards.get(id) === entry) guards.delete(id); };
  }
  function valid(token: { scope?: string; onlyId?: string; entries: Array<{ id: string; entry: Entry; revision: string }> }) {
    for (const [id, entry] of guards) {
      if (token.scope && entry.guard.scope !== token.scope) continue;
      if (token.onlyId && id !== token.onlyId) continue;
      if (entry.guard.busy?.()) return false;
      if (!entry.guard.dirty()) continue;
      const saved = token.entries.find(saved => saved.id === id && saved.entry === entry);
      if (!saved || saved.revision !== entry.guard.revision()) return false;
    }
    return true;
  }
  async function prepare(scope?: string, onlyId?: string) {
    const token = { scope, onlyId, entries: [] as Array<{ id: string; entry: Entry; revision: string }> };
    for (const [id, entry] of [...guards].reverse()) {
      const guard = entry.guard;
      if (scope && guard.scope !== scope) continue;
      if (onlyId && id !== onlyId) continue;
      if (guard.busy?.()) return null;
      if (!guard.dirty()) continue;
      const revision = guard.revision();
      if (!entry.pending) entry.pending = Promise.resolve().then(() => guard.confirm()).catch(() => false).finally(() => { entry.pending = undefined; });
      if (!await entry.pending) return null;
      token.entries.push({ id, entry, revision });
    }
    return valid(token) ? token : null;
  }
  return { register, prepare, valid,
    async confirmOne(id: string) { return !!await prepare(undefined, id); },
    hasPending(scope?: string) { return [...guards.values()].some(({guard}) => (!scope || guard.scope === scope) && (guard.dirty() || guard.busy?.())); },
    async confirm(scope?: string) { return !!await prepare(scope); } };
}
export const pendingChanges = createPendingChangesRegistry();

export interface CloseParticipant {
  prepare(): Promise<boolean>;
  validate(): Promise<boolean>;
  release(): void | Promise<void>;
}
// Consent never clears a draft. No window is destroyed until every participant
// has approved and all snapshots remain current. A cancellation releases all.
export async function coordinateClose(participants: CloseParticipant[], finish: () => Promise<void>) {
  let committed = false;
  try {
    for (const participant of participants) if (!await participant.prepare()) return false;
    for (const participant of participants) if (!await participant.validate()) return false;
    await finish(); committed = true; return true;
  } finally {
    if (!committed) await Promise.allSettled(participants.map(participant => Promise.resolve().then(() => participant.release())));
  }
}
