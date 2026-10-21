import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import * as home from '../src/common/homeDashboard.ts';
import { objectClassName } from '../src/common/objectUi.ts';
import * as mediaTitles from '../src/common/mediaTitle.ts';
import { SIDEBAR } from '../src/common/constants.ts';

const read = (path: string) => readFileSync(new URL(`../src/${path}`, import.meta.url), 'utf8');
const source = read('components/HomeDashboard.vue');
const dictionary = JSON.parse(read('locales/zh-CN.json'));
const t = (key: string, values: any = {}) => String(key.split('.').reduce((value, part) => value?.[part], dictionary) || key).replace(/\{(\w+)\}/g, (_m, key) => values[key] ?? key);
const tick = async () => { await Vue.nextTick(); await new Promise(resolve => setImmediate(resolve)); };
function overview(libraryId = 'lib'): home.HomeOverview {
  return {
    libraryId, day: '2026-10-20', photos: 40, videos: 8, places: 2, firstYear: 2017, lastYear: 2026, featuredCount: 2,
    people: 3, objectClasses: 7, favorites: 8, tags: 2,
    objectLabels: [{ key: 'coco2017-80@1:16', name: 'Dog' }],
    featured: [{ id: 1, name: '湖畔.jpg', rating: 5, takenDate: 1_700_000_000, modifiedAt: 12, location: { hierarchy: [{ name: '浙江' }, { name: '杭州' }] } }, { id: 2, name: '山.jpg', rating: 4, takenDate: null, modifiedAt: null }],
    years: [{ year: 2017, count: 10, coverFileIds: [1], months: [{ month: 2, count: 10, coverFileIds: [1] }] }, { year: 2026, count: 40, coverFileIds: [2], months: [{ month: 10, count: 39, coverFileIds: [2] }, { month: 12, count: 1, coverFileIds: [] }] }],
    anniversaryCount: 1, anniversary: [{ id: 3, name: '旧照片.jpg', rating: 0, takenDate: 1_600_000_000, modifiedAt: null }],
    mostViewed: [],
    recent: [{ id: 4, name: '新照片.jpg', rating: 4, takenDate: null, modifiedAt: 12 }],
    topPlaces: [{ kind: 'location', value: '7', name: '杭州', count: 20, coverFileId: 1 }],
    topPeople: [{ kind: 'person', value: '2', name: '小明', count: 7, coverFileId: 2 }],
    topObjects: [{ kind: 'object', value: 'coco2017-80@1:16', name: 'Dog', count: 3, coverFileId: 1 }],
    topTags: [{ kind: 'tag', value: '3', name: '旅行', count: 10, coverFileId: 1 }],
  };
}

test('preferences normalize old or invalid values and year marks start at zero', () => {
  assert.deepEqual(home.homePreferences(null), { minimumRating: 4, intervalSeconds: 8, autoplay: true });
  assert.deepEqual(home.homePreferences({ minimumRating: 99, intervalSeconds: 0, autoplay: false }), { minimumRating: 4, intervalSeconds: 8, autoplay: false });
  assert.equal(home.homeYearHeight(10, 40), 25);
  assert.equal(home.homeYearHeight(0, 0), 0);
  assert.equal(home.homeYearSpan({ firstYear: 2026, lastYear: 2026 }), 1);
  assert.equal(home.homeYearSpan({ firstYear: null, lastYear: null }), null);
});

test('home reads never replace the newer query or cross a switched library', async () => {
  let library = 'lib'; const pending: Array<(value: home.HomeOverview) => void> = [];
  const loader = home.createHomeLoader(() => new Promise(resolve => pending.push(resolve)), () => library);
  const a = loader.load(4), b = loader.load(5);
  const latest = overview(); latest.photos = 5;
  pending[1](latest); await b; pending[0](overview()); await a;
  assert.equal(loader.state.data?.photos, 5);
  const c = loader.load(4); library = 'other'; pending[2](overview()); await c;
  assert.equal(loader.state.data, null);
  assert.equal(loader.state.error, '');
  loader.dispose();
});

