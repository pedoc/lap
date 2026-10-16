/** Virtual organization is independent of query semantics and original files. */
export type SmartRule = {
  id: string;
  field: string;
  operator: string;
  value: any;
};
export const MAX_SMART_RULES = 100;
export const MAX_SMART_DEPTH = 6;
export const isSmartFolder = (node: any) => node?.kind === "folder";
export const smartChildren = (rule: SmartRule): SmartRule[] =>
  rule.field === "group" && Array.isArray(rule.value?.rules)
    ? rule.value.rules
    : [];
export function visitSmartRules(
  rules: SmartRule[],
  visit: (rule: SmartRule) => void,
) {
  for (const rule of rules || []) {
    visit(rule);
    if (rule.field === "group") visitSmartRules(smartChildren(rule), visit);
  }
}
export function smartRuleCount(rules: SmartRule[]): number {
  let count = 0;
  visitSmartRules(rules, () => count++);
  return count;
}
export function hasTimeSmartRules(rules: SmartRule[]): boolean {
  let result = false;
  visitSmartRules(rules, (rule) => {
    if (
      rule.field === "anniversary" ||
      (["date_taken", "date_created", "date_modified"].includes(rule.field) &&
        (["in_last", "older_than"].includes(rule.operator) ||
          ["this_week", "this_month", "this_year"].includes(rule.value)))
    )
      result = true;
  });
  return result;
}
export function hasSemanticRules(rules: SmartRule[]): boolean {
  let result = false;
  visitSmartRules(rules, (r) => {
    if (["semantic", "similar_image"].includes(r.field)) result = true;
  });
  return result;
}
export function hasObjectRules(rules: SmartRule[]): boolean {
  let result = false;
  visitSmartRules(rules, rule => { if (['object_state','object_class','object_count'].includes(rule.field)) result = true; });
  return result;
}
export function hasOcrRules(rules: SmartRule[]): boolean {
  let result = false;
  visitSmartRules(rules, rule => { if (['ocr_text', 'ocr_state'].includes(rule.field)) result = true; });
  return result;
}
export function smartQueryParams(album: any) {
  return {
    version: 2,
    match: album.query?.match === "any" ? "any" : "all",
    rules: album.query?.rules || [],
    includeFileIds: album.query?.includeFileIds || [],
    excludeFileIds: album.query?.excludeFileIds || [],
    sortType: Number(album.sort?.type || 0),
    sortOrder: Number(album.sort?.order ?? 1),
    groupBy: Number(album.group?.type || 0),
  };
}
export function smartResultSignature(album: any): string {
  if (!album || isSmartFolder(album)) return "";
  return JSON.stringify(smartQueryParams(album));
}
export function canMoveSmartNode(
  nodes: any[],
  id: string,
  parentId: string | null,
): boolean {
  if (!nodes.some((n) => n.id === id) || id === parentId) return false;
  const seen = new Set<string>([id]);
  let cursor = parentId;
  while (cursor) {
    if (seen.has(cursor)) return false;
    seen.add(cursor);
    const parent = nodes.find((n) => n.id === cursor);
    if (!isSmartFolder(parent)) return false;
    cursor = parent.parentId || null;
  }
  return true;
}
export function moveSmartNode(
  nodes: any[],
  id: string,
  parentId: string | null,
): any[] {
  if (!canMoveSmartNode(nodes, id, parentId)) return nodes;
  const node = nodes.find((n) => n.id === id);
  return [
    ...nodes
      .filter((n) => n.id !== id)
      .map((n) => (n.id === parentId ? { ...n, expanded: true } : n)),
    { ...node, parentId },
  ];
}
export function removeSmartNode(nodes: any[], id: string): any[] {
  const node = nodes.find((n) => n.id === id);
  return nodes
    .filter((n) => n.id !== id)
    .map((n) =>
      n.parentId === id ? { ...n, parentId: node?.parentId || null } : n,
    );
}
export function smartTreeRows(
  nodes: any[],
  query = "",
): { node: any; depth: number; path: string }[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const matching = new Set<string>();
  const text = query.trim().toLocaleLowerCase();
  if (text)
    for (const n of nodes) {
      if (!String(n.name).toLocaleLowerCase().includes(text)) continue;
      let cursor: any = n;
      const seen = new Set<string>();
      while (cursor && !seen.has(cursor.id)) {
        seen.add(cursor.id);
        matching.add(cursor.id);
        cursor = byId.get(cursor.parentId);
      }
    }
  const rows: { node: any; depth: number; path: string }[] = [];
  const visited = new Set<string>();
  function hide(parentId: string) {
    for (const n of nodes.filter((n) => n.parentId === parentId)) {
      if (visited.has(n.id)) continue;
      visited.add(n.id);
      hide(n.id);
    }
  }
  function append(parentId: string | null, depth: number, path: string) {
    for (const node of nodes.filter((n) => (n.parentId || null) === parentId)) {
      if (visited.has(node.id)) continue;
      visited.add(node.id);
      if (text && !matching.has(node.id)) continue;
      const nextPath = path ? `${path} / ${node.name}` : node.name;
      rows.push({ node, depth, path: nextPath });
      if (isSmartFolder(node)) {
        if (text || node.expanded !== false)
          append(node.id, depth + 1, nextPath);
        else hide(node.id);
      }
    }
  }
  append(null, 0, "");
  // Recover orphaned/cyclic nodes in the UI instead of silently losing definitions.
  for (const node of nodes)
    if (!visited.has(node.id) && (!text || matching.has(node.id)))
      rows.push({ node, depth: 0, path: node.name });
  return rows;
}
export function setSmartOverrides(
  album: any,
  ids: number[],
  action: "include" | "exclude" | "reset",
) {
  const include = new Set<number>(album.query?.includeFileIds || []);
  const exclude = new Set<number>(album.query?.excludeFileIds || []);
  for (const id of ids.filter((id) => Number.isSafeInteger(id) && id > 0)) {
    include.delete(id);
    exclude.delete(id);
    if (action === "include") include.add(id);
    if (action === "exclude") exclude.add(id);
  }
  return {
    ...album,
    query: {
      ...album.query,
      includeFileIds: [...include],
      excludeFileIds: [...exclude],
    },
  };
}
export const SMART_TEMPLATES = [
  "with_objects",
  "multiple_people",
  "with_text",
  "pending_text",
  "recent_picks",
  "person",
  "location",
  "pets",
  "landscape",
  "unreviewed",
  "anniversary",
] as const;
export function smartTemplateRules(
  template: string,
  makeRule: (field?: string) => SmartRule,
): SmartRule[] {
  const rule = (field: string, operator: string, value: any) => ({
    ...makeRule(field),
    operator,
    value,
  });
  switch (template) {
    case "with_objects": return [rule("object_state", "is", "detected")];
    case "multiple_people": return [rule("object_count", "gte", {classKey:"coco2017-80@1:0",count:2})];
    case "with_text": return [rule("ocr_state", "is", "text")];
    case "pending_text": return [rule("ocr_state", "is", "pending")];
    case "recent_picks":
      return [
        rule("date_taken", "in_last", { amount: 1, unit: "month" }),
        rule("rating", "gte", 3),
      ];
    case "person":
      return [makeRule("person")];
    case "location":
      return [makeRule("location")];
    case "pets":
      return [
        rule("semantic", "matches", {
          prompt: "a photo of a domestic pet dog or cat",
          threshold: 0.25,
        }),
      ];
    case "landscape":
      return [
        rule("semantic", "matches", {
          prompt:
            "a scenic landscape photo of mountains, lakes, forest or ocean",
          threshold: 0.25,
        }),
      ];
    case "unreviewed":
      return [rule("culling", "is", 0)];
    case "anniversary":
      return [rule("anniversary", "is", true)];
    default:
      return [makeRule()];
  }
}
