import test from "node:test";
import assert from "node:assert/strict";
import {
  isSmartFolder,
  smartTreeRows,
  canMoveSmartNode,
  moveSmartNode,
  removeSmartNode,
  smartResultSignature,
  setSmartOverrides,
  hasSemanticRules,
  smartRuleCount,
  smartTemplateRules,
  SMART_TEMPLATES,
} from "../src/common/smartAlbums.ts";
const rule = (field = "rating") => ({
  id: crypto.randomUUID(),
  field,
  operator: "gte",
  value: 3,
});
const album = (id: string, parentId: string | null = null) => ({
  id,
  parentId,
  name: id,
  query: { match: "all", rules: [rule()] },
  sort: { type: 0, order: 1 },
  group: { type: 0 },
});
const tree = () => [
  { id: "travel", kind: "folder", name: "旅行", expanded: true },
  {
    id: "china",
    kind: "folder",
    parentId: "travel",
    name: "国内",
    expanded: true,
  },
  album("精选", "china"),
  album("待整理"),
];
test("virtual folders produce paths without touching query definitions", () => {
  const nodes = tree();
  assert.deepEqual(
    smartTreeRows(nodes).map((r) => [r.node.id, r.depth]),
    [
      ["travel", 0],
      ["china", 1],
      ["精选", 2],
      ["待整理", 0],
    ],
  );
  assert.equal(smartTreeRows(nodes)[2].path, "旅行 / 国内 / 精选");
  assert.equal(isSmartFolder(nodes[2]), false);
});
test("collapse really hides descendants and does not recover them as orphans", () => {
  const nodes = tree();
  nodes[0].expanded = false;
  assert.deepEqual(
    smartTreeRows(nodes).map((r) => r.node.id),
    ["travel", "待整理"],
  );
});
test("search expands matching ancestors even when collapsed", () => {
  const nodes = tree();
  nodes[0].expanded = false;
  assert.deepEqual(
    smartTreeRows(nodes, "精选").map((r) => r.node.id),
    ["travel", "china", "精选"],
  );
});
test("moves preserve independent rules and prohibit cycles or album parents", () => {
  const nodes = tree();
  const signature = smartResultSignature(nodes[2]);
  assert.equal(canMoveSmartNode(nodes, "travel", "china"), false);
  assert.equal(canMoveSmartNode(nodes, "china", "china"), false);
  assert.equal(canMoveSmartNode(nodes, "travel", "精选"), false);
  assert.equal(canMoveSmartNode(nodes, "travel", "missing"), false);
  assert.equal(moveSmartNode(nodes, "travel", "china"), nodes);
  const moved = moveSmartNode(nodes, "精选", null);
  assert.equal(moved.at(-1).parentId, null);
  assert.equal(smartResultSignature(moved.at(-1)), signature);
});
test("moving a folder retains descendants and expands its destination", () => {
  const nodes = [
    ...tree(),
    { id: "work", kind: "folder", name: "工作", expanded: false },
  ];
  const moved = moveSmartNode(nodes, "china", "work");
  assert.equal(moved.find((n) => n.id === "work").expanded, true);
  assert.equal(moved.find((n) => n.id === "精选").parentId, "china");
});
test("deleting a folder promotes children and never removes albums", () => {
  const remaining = removeSmartNode(tree(), "china");
  assert.equal(remaining.length, 3);
  assert.equal(remaining.find((n) => n.id === "精选").parentId, "travel");
  assert.equal(removeSmartNode(tree(), "精选").length, 3);
});
test("orphaned and cyclic definitions remain visible without infinite recursion", () => {
  const nodes = [
    album("orphan", "missing"),
    { id: "a", kind: "folder", name: "a", parentId: "b" },
    { id: "b", kind: "folder", name: "b", parentId: "a" },
  ];
  assert.equal(smartTreeRows(nodes).length, 3);
  assert.equal(canMoveSmartNode(nodes, "orphan", "a"), false);
});
test("rename, reorder, move and count/cover updates do not invalidate results", () => {
  const a = album("a");
  const signature = smartResultSignature(a);
  assert.equal(
    smartResultSignature({
      ...a,
      name: "new",
      parentId: "travel",
      count: 12,
      coverFileId: 32,
      updatedAt: 100,
    }),
    signature,
  );
  assert.notEqual(
    smartResultSignature({ ...a, sort: { type: 7, order: 1 } }),
    signature,
  );
  assert.equal(smartResultSignature({ id: "f", kind: "folder" }), "");
});
test("manual overrides are exclusive, idempotent, album-scoped and reversible", () => {
  const a = album("a");
  const included = setSmartOverrides(a, [1, 2, 2, -3, NaN], "include");
  assert.deepEqual(included.query.includeFileIds, [1, 2]);
  assert.equal(a.query.includeFileIds, undefined);
  const excluded = setSmartOverrides(included, [2], "exclude");
  assert.deepEqual(excluded.query.includeFileIds, [1]);
  assert.deepEqual(excluded.query.excludeFileIds, [2]);
  assert.deepEqual(
    setSmartOverrides(excluded, [2], "reset").query.excludeFileIds,
    [],
  );
  assert.deepEqual(setSmartOverrides(excluded, [1], "include"), excluded);
});
test("nested groups detect semantic capabilities and count all conditions", () => {
  const rules = [
    {
      ...rule("group"),
      operator: "all",
      value: {
        rules: [
          rule(),
          {
            ...rule("group"),
            operator: "any",
            value: { rules: [rule("semantic"), rule("similar_image")] },
          },
        ],
      },
    },
  ];
  assert.equal(smartRuleCount(rules), 5);
  assert.equal(hasSemanticRules(rules), true);
  assert.equal(hasSemanticRules([rule()]), false);
});
for (const template of SMART_TEMPLATES)
  test(`template ${template} produces editable independent rules`, () => {
    const a = smartTemplateRules(template, rule);
    const b = smartTemplateRules(template, rule);
    assert.ok(a.length > 0);
    assert.notEqual(a[0].id, b[0].id);
  });