test('errors are not empty libraries, wrong owner payloads are rejected, and disposed loads cannot revive data', async () => {
  const bad = home.createHomeLoader(async () => { throw new Error('database unavailable'); }, () => 'lib');
  await bad.load(4); assert.match(bad.state.error, /database unavailable/); assert.equal(bad.state.data, null);
  const wrong = home.createHomeLoader(async () => overview('other'), () => 'lib');
  await wrong.load(4); assert.match(wrong.state.error, /Library changed/);
  let resolve: any;
  const gone = home.createHomeLoader(() => new Promise(r => resolve = r), () => 'lib');
  const run = gone.load(4); gone.dispose(); resolve(overview()); await run;
  assert.equal(gone.state.data, null); assert.equal(gone.state.busy, false);
});

test('ordinary background refresh keeps the current photos while visibility invalidation clears them immediately', async () => {
  let finish: any; let round = 0;
  const loader = home.createHomeLoader(async () => ++round === 1 ? overview() : await new Promise(resolve => finish = resolve), () => 'lib');
  await loader.load(4);
  const previous = loader.state.data;
  const pending = loader.load(4, true);
  assert.equal(loader.state.data, previous);
  assert.equal(loader.state.busy, true);
  loader.clear();
  assert.equal(loader.state.data, null);
  finish(overview()); await pending;
  assert.equal(loader.state.data, null);
});

test('a bounded timeline keeps every year reachable and independent preview IDs preserve the selected photo', () => {
  const years = Array.from({ length: 25 }, (_, i) => ({ year: 2000 + i, count: i + 1, coverFileIds: [i + 1] }));
  let page = home.homeYearWindow(years, 25);
  assert.equal(page.years[0].year, 2015); assert.equal(page.hasPrevious, true); assert.equal(page.hasNext, false);
  page = home.homeYearWindow(years, 15);
  assert.equal(page.years[0].year, 2005); assert.equal(page.hasNext, true);
  assert.deepEqual(home.homePhotoIds(1, [1, 1, -1, 2, NaN]), [1, 2]);
  assert.deepEqual(home.homePhotoIds(99, [1]), []);
  const ids = home.homePhotoIds(20, Array.from({ length: 30 }, (_, i) => i + 1));
  assert.equal(ids.length, 12); assert.equal(ids[0], 20);
});

test('featured and anniversary links use the same structured conditions without semantic inference', () => {
  const rules = home.homeBrowseRules('featured', 4);
  assert.equal(rules.find(rule => rule.field === 'rating')?.value, 4);
  assert.equal(rules.find(rule => rule.field === 'file_type')?.value, 5);
  assert.equal(rules.find(rule => rule.field === 'culling')?.value, 2);
  const anniversary = home.homeBrowseRules('anniversary');
  assert.equal(anniversary.find(rule => rule.field === 'anniversary')?.value, true);
  assert.ok([...rules, ...anniversary].every(rule => !['semantic', 'similar_image'].includes(rule.field)));
});

test('month browsing uses local half-open boundaries including leap days and December rollover', () => {
  for (const [year, month, dayCount] of [[2024, 2, 29], [2025, 2, 28], [2026, 12, 31], [1956, 10, 31], [99, 1, 31]]) {
    const rule = home.homePeriodRules(year, month)[0];
    const start = new Date(rule.value.start * 1000), end = new Date(rule.value.end * 1000);
    assert.equal(start.getFullYear(), year); assert.equal(start.getMonth(), month - 1); assert.equal(start.getDate(), 1); assert.equal(start.getHours(), 0);
    const last = new Date((rule.value.end - 1) * 1000);
    assert.equal(last.getDate(), dayCount); assert.equal(last.getMonth(), month - 1);
    assert.equal(end.getMonth(), month % 12); assert.equal(end.getDate(), 1); assert.equal(end.getFullYear(), year + (month === 12 ? 1 : 0));
  }
  assert.deepEqual(home.homePeriodRules(2026, 13), []); assert.deepEqual(home.homePeriodRules(NaN), []);
  const year = home.homePeriodRules(2024)[0].value;
  assert.equal(new Date(year.start * 1000).getMonth(), 0); assert.equal(new Date(year.end * 1000).getFullYear(), 2025);
});

