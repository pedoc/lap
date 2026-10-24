<template>
  <section v-if="[1, 2, 3].includes(Number(file.file_type))" class="border-t border-base-content/5 px-1 py-4 space-y-3 text-xs" data-photo-media-ai @keydown.stop>
    <div class="flex gap-1 items-center"><button class="font-bold flex-1 text-left text-base-content/50" :aria-expanded="expanded" @click="expanded = !expanded">{{ expanded ? '▾' : '▸' }} {{ t('media_ai.title') }}</button><button class="btn btn-xs btn-ghost" @click="openMediaAiSettings">{{ t('media_ai.configure') }}</button></div>
    <div v-if="expanded" class="space-y-3">
      <p v-if="error" role="alert" class="text-error break-words whitespace-pre-wrap">{{ error }}</p>
      <div class="flex gap-1 flex-wrap"><button class="btn btn-xs" :disabled="busy || running" @click="analyze">{{ t(view?.analyses?.length ? 'media_ai.reanalyze' : 'media_ai.understand') }}</button><button class="btn btn-xs" :disabled="busy || running || !view?.analyses?.length" @click="index">{{ t('media_ai.index') }}</button><button class="btn btn-xs btn-ghost" @click="openMediaAiBatch(libConfig._libraryId, [Number(file.id)])">{{ t('media_ai.jobs') }}</button></div>
      <p v-if="running" role="status" class="opacity-60">{{ t('media_ai.state_running') }}</p>
      <p v-if="!view?.analyses?.length" class="opacity-60">{{ t('media_ai.no_analysis') }}</p>
      <template v-else-if="analysis">
        <select v-if="view.analyses.length > 1" v-model="segment" class="select select-bordered select-xs w-full" :aria-label="t('media_ai.matched_segment')"><option v-for="(a, i) in view.analyses" :key="a.id" :value="i">{{ mediaTime(a.start) }} – {{ mediaTime(a.end) }} · {{ a.title }}</option></select>
        <p class="text-[10px] opacity-50 break-all">{{ analysis.model.model.name }} · {{ new Date(analysis.createdAt).toLocaleString() }} · {{ t(analysis.indexed ? 'media_ai.indexed' : 'media_ai.not_indexed') }}</p>
        <div v-if="!analysis.dismissed.includes('title')" class="space-y-1"><p class="font-semibold break-words">{{ analysis.title }}</p><div class="flex gap-1"><button class="btn btn-xs" :disabled="busy" @click="adopt('title')">{{ t('media_ai.adopt_title') }}</button><button class="btn btn-xs btn-ghost" :disabled="busy" @click="dismiss('title')">{{ t('media_ai.dismiss') }}</button></div></div>
        <div v-if="!analysis.dismissed.includes('description')" class="space-y-1"><p class="whitespace-pre-wrap break-words leading-relaxed">{{ analysis.description }}</p><div class="flex gap-1"><button class="btn btn-xs" :disabled="busy" @click="adopt('description')">{{ t('media_ai.adopt_description') }}</button><button class="btn btn-xs btn-ghost" :disabled="busy" @click="dismiss('description')">{{ t('media_ai.dismiss') }}</button></div></div>
        <div v-if="!analysis.dismissed.includes('tags') && analysis.tags.length" class="space-y-2"><div class="flex flex-wrap gap-1"><span v-for="tag in analysis.tags" :key="tag" class="badge badge-sm">{{ tag }}</span></div><label class="flex gap-2"><input v-model="createTags" type="checkbox" class="checkbox checkbox-xs" :disabled="busy" />{{ t('media_ai.create_tags') }}</label><div class="flex gap-1"><button class="btn btn-xs" :disabled="busy" @click="adopt('tags')">{{ t('media_ai.adopt_tags') }}</button><button class="btn btn-xs btn-ghost" :disabled="busy" @click="dismiss('tags')">{{ t('media_ai.dismiss') }}</button></div></div>
        <p class="text-[10px] opacity-50">{{ t('media_ai.suggestions_hint') }}</p>
      </template>
      <p v-if="Number(file.file_type) === 2" class="text-[10px] opacity-50">{{ t('media_ai.sampling') }} <button v-if="analysis" class="link" @click="emit('seek', analysis.start)">{{ mediaTime(analysis.start) }}</button></p>
      <div class="border-t border-base-content/10 pt-3 space-y-2">
        <label class="block font-semibold" :for="questionId">{{ t('media_ai.question') }}</label>
        <textarea :id="questionId" v-model="question" class="textarea textarea-bordered w-full text-xs" maxlength="2000" rows="2" :placeholder="t('media_ai.question_placeholder')" :disabled="busy"></textarea>
        <label class="flex gap-2"><input v-model="visual" type="checkbox" class="checkbox checkbox-xs" :disabled="busy" />{{ t('media_ai.visual') }}</label>
        <button class="btn btn-xs btn-primary" :disabled="busy || !question.trim()" @click="ask">{{ t('media_ai.ask') }}</button>
        <button v-if="requestId" class="btn btn-xs" @click="cancelRequest">{{ t('media_ai.cancel_request') }}</button>
        <article v-for="a in view?.answers || []" :key="a.id" class="space-y-1 border-b border-base-content/10 pb-2"><p class="font-semibold whitespace-pre-wrap break-words">{{ a.question }}</p><p class="whitespace-pre-wrap break-words leading-relaxed">{{ a.answer }}</p><div class="flex flex-wrap gap-1"><button v-for="e in a.evidence" :key="e.id" class="badge badge-ghost h-auto py-1 cursor-pointer" :title="e.description" @click="Number(file.file_type) === 2 && emit('seek', e.start)">[{{ e.id }}] {{ Number(file.file_type) === 2 ? mediaTime(e.start) + '–' + mediaTime(e.end) : t('media_ai.evidence') }}</button></div></article>
      </div>
    </div>
  </section>
