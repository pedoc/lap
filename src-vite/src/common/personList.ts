/** Refresh counts/previews in place without moving existing rows or stealing selection. */
export function reconcilePeople<T extends { id: number }>(existing: T[], incoming: T[]): T[] {
  const fresh = new Map(incoming.map(person => [person.id, person]));
  const result: T[] = [];
  for (const person of existing) {
    const update = fresh.get(person.id);
    if (!update) continue;
    Object.assign(person, update);
    result.push(person);
    fresh.delete(person.id);
  }
  result.push(...fresh.values());
  return result;
}
export function canUsePersonCover(item: { personId: number | null; faceId: number | null; state: string }, personId: number | null): boolean {
  return !!personId && item.personId === personId && item.faceId != null && ['suggested', 'confirmed'].includes(item.state);
}