test('discovery links use validated IDs and supported structured operators', () => {
  const data = overview();
  assert.deepEqual(home.homeEntityRules(data.topObjects[0]).map(rule => [rule.field, rule.operator, rule.value]), [['object_class', 'has', 'coco2017-80@1:16']]);
  assert.equal(home.homeEntityRules({ ...data.topTags[0], value: '7' })[0].value, 7);
  assert.equal(home.homeEntityRules({ ...data.topTags[0], kind: 'collection' })[0].operator, 'is');
  assert.equal(home.homeEntityRules({ ...data.topTags[0], kind: 'location' })[0].operator, 'is');
  assert.deepEqual(home.homeEntityRules({ ...data.topTags[0], value: '0' }), []);
  assert.deepEqual(home.homeEntityRules({ ...data.topTags[0], value: '1 OR 1' }), []);
  assert.equal(home.homeMonths(data.years[1]).length, 12);
  assert.deepEqual(home.homeMonths(data.years[1])[11].coverFileIds, []);
  assert.equal(home.homeMonths(data.years[1])[0].count, 0);
});

test('entity search rejects superseded, switched-library and disposed suggestion responses', async () => {
  let library = 'lib'; const requests: Array<(result: home.HomeSearchResult) => void> = [];
  const loader = home.createHomeEntitySearch(() => new Promise(resolve => requests.push(resolve)), () => library);
  const a = loader.load('杭'), b = loader.load('杭州');
  requests[1]({ libraryId: 'lib', items: overview().topPlaces }); await b;
  requests[0]({ libraryId: 'lib', items: overview().topTags }); await a;
  assert.equal(loader.state.items[0].kind, 'location');
  const c = loader.load('小明'); library = 'new'; requests[2]({ libraryId: 'lib', items: overview().topPeople }); await c;
  assert.equal(loader.state.items.length, 0); assert.equal(loader.state.busy, false);
  const d = loader.load('标签'); loader.dispose(); requests[3]({ libraryId: 'new', items: overview().topTags }); await d;
  assert.equal(loader.state.items.length, 0);
});

