import test from "node:test";
import * as mediaTitles from '../src/common/mediaTitle.ts';
import * as mediaViews from '../src/common/mediaViews.ts';
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { stripTypeScriptTypes } from "node:module";
import { parse, compileScript } from "@vue/compiler-sfc";
import * as Vue from "vue";
import { renderToString } from "vue/server-renderer";
import * as helpers from "../src/common/smartAlbums.ts";
import * as objectHelpers from "../src/common/objectUi.ts";
import { SMART_TAG_CATEGORIES } from "../src/common/smartTags.ts";
const source = readFileSync(
  new URL("../src/components/SmartAlbumEdit.vue", import.meta.url),
  "utf8",
);
const dictionary = JSON.parse(
  readFileSync(new URL("../src/locales/en.json", import.meta.url), "utf8"),
);
const props: any = {
  smartAlbum: null,
  parentId: "travel",
  nodes: [{ id: "travel", kind: "folder", name: "Travel" }],
};
function context(overrides: any = {}) {
  const libConfig = Vue.reactive({ _libraryId: "lib" });
  return {
    ...helpers,
    ...mediaTitles, ...mediaViews,
    ...objectHelpers,
    useObjects: () => ({ view: Vue.ref({classes:[{key:"coco2017-80@1:0",name:"person",supported:true}]}) }),
    ...Vue,
    SMART_TAG_CATEGORIES,
    props,
    crypto,
    JSON,
    Number,
    Date,
    Promise,
    libConfig,
    config: {
      search: { sortType: 0, sortOrder: 1, groupBy: 0 },
      settings: { language: "en", ai: { semanticProfile: "model-a" } },
    },
    GROUP: {
      NONE: 0,
      FOLDER: 1,
      DAY: 2,
      MONTH: 3,
      YEAR: 4,
      FILE_TYPE: 5,
      CULLING: 6,
      RATING: 7,
      CAMERA: 8,
      LENS: 9,
      LOCATION: 10,
    },
    useI18n: () => ({
      t: (key: string) => key,
      locale: Vue.ref("en"),
      messages: Vue.ref({ en: dictionary }),
    }),
    useUIStore: () => ({ pushInputHandler() {}, removeInputHandler() {} }),
    getSmartQueryCountAndSum: async () => [2, 100],
    getSmartQueryFiles: async () => [
      { id: 1, name: "one" },
      { id: 2, name: "two" },
    ],
    getSmartAlbumStatus: async () => ({
      profile: "model-a",
      indexed: 2,
      total: 3,
      pending: 1,
      job: {},
    }),
    getThumbUrl: (id: number) => `thumb-${id}`,
    isWin: false,
    getFileThumbById: async () => "",
    getThumbnailDataUrl: () => "",
    setTimeout: () => 1,
    clearTimeout() {},
    window: {
      addEventListener() {},
      removeEventListener() {},
      innerWidth: 1000,
      innerHeight: 1000,
    },
    ...overrides,
  };
}
function harness(overrides: any = {}) {
  const emits: any[] = [];
  const watches: any[] = [];
  const ctx = context({
    ...overrides,
    defineProps: () => overrides.props || { ...props },
    defineEmits:
      () =>
      (...args: any[]) =>
        emits.push(args),
    onMounted() {},
    onUnmounted() {},
    watch: (_: any, fn: any) => watches.push(fn),
  });
  const script = stripTypeScriptTypes(
    source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1],
  ).replace(/^import[\s\S]*?from\s*[\'"][^\'"]+[\'"];?\r?\n/gm, "");
  const api = runInNewContext(
    `${script}\n;({rules,name,parentId,template,applyTemplate,previewResults,preview,previewError,previewStale,canSave,clickOk,includeIds,excludeIds,recalibrate,makeRule,getOperatorOptions,isRuleValid,ocrPreviewRevision,viewPreviewRevision});`,
    ctx,
  );
  return { api, ctx, emits, watches };
}
test("editor templates create independent groups and retain manual overrides on save", () => {
  const h = harness();
  h.api.template.value = "recent_picks";
  h.api.applyTemplate();
  assert.equal(h.api.rules.value.length, 2);
  h.api.includeIds.value = [10];
  h.api.excludeIds.value = [20];
  h.api.clickOk();
  const album = h.emits[0][1];
  assert.equal(album.parentId, "travel");
  assert.equal(album.query.version, 2);
  assert.deepEqual(Array.from(album.query.includeFileIds), [10]);
  assert.equal(album.kind, "album");
});
test("AI previews pin the model, distinguish incomplete indexing and require recalibration after edits", async () => {
  const h = harness();
  h.api.template.value = "pets";
  h.api.applyTemplate();
  assert.equal(h.api.canSave.value, false);
  await h.api.previewResults();
  assert.equal(h.api.preview.value.count, 2);
  assert.equal(h.api.preview.value.status.pending, 1);
  assert.equal(h.api.rules.value[0].value.modelProfile, "model-a");
  assert.equal(h.api.canSave.value, true);
  h.api.rules.value[0].value.threshold = 0.7;
  assert.equal(h.api.previewStale.value, true);
  assert.equal(h.api.canSave.value, false);
  h.api.recalibrate();
  assert.equal(h.api.rules.value[0].value.modelProfile, undefined);
});
test("query errors are not successful zero-count previews", async () => {
  const h = harness({
    getSmartQueryCountAndSum: async () => {
      throw new Error("network unavailable");
    },
  });
  h.api.template.value = "pets";
  h.api.applyTemplate();
  await h.api.previewResults();
  assert.match(h.api.previewError.value, /network unavailable/);
  assert.equal(h.api.preview.value, null);
  assert.equal(h.api.canSave.value, false);
});
test("late preview results cannot adopt edited conditions or another library", async () => {
  for (const change of ["conditions", "library"]) {
    let finish: any;
    const h = harness({
      getSmartQueryCountAndSum: () =>
        new Promise((resolve) => (finish = resolve)),
    });
    h.api.template.value = "pets";
    h.api.applyTemplate();
    const pending = h.api.previewResults();
    if (change === "conditions") h.api.rules.value[0].value.prompt = "snow";
    else h.ctx.libConfig._libraryId = "other";
    finish([99, 100]);
    await pending;
    assert.equal(h.api.preview.value, null);
    assert.equal(h.api.rules.value[0].value.modelProfile, undefined);
  }
});
test("changing libraries prevents saving an old editor into the new library", () => {
  const h = harness();
  h.api.name.value = "Test";
  h.ctx.libConfig._libraryId = "other";
  h.api.clickOk();
  assert.equal(h.emits.length, 0);
});
function renderComponent(text: string, bindings: any, filename: string) {
  const { descriptor } = parse(text, { filename });
  let code = stripTypeScriptTypes(
    compileScript(descriptor, { id: filename, inlineTemplate: true }).content,
  );
  const imports: any = {};
  code = code
    .replace(/import\s*\{([^}]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
      for (const name of names.split(",")) {
        const [original, alias] = name.trim().split(/\s+as\s+/);
        imports[alias || original] = (Vue as any)[original];
      }
      return "";
    })
    .replace(/^import[\s\S]*?from\s*[\'"][^\'"]+[\'"];?\r?\n/gm, "")
    .replace("export default", "const component=");
  return runInNewContext(`${code}\n;component`, {
    ...context(),
    ...bindings,
    ...imports,
  });
}
async function ssr(component: any, props: any) {
  const app = Vue.createSSRApp(component, props);
  app.config.globalProperties.$t = (key: string) => key;
  return renderToString(app);
}
test("actual editor template renders defaults and recursive mixed condition groups", async () => {
  const groupText = readFileSync(
    new URL("../src/components/SmartAlbumRules.vue", import.meta.url),
    "utf8",
  );
  const group = renderComponent(groupText, {}, "SmartAlbumRules.vue");
  const stub = {
    render() {
      return Vue.h("span");
    },
  };
  const editor = renderComponent(
    source,
    {
      SmartAlbumRules: group,
      SmartAlbumStatus: stub,
      ModalDialog: { template: "<section><slot /></section>" },
      TButton: stub,
      IconAdd: stub,
      IconEdit: stub,
      IconRemove: stub,
    },
    "SmartAlbumEdit.vue",
  );
  let html = await ssr(editor, { ...props });
  assert.match(html, /smart_albums.template/);
  assert.match(html, /smart_albums.add_group/);
  assert.match(html, /smart_albums.preview/);
  const album = {
    name: "Mixed",
    query: {
      match: "all",
      rules: [
        {
          id: "g",
          field: "group",
          operator: "any",
          value: {
            rules: [
              { id: "a", field: "rating", operator: "gte", value: 3 },
              {
                id: "b",
                field: "semantic",
                operator: "matches",
                value: { prompt: "pets", threshold: 0.25 },
              },
            ],
          },
        },
      ],
    },
  };
  html = await ssr(editor, { ...props, smartAlbum: album });
  assert.match(html, /pets/);
  assert.match(html, /smart_albums.preview_required/);
  assert.match(html, /disabled/);
});

test("actual sidebar template keeps collapsed children hidden and shows explicit failed counts", async () => {
  const text = readFileSync(
    new URL("../src/components/SmartAlbumList.vue", import.meta.url),
    "utf8",
  );
  const ctx = context();
  ctx.libConfig.smartAlbum = { type: "custom", id: "album" };
  ctx.libConfig.smartAlbums = [
    { id: "folder", kind: "folder", name: "Travel", expanded: false },
    {
      id: "album",
      parentId: "folder",
      name: "Hidden leaf",
      query: {
        match: "all",
        rules: [{ id: "r", field: "rating", operator: "gte", value: 3 }],
      },
      count: 12,
    },
  ];
  ctx.config.settings.thumbnailSize = 128;
  const stub = {
    render() {
      return Vue.h("span");
    },
  };
  const component = renderComponent(
    text,
    {
      ...ctx,
      TButton: stub,
      ContextMenu: stub,
      SmartAlbumEdit: stub,
      ModalDialog: stub,
      MessageBox: stub,
      IconFolder: stub,
      IconFolderCog: stub,
      IconMore: stub,
      IconAdd: stub,
      IconEdit: stub,
      IconTrash: stub,
      IconOrder: stub,
    },
    "SmartAlbumList.vue",
  );
  let html = await ssr(component, {});
  assert.match(html, /Travel/);
  assert.doesNotMatch(html, /Hidden leaf/);
  ctx.libConfig.smartAlbums[0].expanded = true;
  html = await ssr(component, {});
  assert.match(html, /Hidden leaf/);
  assert.match(html, /12/);
});

function sidebarHarness(overrides: any = {}) {
  const text = readFileSync(
    new URL("../src/components/SmartAlbumList.vue", import.meta.url),
    "utf8",
  );
  const album = {
    id: "a",
    name: "Album",
    query: {
      match: "all",
      rules: [{ id: "r", field: "rating", operator: "gte", value: 3 }],
    },
    count: 12,
    coverFileId: 1,
  };
  const ctx = context({
    watch() {},
    onMounted() {},
    onBeforeUnmount() {},
    ...overrides,
  });
  ctx.libConfig.smartAlbums = [album];
  ctx.libConfig.smartAlbum = { type: "custom", id: "a" };
  const script = stripTypeScriptTypes(
    text.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1],
  ).replace(/^import[\s\S]*?from\s*[\'"][^\'"]+[\'"];?\r?\n/gm, "");
  const api = runInNewContext(
    `${script}\n;({refreshCounts,countErrors,saveAlbum,nodes});`,
    ctx,
  );
  return { api, ctx, album };
}
test("sidebar counts distinguish a failed query from a successful empty album", async () => {
  const h = sidebarHarness({
    getSmartQueryCountAndSum: async () => {
      throw new Error("bad model");
    },
  });
  await h.api.refreshCounts();
  assert.equal(h.ctx.libConfig.smartAlbums[0].count, null);
  assert.match(h.api.countErrors.value.a, /bad model/);
  const empty = sidebarHarness({
    getSmartQueryCountAndSum: async () => [0, 0],
  });
  await empty.api.refreshCounts();
  assert.equal(empty.ctx.libConfig.smartAlbums[0].count, 0);
  assert.equal(empty.api.countErrors.value.a, undefined);
});
test("renaming or moving from the editor preserves existing counts when query is unchanged", () => {
  const h = sidebarHarness();
  h.api.saveAlbum({
    ...h.album,
    name: "New name",
    parentId: null,
    count: null,
  });
  assert.equal(h.ctx.libConfig.smartAlbums[0].count, 12);
  assert.equal(h.ctx.libConfig.smartAlbums[0].coverFileId, 1);
});
test("late sidebar counts cannot write into another library", async () => {
  let finish: any;
  const h = sidebarHarness({
    getSmartQueryCountAndSum: () =>
      new Promise((resolve) => (finish = resolve)),
  });
  const pending = h.api.refreshCounts();
  h.ctx.libConfig._libraryId = "other";
  h.ctx.libConfig.smartAlbums = [{ ...h.album, count: 99 }];
  finish([2, 100]);
  await pending;
  assert.equal(h.ctx.libConfig.smartAlbums[0].count, 99);
});

test('OCR smart rules expose literal text and explicit status without requiring semantic indexing', () => {
  const h = harness();
  const state = h.api.makeRule('ocr_state'); assert.equal(state.value, 'text'); assert.equal(h.api.isRuleValid(state), true);
  state.value = 'pending'; assert.equal(h.api.isRuleValid(state), true); state.value = 'invalid'; assert.equal(h.api.isRuleValid(state), false);
  const text = h.api.makeRule('ocr_text'); assert.equal(h.api.isRuleValid(text), false); text.value = '中文 100% _'; assert.equal(h.api.isRuleValid(text), true);
  assert.deepEqual(JSON.parse(JSON.stringify(h.api.getOperatorOptions('ocr_text').map((option:any) => option.value))), ['contains','not_contains']);
  assert.deepEqual(JSON.parse(JSON.stringify(h.api.getOperatorOptions('ocr_state').map((option:any) => option.value))), ['is','is_not']);
});
test('OCR previews become stale after a global model change or text-index update', async () => {
  const config = Vue.reactive({search:{sortType:0,sortOrder:1,groupBy:0},settings:{language:'en',ai:{semanticProfile:'semantic',ocrProfile:'ocr-a'}}});
  const h = harness({config}); h.api.rules.value = [h.api.makeRule('ocr_state')]; await h.api.previewResults(); assert.equal(Boolean(h.api.previewStale.value), false);
  config.settings.ai.ocrProfile = 'ocr-b'; assert.equal(Boolean(h.api.previewStale.value), true); await h.api.previewResults(); assert.equal(Boolean(h.api.previewStale.value), false);
  h.api.ocrPreviewRevision.value++; assert.equal(Boolean(h.api.previewStale.value), true);
});
test('actual OCR smart rule controls render states, literal input and coverage guidance', async () => {
  const group = renderComponent(readFileSync(new URL('../src/components/SmartAlbumRules.vue',import.meta.url),'utf8'),{},'SmartAlbumRules.vue');
  const stub = {render(){return Vue.h('span');}};
  const editor = renderComponent(source,{SmartAlbumRules:group,SmartAlbumStatus:stub,ModalDialog:{template:'<section><slot /></section>'},TButton:stub,IconAdd:stub,IconEdit:stub,IconRemove:stub},'SmartAlbumEdit.vue');
  const html = await ssr(editor,{...props,smartAlbum:{id:'ocr',name:'Text',query:{match:'all',rules:[{id:'s',field:'ocr_state',operator:'is',value:'pending'},{id:'t',field:'ocr_text',operator:'contains',value:'中文 <script>'}]}}});
  assert.match(html,/value="pending"/);assert.match(html,/value="failed"/);assert.match(html,/maxlength="512"/);assert.match(html,/ocr.smart_hint/);assert.doesNotMatch(html,/<script>/);
});

test('object smart rules validate classes and counts without starting inference', () => {
  const h=harness();
  const count=h.api.makeRule('object_count');
  assert.equal(count.value.classKey,'coco2017-80@1:0');assert.equal(count.value.count,1);assert.equal(h.api.isRuleValid(count),true);
  for (const invalid of [-1,1.5,10001,NaN]) {count.value.count=invalid;assert.equal(h.api.isRuleValid(count),false);}
  count.value.count=0;assert.equal(h.api.isRuleValid(count),true);
  const state=h.api.makeRule('object_state');assert.equal(state.value,'detected');state.value='partial';assert.equal(h.api.isRuleValid(state),true);state.value='all';assert.equal(h.api.isRuleValid(state),false);
  const category=h.api.makeRule('object_class');assert.equal(h.api.isRuleValid(category),true);category.value='';assert.equal(h.api.isRuleValid(category),false);
});
test('object previews are invalidated by the global detector and saved-result revisions',async()=>{
  const config=Vue.reactive({search:{sortType:0,sortOrder:1,groupBy:0},settings:{language:'en',ai:{semanticProfile:'semantic',detectionProfile:'objects-a'}}});
  const h=harness({config});h.api.rules.value=[h.api.makeRule('object_count')];await h.api.previewResults();assert.equal(Boolean(h.api.previewStale.value),false);
  config.settings.ai.detectionProfile='objects-b';assert.equal(Boolean(h.api.previewStale.value),true);await h.api.previewResults();assert.equal(Boolean(h.api.previewStale.value),false);
  h.api.ocrPreviewRevision.value++;assert.equal(Boolean(h.api.previewStale.value),true);
});

test('combined filters apply without an album name and exclude semantic inference conditions', () => {
  const h = harness({ props: { ...props, mode: 'filter' } });
  h.api.rules.value = [h.api.makeRule('rating')];
  h.api.rules.value[0].value = 3;
  assert.equal(h.api.name.value, '');
  assert.equal(h.api.canSave.value, true);
  h.api.clickOk();
  assert.equal(h.emits.length, 1);
  const result = h.emits[0][1];
  assert.equal(result.query.rules[0].field, 'rating');
  assert.equal(result.query.rules[0].value, 3);
  assert.equal(result.id, undefined);
  assert.equal(result.name, undefined);
  h.api.rules.value = [{ id: 'nested', field: 'group', operator: 'any', value: { rules: [h.api.makeRule('semantic')] } }];
  assert.equal(h.api.canSave.value, false);
  h.api.clickOk();
  assert.equal(h.emits.length, 1);
});

test('structured preview sends a query object and does not request semantic status', async () => {
  const calls: any[] = [];
  const h = harness({
    props: { ...props, mode: 'filter' },
    getSmartQueryCountAndSum: async (params: any) => { calls.push(params); return [0, 0]; },
    getSmartQueryFiles: async (params: any) => { calls.push(params); return []; },
    getSmartAlbumStatus: async () => { throw new Error('must not request semantic status'); },
  });
  h.api.rules.value = [{ id: 'r', field: 'rating', operator: 'gte', value: 3 }];
  await h.api.previewResults();
  assert.equal(h.api.previewError.value, '');
  assert.equal(h.api.preview.value.count, 0);
  assert.equal(calls.length, 2);
  for (const params of calls) {
    assert.equal(Array.isArray(params), false);
    assert.equal(params.version, 2);
    assert.equal(params.match, 'all');
    assert.equal(params.rules[0].value, 3);
  }
});

test('actual combined-filter editor hides album setup and semantic fields while keeping groups and preview', async () => {
  const group = renderComponent(readFileSync(new URL('../src/components/SmartAlbumRules.vue', import.meta.url), 'utf8'), {}, 'SmartAlbumRules.vue');
  const stub = { render() { return Vue.h('span'); } };
  const editor = renderComponent(source, { SmartAlbumRules: group, SmartAlbumStatus: stub, ModalDialog: { template: '<section><slot /></section>' }, TButton: stub, IconEdit: stub }, 'SmartAlbumEdit.vue');
  const html = await ssr(editor, { ...props, mode: 'filter' });
  assert.match(html, /structured_search.scope/);
  assert.match(html, /structured_search.apply/);
  assert.match(html, /smart_albums.add_group/);
  assert.match(html, /smart_albums.preview/);
  assert.doesNotMatch(html, /smart_albums.template|album.smart_edit.validation_name|value="semantic"|value="similar_image"/);
  assert.match(html, /value="ocr_text"/);
  assert.match(html, /value="object_class"/);
});


test('view-count smart rules accept zero, reject invalid counts and invalidate their previews independently', async () => {
  const h = harness(); h.api.name.value = 'Unseen';
  h.api.rules.value = [{ id: 'views', field: 'view_count', operator: 'is', value: 0 }];
  assert.equal(h.api.isRuleValid(h.api.rules.value[0]), true);
  assert.ok(h.api.getOperatorOptions('view_count').some((option: any) => option.value === 'gte'));
  for (const value of [-1, 1.5, NaN]) assert.equal(h.api.isRuleValid({ field: 'view_count', operator: 'is', value }), false);
  assert.equal(h.api.isRuleValid({ field: 'view_count', operator: 'between', value: { min: 4, max: 2 } }), false);
  await h.api.previewResults(); assert.ok(h.api.preview.value); assert.equal(h.api.previewStale.value, false);
  h.api.ocrPreviewRevision.value++; assert.equal(h.api.previewStale.value, false);
  h.api.viewPreviewRevision.value++; assert.equal(h.api.previewStale.value, true);
  assert.equal(helpers.hasViewRules([{ id: 'g', field: 'group', operator: 'all', value: { rules: [{ id: 'v', field: 'date_viewed', operator: 'in_last', value: { amount: 7, unit: 'day' } }] } }]), true);
  assert.equal(helpers.hasTimeSmartRules([{ id: 'v', field: 'date_viewed', operator: 'in_last', value: { amount: 7, unit: 'day' } }]), true);
});

test('view-driven sidebar counts refresh only albums depending on viewing statistics, not unrelated AI queries', async () => {
  const queries: any[] = [];
  const h = sidebarHarness({ getSmartQueryCountAndSum: async (params: any) => { queries.push(params); return [1, 100]; } });
  const unseen = { ...h.album, id: 'unseen', query: { match: 'all', rules: [{ id: 'v', field: 'view_count', operator: 'is', value: 0 }] } };
  const ai = { ...h.album, id: 'ai', query: { match: 'all', rules: [{ id: 's', field: 'semantic', operator: 'matches', value: { prompt: 'travel', threshold: .2 } }] } };
  h.ctx.libConfig.smartAlbums = [h.album, unseen, ai];
  await h.api.refreshCounts('views');
  assert.equal(queries.length, 1); assert.equal(queries[0].rules[0].field, 'view_count');
  assert.equal(h.ctx.libConfig.smartAlbums[0].count, 12); assert.equal(h.ctx.libConfig.smartAlbums[1].count, 1);
});