</template>
<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { mediaTime, openMediaAiBatch, openMediaAiSettings } from '@/common/mediaAi';
import { pendingChanges } from '@/common/pendingChanges';
const props = defineProps<{ file: any }>();
const emit = defineEmits<{ seek: [seconds: number] }>();
const { t } = useI18n();
const expanded = ref(false), view = ref<any>(null), error = ref(''), busy = ref(false), running = ref(false), segment = ref(0), question = ref(''), visual = ref(false), createTags = ref(false);
const questionId = `media-ai-question-${crypto.randomUUID()}`;
const requestId = ref('');
const analysis = computed(() => view.value?.analyses[segment.value] || view.value?.analyses[0]);
let generation = 0, disposed = false, off: (() => void) | undefined, timer: ReturnType<typeof setInterval> | undefined;
async function refresh() {
  const epoch = generation, libraryId = libConfig._libraryId, fileId = Number(props.file.id);
  try { const next = await invoke('media_ai_inspect', { libraryId, fileId }); if (!disposed && epoch === generation) view.value = next; }
  catch (e) { if (!disposed && epoch === generation) error.value = String(e); }
}
async function action(fn: (libraryId: string, fileId: number) => Promise<void>) {
  if (busy.value) return; const epoch = generation; busy.value = true; error.value = '';
  try { await fn(libConfig._libraryId, Number(props.file.id)); if (epoch === generation) await refresh(); }
  catch (e) { if (!disposed && epoch === generation) error.value = String(e); }
  finally { if (epoch === generation) busy.value = false; }
}
async function start(indexOnly: boolean) {
  await action(async (libraryId, fileId) => { await invoke('media_ai_start_job', { libraryId, fileIds: [fileId], options: { indexOnly, index: indexOnly, force: !indexOnly && !!view.value?.analyses?.length } }); running.value = true; });
}
const analyze = () => start(false), index = () => start(true);
async function adopt(field: string) {
  if (!await pendingChanges.confirm('media-title')) return;
  const source = structuredClone(JSON.parse(JSON.stringify(view.value.source))), analysisId = analysis.value.id, epoch = generation;
  await action(async (libraryId) => { await invoke('media_ai_adopt', { libraryId, source, analysisId, options: { [field]: true, createTags: createTags.value } }); if (epoch === generation && libraryId === libConfig._libraryId) { const next: any = await invoke('media_ai_inspect', { libraryId, fileId: source.fileId }); if (epoch === generation) { props.file.title = next.source.title; props.file.comments = next.source.comments; } } });
}
async function dismiss(field: string) { const stamp = analysis.value.stamp; await action(async (libraryId, fileId) => { await invoke('media_ai_dismiss', { libraryId, fileId, stamp, field }); }); }
async function cancelRequest() { const id = requestId.value; if (id) await invoke('media_ai_cancel_request', { requestId: id }); }
async function ask() { const epoch = generation, text = question.value, id = crypto.randomUUID(); requestId.value = id; try { await action(async (libraryId, fileId) => { await invoke('media_ai_ask', { libraryId, fileId, question: text, visual: visual.value, start: analysis.value?.start ?? null, requestId: id }); if (epoch === generation) question.value = ''; }); } finally { if (requestId.value === id) requestId.value = ''; } }
watch(() => [props.file.id, libConfig._libraryId], () => { void cancelRequest(); requestId.value = ''; generation++; busy.value = false; view.value = null; segment.value = 0; error.value = ''; question.value = ''; visual.value = false; running.value = false; if (expanded.value) void refresh(); });
watch(expanded, value => { if (value) void refresh(); });
onMounted(async () => {
  const stop = await listen<any>('media-ai-progress', e => { if (e.payload.libraryId === libConfig._libraryId && expanded.value) void refresh(); }); if (disposed) stop(); else off = stop;
  timer = setInterval(async () => { if (!running.value || disposed) return; try { const jobs: any[] = await invoke('media_ai_jobs', { libraryId: libConfig._libraryId }); running.value = jobs.some(j => j.state === 'running'); await refresh(); } catch { running.value = false; } }, 1500);
});
onUnmounted(() => { void cancelRequest(); disposed = true; generation++; clearInterval(timer); off?.(); });
</script>