function context(overrides: any = {}) {
  const timers = new Map<number, { fn: () => void; delay: number }>();
  const intervals = new Map<number, { fn: () => void; delay: number }>(); let timerId = 0;
  const listeners: Record<string, (event: any) => void> = {}, mounts: Array<() => any> = [], unmounts: Array<() => any> = [], emitted: any[] = [], calls: any[] = [];
  const libConfig = Vue.reactive({ _libraryId: 'lib' });
  const config = Vue.reactive({ home: { minimumRating: 4, intervalSeconds: 8, autoplay: true }, settings: { face: { enabled: true }, ai: {} } });
  const document = { hidden: false, addEventListener() {}, removeEventListener() {} };
  const match = { matches: false, addEventListener() {}, removeEventListener() {} };
  const ctx: any = {
    ...Vue, ...home, ...mediaTitles, objectClassName, Date, Intl, Set, Map, console, document, window: { matchMedia: () => match }, libConfig, config,
    useI18n: () => ({ t, locale: Vue.ref('zh-CN') }), useUIStore: () => ({ countUpdateTick: 0, setActivePane() {} }),
    props: { openPreview: async (...args: any[]) => calls.push(['preview', ...args]), libraryEmpty: false },
    defineProps: () => ctx.props, defineEmits: () => (...args: any[]) => emitted.push(args), defineExpose() {},
    invoke: async (name: string, args: any) => { calls.push([name, args]); return overview(args.libraryId); },
    listen: async (name: string, fn: any) => { listeners[name] = fn; return () => delete listeners[name]; },
    onMounted: (fn: any) => mounts.push(fn), onBeforeUnmount: (fn: any) => unmounts.push(fn),
    getPreviewUrl: (id: number) => `preview-${id}`, getThumbUrl: (id: number) => `thumb-${id}`,
    setTimeout: (fn: () => void, delay: number) => { const id = ++timerId; timers.set(id, { fn, delay }); return id; }, clearTimeout: (id: number) => timers.delete(id),
    setInterval: (fn: () => void, delay: number) => { const id = ++timerId; intervals.set(id, { fn, delay }); return id; }, clearInterval: (id: number) => intervals.delete(id), ...overrides,
  };
  // Run the actual title subscription with the existing deterministic lifecycle/IPC fixtures.
  const hook = stripTypeScriptTypes(read('common/mediaTitleEvents.ts'))
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '')
    .replace('export function', 'function');
  ctx.useMediaTitleEvents = runInNewContext(`${hook};useMediaTitleEvents`, {
    onMounted: ctx.onMounted, onBeforeUnmount: ctx.onBeforeUnmount, listen: ctx.listen,
  });
  const viewHook = stripTypeScriptTypes(read('common/mediaViewTracking.ts'))
    .slice(stripTypeScriptTypes(read('common/mediaViewTracking.ts')).indexOf('export function useMediaViewEvents('))
    .replace('export function', 'function');
  ctx.useMediaViewEvents = runInNewContext(`${viewHook};useMediaViewEvents`, {
    onMounted: ctx.onMounted, onBeforeUnmount: ctx.onBeforeUnmount, listen: ctx.listen,
  });
  return { ctx, timers, intervals, listeners, mounts, unmounts, emitted, calls, match };
}
function harness(overrides: any = {}) {
  const h = context(overrides);
  const script = stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const scope = Vue.effectScope();
  const api = scope.run(() => runInNewContext(`${script};({state,activeIndex,activePhoto,step,selectPhoto,hovered,focused,pageVisible,playing,togglePlayback,preferences,refresh,openPhoto,previewError,heroFailed,useThumbnail,imageFailed,queueRefresh,expandedYear,expandedYearData,months,toggleYear,pageYears,yearWindow,stats,browseEntity,coverStep,timelineCover,failedThumbs,opening});`, h.ctx));
  return { ...h, api, mount: async () => { for (const mount of h.mounts) await mount(); }, dispose: () => { h.unmounts.forEach(fn => fn()); scope.stop(); } };
}

test('actual carousel pauses for hover, focus and hidden pages; timers and listeners are released on disposal', async () => {
  const h = harness(); await tick(); await h.mount(); await tick();
  assert.ok([...h.timers.values()].some(timer => timer.delay === 8000));
  h.api.hovered.value = true; await tick(); assert.equal(h.timers.size, 0);
  h.api.hovered.value = false; await tick(); assert.equal(h.timers.size, 1);
  h.api.focused.value = true; await tick(); assert.equal(h.timers.size, 0);
  h.api.focused.value = false; h.api.pageVisible.value = false; await tick(); assert.equal(h.timers.size, 0);
  h.api.pageVisible.value = true; await tick(); h.api.step(-1); assert.equal(h.api.activeIndex.value, 1);
  h.api.step(1); assert.equal(h.api.activeIndex.value, 0);
  h.api.togglePlayback(); await tick(); assert.equal(h.api.playing.value, false); assert.equal(h.timers.size, 0);
  h.dispose(); assert.equal(Object.keys(h.listeners).length, 0); assert.equal(h.timers.size, 0);
});

test('home only reads its overview, honors reduced motion and invalidates visible media before refreshing', async () => {
  const h = harness(); await tick(); h.match.matches = true; await h.mount(); await tick();
  assert.equal(h.api.playing.value, false);
  assert.ok(h.calls.every(([name]) => name === 'get_home_overview'));
  h.listeners['media-lifecycle-changed']({ payload: { libraryId: 'other' } });
  assert.ok(h.api.state.data);
  h.listeners['media-lifecycle-changed']({ payload: { libraryId: 'lib' } });
  assert.equal(h.api.state.data, null);
  assert.ok([...h.timers.values()].some(timer => timer.delay === 600));
  h.dispose();
});

