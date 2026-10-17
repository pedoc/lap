export interface ObjectBoxLike {
  id: string;
  bounds: number[];
}
export interface ObjectMatch {
  human: ObjectBoxLike;
  model: ObjectBoxLike;
}
export interface ObjectDiff {
  candidates: ObjectBoxLike[];
  humanOnly: ObjectBoxLike[];
  matched: ObjectMatch[];
}
function iou(a: number[], b: number[]) {
  if (
    !Array.isArray(a) ||
    a.length !== 4 ||
    !Array.isArray(b) ||
    b.length !== 4 ||
    ![...a, ...b].every(Number.isFinite) ||
    a[2] <= 0 ||
    a[3] <= 0 ||
    b[2] <= 0 ||
    b[3] <= 0
  )
    return 0;
  const l = Math.max(a[0], b[0]),
    t = Math.max(a[1], b[1]),
    r = Math.min(a[0] + a[2], b[0] + b[2]),
    btm = Math.min(a[1] + a[3], b[1] + b[3]);
  if (r <= l || btm <= t) return 0;
  const inter = (r - l) * (btm - t);
  const union = a[2] * a[3] + b[2] * b[3] - inter;
  return union > 0 ? inter / union : 0;
}
export function diffObjects(
  human: ObjectBoxLike[],
  model: ObjectBoxLike[],
  threshold = 0.5,
): ObjectDiff {
  const candidates: ObjectBoxLike[] = [];
  const matched: ObjectMatch[] = [];
  const usedHuman = new Set<string>();
  for (const m of model) {
    let best: { human: ObjectBoxLike; score: number } | null = null;
    for (const h of human) {
      const score = iou(m.bounds, h.bounds);
      if (score >= threshold && (!best || score > best.score))
        best = { human: h, score };
    }
    if (best) {
      matched.push({ human: best.human, model: m });
      usedHuman.add(best.human.id);
    } else {
      candidates.push(m);
    }
  }
  const humanOnly = human.filter((h) => !usedHuman.has(h.id));
  return { candidates, humanOnly, matched };
}