test('preview errors stay visible and failed hero previews fall back only once', async () => {
  const h = harness({ props: { openPreview: async () => { throw new Error('offline'); } } }); await tick();
  await h.api.openPhoto(h.api.activePhoto.value, h.api.state.data.featured);
  assert.match(h.api.previewError.value, /offline/);
  h.api.heroFailed(); assert.equal(h.api.useThumbnail.value, true); assert.equal(h.api.imageFailed.value, false);
  h.api.heroFailed(); assert.equal(h.api.imageFailed.value, true);
  h.dispose();
});

test('the actual timeline expands in place, exposes all twelve months and keeps video-only months reachable', async () => {
  const h = harness(); await tick();
  h.api.toggleYear(2026);
  assert.equal(h.api.expandedYearData.value.year, 2026);
  assert.equal(h.api.months.value.length, 12);
  assert.equal(h.api.months.value[0].count, 0);
  assert.equal(h.api.months.value[11].count, 1);
  assert.deepEqual(Array.from(h.api.months.value[11].coverFileIds), []);
  assert.equal(h.emitted.length, 0, 'expanding a year must stay on Home');
  h.api.toggleYear(2026); assert.equal(h.api.expandedYear.value, null);
  h.api.toggleYear(2017); assert.equal(h.api.months.value[1].count, 10);
  h.api.pageYears(-1); assert.equal(h.api.expandedYear.value, null);
  const kinds = Array.from(h.api.stats.value, (stat: any) => stat.kind);
  assert.deepEqual(kinds, ['photos', 'videos', 'people', 'objects', 'places', 'years']);
  h.api.browseEntity(h.api.state.data.topTags[0]);
  assert.equal(h.emitted[0][1].entity.value, '3');
  h.dispose();
});

test('background refresh preserves the older year page and expanded month panel when new years arrive', async () => {
  let data = overview();
  data.years = Array.from({ length: 25 }, (_, i) => ({ year: 2000 + i, count: 1, coverFileIds: [1], months: [{ month: 1, count: 1, coverFileIds: [1] }] }));
  const h = harness({ invoke: async () => data }); await tick();
  h.api.pageYears(-1); h.api.toggleYear(2010);
  assert.equal(h.api.yearWindow.value.years.at(-1).year, 2014);
  data = { ...data, years: [{ ...data.years[0], year: 1999 }, ...data.years, { ...data.years[0], year: 2025 }] };
  await h.api.refresh(true);
  assert.equal(h.api.yearWindow.value.years.at(-1).year, 2014);
  assert.equal(h.api.expandedYearData.value.year, 2010);
  h.dispose();
});

async function render(data: home.HomeOverview | null, busy = false, error = '') {
  const h = context({ createHomeLoader: () => ({ state: Vue.reactive({ data, busy, error }), load: async () => {}, clear() {}, dispose() {} }) });
  const { descriptor } = parse(source, { filename: 'HomeDashboard.vue' });
  const imports: any = {};
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: 'HomeDashboard.vue', inlineTemplate: true }).content)
    .replace(/import\s*\{([^}]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  for (const name of ['IconRefresh', 'IconPhoto', 'IconStar', 'IconStarFilled', 'IconPause', 'IconPlay', 'IconPrev', 'IconNext', 'IconRight', 'IconClose', 'IconPerson', 'IconLocation', 'IconHeart']) imports[name] = () => Vue.h('svg');
  imports.HomeSearch = () => Vue.h('input', { 'aria-label': t('home_dashboard.search') });
  const component = runInNewContext(`${code};component`, { ...h.ctx, ...imports });
  return renderToString(Vue.createSSRApp(component, { openPreview: async () => {} }));
}
test('timeline shares one thumbnail-only clock, skips failed covers and pauses for hidden or inactive playback', async () => {
  const h = harness(); await tick(); await h.mount();
  assert.equal(h.intervals.size, 1);
  const clock = [...h.intervals.values()][0]; assert.equal(clock.delay, 8000);
  assert.equal(h.api.timelineCover([10,20]), 10);
  clock.fn(); assert.equal(h.api.timelineCover([10,20]), 20);
  h.api.pageVisible.value = false; clock.fn(); assert.equal(h.api.coverStep.value, 1);
  h.api.pageVisible.value = true;
  h.api.togglePlayback(); clock.fn(); assert.equal(h.api.coverStep.value, 1);
  h.api.togglePlayback(); h.api.opening.value = true; clock.fn(); assert.equal(h.api.coverStep.value, 1);
  h.api.opening.value = false; clock.fn(); assert.equal(h.api.coverStep.value, 2);
  h.api.failedThumbs.add(10); assert.equal(h.api.timelineCover([10,20]), 20);
  h.api.failedThumbs.add(20); assert.equal(h.api.timelineCover([10,20]), null);
  assert.equal(h.api.timelineCover([]), null);
  assert.equal(h.api.timelineCover(undefined), null);
  h.dispose(); assert.equal(h.intervals.size, 0);
});

test('actual year and month images rotate their ranked IDs and use the next candidate after thumbnail errors', async () => {
  const h = harness(); await tick(); await h.mount();
  const clock = [...h.intervals.values()][0];
  for (const period of ['year', 'month']) {
    h.api.coverStep.value = 0; h.api.failedThumbs.clear();
    const template = source.match(new RegExp(`<template v-for="id in \\[timelineCover\\(${period}.coverFileIds\\)\\]"[\\s\\S]*?<\\/template>`))![0];
    const imports: any = {};
    const { code, errors } = compileTemplate({ source: template, filename: 'TimelineCover.vue', id: 'timeline-cover' });
    assert.deepEqual(errors, []);
    const js = code.replace(/import\s*\{([^}]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
      for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return '';
    }).replace('export function render', 'function render');
    const render = runInNewContext(`${js};render`, imports);
    const state = { [period]: { count: 2, coverFileIds: [10,20] }, timelineCover: h.api.timelineCover, failedThumbs: h.api.failedThumbs, getThumbUrl: (id: number) => `thumb-${id}` };
    const html = async () => {
      const app = Vue.createSSRApp({ data: () => state, render }); app.component('IconPhoto', () => Vue.h('svg')); return renderToString(app);
    };
    assert.match(await html(), /src="thumb-10"/); clock.fn(); assert.match(await html(), /src="thumb-20"/);
    h.api.failedThumbs.add(20); assert.match(await html(), /src="thumb-10"/);
    h.api.failedThumbs.add(10); assert.ok(!(await html()).includes('<img'));
  }
  h.dispose();
});

test('timeline clock respects reduced motion even when the featured carousel is empty', async () => {
  const data = overview(); data.featured = [];
  const h = harness({ invoke: async () => data }); await tick(); h.match.matches = true; await h.mount();
  const clock = [...h.intervals.values()][0]; clock.fn(); assert.equal(h.api.coverStep.value, 0);
  h.api.togglePlayback(); clock.fn(); assert.equal(h.api.coverStep.value, 1);
  await h.api.refresh(); assert.equal(h.api.coverStep.value, 0);
  h.dispose();
});

test('the real template renders real counts, scale-dependent years, photos and safe text', async () => {
  const data = overview(); data.featured[0].name = '<script>private</script>.jpg';
  const html = await render(data);
  assert.match(html, /&lt;script&gt;private&lt;\/script&gt;/); assert.doesNotMatch(html, /<script>/);
  assert.match(html, /height:25%/); assert.match(html, /height:100%/);
  assert.match(html, /40/); assert.match(html, /preview-1/); assert.match(html, /杭州/);
  assert.match(html, /回看这一天/); assert.match(html, /aria-label="搜索照片、标签、人物、地点…"/);
  for (const label of ['位人物', '类对象', '最近加入', '常拍地点', '照片里的人', '发现主题', '旅行']) assert.ok(html.includes(label), label);
  assert.doesNotMatch(html, /按拍摄时间浏览|沿着时间/);
  assert.doesNotMatch(html.match(/class="home-section-hint">([^<]+)/)![1], /\d{4}/, 'on-this-day spans multiple previous years');
});
test('the real template distinguishes loading, no ratings, no dates and query errors', async () => {
  const loading = await render(null, true); assert.match(loading, /正在整理你的影像/);
  const empty = overview(); empty.featured = []; empty.years = []; empty.anniversary = []; empty.anniversaryCount = 0;
  const html = await render(empty); assert.match(html, /还没有符合评分的照片/); assert.match(html, /有拍摄日期/);
  assert.doesNotMatch(html, /src="preview/);
  const failed = await render(null, false, 'database failure'); assert.match(failed, /role="alert"/); assert.match(failed, /database failure/); assert.match(failed, /重试/);
});

test('featured captions retain full addresses with country and city, without repeating existing levels', async () => {
  const hierarchy = ['中国', '浙江省', '杭州市', '西湖区', '翠苑街道'].map(name => ({ name }));
  const address = '浙江省杭州市西湖区翠苑街道文一路123号';
  const location = { hierarchy, address };
  const expected = `中国 · ${address}`;
  assert.equal(home.homePhotoAddress(location), expected);
  assert.equal(home.homePhotoAddress({ hierarchy, address: `中国${address}` }), `中国${address}`);
  assert.equal(home.homePhotoAddress({ hierarchy, address: '文一路123号' }), '中国 · 浙江省 · 杭州市 · 西湖区 · 翠苑街道 · 文一路123号');
  assert.equal(home.homePhotoAddress({ hierarchy }), '中国 · 浙江省 · 杭州市 · 西湖区 · 翠苑街道');
  assert.equal(home.homePhotoAddress({ hierarchy: [{ name: ' ' }, { name: ' 杭州市 ' }, { name: '杭州市' }], address: ' ' }), '杭州市');
  assert.equal(home.homePhotoAddress(null), '');
  const data = overview(); data.featured[0].location = location;
  const html = await render(data);
  assert.ok(html.includes(`class="home-photo-location" title="${expected}">${expected}</span>`));
});

test('discovery sections collapse when the library has no corresponding metadata', async () => {
  const data = overview(); data.topPeople = []; data.topPlaces = []; data.topObjects = []; data.topTags = [];
  const html = await render(data);
  assert.match(html, /最近加入/); assert.doesNotMatch(html, /照片里的人|常拍地点|发现主题/);
});

test('home has a stable new sidebar ID and native previews retain the bounded independent source', async () => {
  assert.equal(SIDEBAR.LIBRARY, 0); assert.equal(SIDEBAR.OBJECTS, 10); assert.equal(SIDEBAR.HOME, 11);
  const content = read('components/Content.vue');
  const fn = content.match(/async function openHomePhoto\([\s\S]*?\n\}/)![0];
  const calls: any[] = [], library = { _libraryId: 'lib' };
  let resolve: any;
  const ctx: any = { libConfig: library, config: { main: { sidebarIndex: SIDEBAR.HOME } }, SIDEBAR, ...home, Map, t,
    invoke: (name: string, args: any) => { assert.equal(name, 'get_home_preview_files'); assert.equal(args.libraryId, 'lib'); return new Promise(r => resolve = r); }, openImageViewer: async (...args: any[]) => calls.push(args) };
  const open = runInNewContext(`(${stripTypeScriptTypes(fn)})`, ctx);
  let pending = open({ id: 2, ids: [1, 2], libraryId: 'lib' });
  resolve([{ id: 2 }, { id: 1 }]); await pending;
  assert.equal(calls[0][0], 1); assert.deepEqual(Array.from(calls[0][3].files).map((file: any) => file.id), [1, 2]);
  pending = open({ id: 2, ids: [1, 2], libraryId: 'lib' }); library._libraryId = 'other'; resolve([{ id: 2 }]); await pending;
  assert.equal(calls.length, 1);
});


test('title events update the real dashboard in place without reloading or crossing libraries', async () => {
  const h = harness(); await tick(); await h.mount(); await tick();
  const before = h.calls.length;
  h.listeners['media-title-changed']({ payload: { library_id: 'lib', file_id: 1, title: '自定义标题' } });
  assert.equal(mediaTitles.mediaDisplayName(h.api.activePhoto.value), '自定义标题');
  assert.equal(h.api.activePhoto.value.name, '湖畔.jpg');
  assert.equal(h.calls.length, before);
  h.listeners['media-title-changed']({ payload: { library_id: 'other', file_id: 1, title: 'Wrong library' } });
  assert.equal(h.api.activePhoto.value.title, '自定义标题');
  h.dispose(); assert.equal(h.listeners['media-title-changed'], undefined);
});


test('frequently viewed photos show local counters and browse only viewed photos', async () => {
  const data = overview(); data.mostViewed = [{ ...data.featured[0], viewCount: 7, lastViewedAt: 1000 }];
  const html = await render(data);
  assert.match(html, /经常查看/); assert.match(html, /7 次查看/);
  const rules = home.homeBrowseRules('most-viewed');
  assert.equal(rules.find(rule => rule.field === 'view_count')?.operator, 'gt');
  assert.equal(rules.find(rule => rule.field === 'view_count')?.value, 0);
  assert.equal(rules.find(rule => rule.field === 'file_type')?.value, 5);
  assert.ok(!rules.some(rule => rule.field === 'semantic'));
  assert.ok(!(await render(overview())).includes('经常查看'));
});

test('view events patch the home counters and debounce a preserved overview refresh without crossing libraries', async () => {
  const data = overview(); data.mostViewed = [{ ...data.featured[0], viewCount: 1, lastViewedAt: 1000 }];
  const h = harness({ invoke: async () => data }); await tick(); await h.mount();
  h.listeners['media-view-changed']({ payload: { library_id: 'other', file_id: 1, view_count: 9, last_viewed_at: 2000 } });
  assert.equal(h.api.state.data.mostViewed[0].viewCount, 1);
  h.listeners['media-view-changed']({ payload: { library_id: 'lib', file_id: 1, view_count: 2, last_viewed_at: 2000 } });
  assert.equal(h.api.state.data.mostViewed[0].viewCount, 2);
  assert.equal(h.api.state.data.featured[0].viewCount, 2);
  assert.ok([...h.timers.values()].some(timer => timer.delay === 600));
  assert.ok(h.api.state.data, 'statistics events must not blank the current dashboard'); h.dispose();
});

test('frequently viewed navigation opens the standard query in descending view-count order', () => {
  const page = read('views/Home.vue'); const start = page.indexOf('function browseHome('), end = page.indexOf('\n}', start) + 2;
  const config = { main: { sidebarIndex: SIDEBAR.HOME }, search: { fileType: 0, sortType: 0, sortOrder: 0, groupBy: 3 }, home: {} };
  let query: any;
  const run = runInNewContext(`(${stripTypeScriptTypes(page.slice(start,end))})`, { config, SIDEBAR, ...home,
    activateMainPanel() {}, showPanel: { value: false }, libConfig: { _libraryId: 'lib' },
    structuredSearch: { libraryId: 'lib' }, applyStructuredSearch: (value: any) => { query = value; return true; } });
  run({ kind: 'most-viewed' });
  assert.equal(config.search.sortType, 11); assert.equal(config.search.sortOrder, 1); assert.equal(config.search.groupBy, 0);
  assert.equal(config.main.sidebarIndex, SIDEBAR.SEARCH); assert.ok(query.query.rules.some((rule: any) => rule.field === 'view_count'));
});
