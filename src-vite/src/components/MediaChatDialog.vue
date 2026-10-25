<template>
  <Teleport to="body">
    <section v-show="mediaChat.open" ref="root" class="fixed z-[550] flex flex-col overflow-hidden rounded-box border border-base-content/20 bg-base-100 text-base-content shadow-2xl text-xs" :style="frameStyle" role="dialog" :aria-label="t('media_chat.title')" aria-modal="false" data-media-chat-dialog data-media-chat-drop @pointerdown="dismissActionsOutside" @keydown.stop="dialogKeydown" @dragenter.stop.prevent="dragOver = true" @dragover.stop.prevent="dragOver = true" @dragleave.stop="dragOver = false" @drop.stop.prevent="dropFiles" @lap-media-chat-files.stop="internalDrop">
      <header class="flex h-11 shrink-0 items-center gap-2 border-b border-base-content/10 px-3 cursor-move touch-none select-none" @pointerdown="beginFrameGesture($event, 'move')" @lostpointercapture="endFrameGesture">
        <strong class="min-w-0 flex-1 truncate">{{ t('media_chat.title') }}</strong>
        <span v-if="sending" class="loading loading-spinner loading-xs" :title="t('media_chat.generating')"></span>
        <button class="btn btn-xs btn-ghost" @pointerdown.stop @click="showHistory = !showHistory">{{ t('media_chat.history') }}</button>
        <button class="btn btn-xs btn-ghost" :aria-label="t('media_chat.close')" @pointerdown.stop @click="closeMediaChat">×</button>
      </header>
      <p v-if="error" role="alert" class="shrink-0 px-3 py-2 text-error whitespace-pre-wrap break-words">{{ error }}</p>
      <p v-if="notice" role="status" class="shrink-0 px-3 py-1 text-warning break-words">{{ notice }}</p>
      <div class="flex min-h-0 flex-1 relative" :class="dragOver ? 'ring-2 ring-inset ring-primary' : ''">
        <aside v-if="showHistory" class="w-44 shrink-0 border-r border-base-content/10 p-2 flex flex-col gap-2 overflow-y-auto" :class="frame.width < 700 ? 'absolute inset-y-0 left-0 z-20 bg-base-100 shadow-lg' : ''" data-chat-history>
          <button class="btn btn-sm btn-primary w-full" :disabled="loading" @click="newSession()">＋ {{ t('media_chat.new_session') }}</button>
          <div v-if="view" class="flex flex-wrap gap-1">
            <button class="btn btn-xs btn-ghost" @click="renameText = view.session.title; renaming = !renaming">{{ t('media_chat.rename') }}</button>
            <button class="btn btn-xs btn-ghost text-error" :disabled="!!view.session.runningRequestId" @click="deleteSession">{{ t('media_chat.delete') }}</button>
          </div>
          <form v-if="renaming" class="flex gap-1" @submit.prevent="renameSession"><input v-model="renameText" class="input input-bordered input-xs min-w-0 w-full" maxlength="120" :aria-label="t('media_chat.session_name')" @keydown.esc.stop="renaming = false" /><button class="btn btn-xs" type="submit">✓</button></form>
          <button v-for="session in sessions" :key="session.id" class="rounded-box p-2 text-left hover:bg-base-200 min-w-0" :class="session.id === view?.session.id ? 'bg-base-200' : ''" :aria-current="session.id === view?.session.id ? 'true' : undefined" :disabled="loading" @click="selectSession(session.id)"><span class="block truncate font-medium" :title="session.title">{{ session.title }}</span><span class="block truncate text-[10px] opacity-50">{{ session.lastMessage || t('media_chat.empty_session') }}</span><span v-if="session.runningRequestId" class="loading loading-dots loading-xs"></span></button>
        </aside>
        <main class="flex flex-col min-w-0 min-h-0 flex-1">
          <div v-if="incomingIds.length" class="shrink-0 px-3 py-2 bg-base-200/70 space-y-1" data-chat-current-media><p>{{ t('media_chat.current_media', { count: incomingIds.length }) }}</p><p class="truncate opacity-60" :title="incoming.map(file => file.name).join('、')">{{ incoming.map(file => file.name).join('、') }}</p><div class="flex gap-1 flex-wrap"><button class="btn btn-xs" :disabled="loading || incomingIds.length > 16" @click="addIncoming">{{ t('media_chat.add_to_turn') }}</button><button class="btn btn-xs btn-ghost" :disabled="loading || incomingIds.length > 16" @click="newSession(incoming)">{{ t('media_chat.new_with_media') }}</button><button class="btn btn-xs btn-ghost" @click="incoming = []; incomingIds = []">{{ t('media_chat.dismiss') }}</button></div></div>
          <div ref="messagesRoot" class="flex-1 min-h-0 overflow-y-auto px-4 py-3 space-y-4" data-chat-messages>
            <p v-if="loading" class="loading loading-spinner loading-sm"></p>
            <div v-else-if="!view?.messages.length" class="h-full min-h-32 flex flex-col items-center justify-center text-center gap-3 text-base-content/60"><strong class="text-base font-medium">{{ t('media_chat.empty_title') }}</strong><p>{{ t('media_chat.empty_hint') }}</p><p class="text-[11px]">{{ t('media_chat.drop_hint') }}</p></div>
            <article v-for="message in view?.messages || []" :key="message.id" class="rounded-box p-3 space-y-2 max-w-full" :class="message.role === 'user' ? 'bg-base-200 ml-6' : 'border border-base-content/10 mr-6'" :data-chat-message="message.id">
              <div class="flex justify-between gap-2 text-[10px] opacity-50"><span>{{ t(message.role === 'user' ? 'media_chat.you' : 'media_chat.assistant') }}<span v-if="message.role === 'assistant' && message.modelName"> · {{ message.modelName }}</span><span v-if="message.role === 'assistant' && message.reasoningEffort"> · {{ t('media_chat.reasoning') }}: {{ reasoningLabel(message.reasoningEffort) }}</span></span><time>{{ new Date(message.createdAt).toLocaleString() }}</time></div>
              <p class="whitespace-pre-wrap break-words leading-relaxed select-text">{{ message.content }}</p>
              <p v-if="message.status === 'pending'" class="opacity-60">{{ t('media_chat.generating') }}</p>
              <p v-if="message.error" class="text-error whitespace-pre-wrap break-words">{{ message.error }}</p>
              <div v-if="message.references.length" class="flex flex-wrap gap-2"><div v-for="(file, i) in message.references" :key="file.fileId + ':' + file.sourceStamp" class="relative group rounded-box border border-base-content/10 bg-base-100 min-w-0 max-w-64" data-chat-reference-card><button class="flex items-center gap-2 p-2 pr-8 w-full min-w-0 text-left hover:bg-base-200 rounded-box" :title="t('media_chat.view_media', { name: file.name })" @click="openFile(file)"><img v-if="thumbnails[file.fileId]" :src="thumbnails[file.fileId]" class="h-12 w-12 rounded object-cover shrink-0" alt="" /><IconFile v-else class="h-8 w-8 m-2 shrink-0 opacity-40" aria-hidden="true" /><span class="min-w-0"><span class="block truncate font-medium">[{{ i + 1 }}] {{ file.name }}</span><span class="block text-[10px] opacity-50">{{ mediaSummary(file) }}</span></span></button><button class="absolute right-1 top-1 btn btn-xs btn-square btn-ghost" :aria-label="t('media_chat.attach_again', { name: file.name })" :disabled="attachments.some(item => item.fileId === file.fileId)" @click="attachIds([file.fileId])">＋</button></div></div>
            </article>
          </div>
          <div class="shrink-0 border-t border-base-content/10 p-3 space-y-2">
            <div v-if="attachments.length" class="space-y-1"><span class="opacity-50">{{ t('media_chat.turn_media', { count: attachments.length }) }}</span><div class="flex gap-2 overflow-x-auto pb-1"><div v-for="file in attachments" :key="file.fileId" class="relative shrink-0 w-52 rounded-box border border-base-content/10 bg-base-200/50" data-chat-attachment-card><button class="flex gap-2 items-center p-2 pr-7 w-full min-w-0 text-left hover:bg-base-200 rounded-box" :title="t('media_chat.view_media', { name: file.name })" @click="openFile(file)"><img v-if="thumbnails[file.fileId]" :src="thumbnails[file.fileId]" class="h-12 w-12 rounded object-cover shrink-0" alt="" /><IconFile v-else class="h-8 w-8 m-2 shrink-0 opacity-40" aria-hidden="true" /><span class="min-w-0"><span class="block truncate font-medium">{{ file.name }}</span><span class="block text-[10px] opacity-50">{{ mediaSummary(file) }}</span></span></button><button type="button" class="btn btn-xs btn-square btn-ghost absolute right-0.5 top-0.5" :aria-label="t('media_chat.remove_attachment', { name: file.name })" @click="removeAttachment(file.fileId)">×</button></div></div></div>
            <p v-if="!readyModels.length" class="text-warning">{{ t('media_chat.no_ready_model') }}</p>
            <p v-else-if="selectedModelId && !readyModels.some(model => model.id === selectedModelId)" class="text-warning">{{ t('media_chat.choose_compatible_model') }}</p>
            <div class="relative">
              <div v-if="pickerOpen || mention" class="absolute z-30 bottom-full left-0 mb-2 w-full rounded-box border border-base-content/20 bg-base-100 shadow-xl p-2 space-y-2" role="listbox" :aria-label="t('media_chat.choose_media')" data-chat-mentions>
                <div class="flex gap-1"><button class="btn btn-xs" :class="candidateSource === 'session' ? 'btn-active' : ''" @mousedown.prevent @click="candidateSource = 'session'">{{ t('media_chat.from_session') }}</button><button class="btn btn-xs" :class="candidateSource === 'library' ? 'btn-active' : ''" @mousedown.prevent @click="candidateSource = 'library'">{{ t('media_chat.from_library') }}</button><button class="btn btn-xs btn-ghost ml-auto" @mousedown.prevent @click="closePicker">×</button></div>
                <input v-if="pickerOpen && !mention" ref="pickerSearch" v-model="pickerQuery" class="input input-bordered input-xs w-full" :placeholder="t('media_chat.search_media')" @keydown="candidateKeydown" />
                <div class="max-h-52 overflow-y-auto"><p v-if="candidateLoading" class="loading loading-spinner loading-xs"></p><button v-for="(file, i) in candidates" :key="file.fileId" class="flex gap-2 items-center rounded p-2 w-full min-w-0 text-left hover:bg-base-200" :class="i === candidateIndex ? 'bg-base-200' : ''" role="option" :aria-selected="i === candidateIndex" @mousedown.prevent @click="pickCandidate(file)"><img v-if="thumbnails[file.fileId]" :src="thumbnails[file.fileId]" class="w-9 h-9 shrink-0 rounded object-cover" alt="" /><span class="min-w-0 flex-1"><span class="block truncate">{{ file.name }}</span><span class="block truncate opacity-50 text-[10px]">{{ file.folderPath }}<span v-if="file.fileType === 2"> · {{ mediaTime(file.duration) }}</span></span></span><span v-if="file.cloudOnly" class="text-warning">{{ t('media_chat.cloud_only') }}</span></button><p v-if="!candidateLoading && !candidates.length" class="p-2 opacity-50">{{ t('media_chat.no_media') }}</p></div>
              </div>
              <textarea ref="composer" v-model="draft" rows="3" maxlength="8000" class="textarea textarea-bordered w-full resize-none leading-relaxed" :placeholder="t('media_chat.composer_placeholder')" :aria-label="t('media_chat.question')" @input="updateMention" @click="updateMention" @keyup="composerKeyup" @keydown="composerKeydown"></textarea>
            </div>
            <div class="flex flex-wrap items-center gap-2" data-chat-composer-controls>
              <div class="relative" data-chat-actions><button class="btn btn-sm btn-circle btn-ghost" :aria-label="t('media_chat.actions')" :aria-expanded="actionsOpen" @click="toggleActions">＋</button><div v-if="actionsOpen" class="absolute bottom-full left-0 mb-2 z-30 w-56 max-h-72 overflow-y-auto rounded-box border border-base-content/15 bg-base-100 shadow-xl p-1" role="menu" :aria-label="t('media_chat.actions')" data-chat-actions-menu><button v-for="(item, i) in menuItems" :key="item.id" type="button" class="block w-full rounded px-3 py-2 text-left hover:bg-base-200 disabled:opacity-40" :class="i === actionIndex ? 'bg-base-200' : ''" role="menuitem" :disabled="item.disabled" @click="runMenuAction(item.id)">{{ t('media_chat.' + item.label) }}</button></div></div>
              <select v-model="selectedModelId" class="select select-ghost select-sm min-w-0 flex-1 max-w-72" :aria-label="t('media_chat.model')"><option value="">{{ t('media_chat.choose_model') }}</option><option v-for="model in readyModels" :key="model.id" :value="model.id">{{ modelName(model) }}</option></select>
              <select v-if="selectedModelId" v-model="reasoningEffort" class="select select-ghost select-sm min-w-0 max-w-48" :disabled="!reasoningLevels.length" :aria-label="t('media_chat.reasoning')" data-chat-reasoning><option :value="null">{{ reasoningDefaultLabel }}</option><option v-if="defaultReasoningEffort" value="">{{ t('media_chat.reasoning_auto') }}</option><option v-for="level in reasoningLevels" :key="level" :value="level">{{ reasoningLabel(level) }}</option></select>
              <button v-if="activeRequestId" class="btn btn-sm ml-auto" @click="stopAnswer">{{ t('media_chat.stop') }}</button><button class="btn btn-sm btn-primary" :class="activeRequestId ? '' : 'ml-auto'" :disabled="!canSend" @click="send">{{ t('media_chat.send') }}</button>
            </div>
            <p class="text-[10px] opacity-50">{{ t('media_chat.scope_hint') }} {{ t('media_chat.close_hint') }}</p>
          </div>
        </main>
      </div>
      <div v-for="edge in CHAT_RESIZE_EDGES" :key="edge" class="absolute z-20 touch-none select-none outline-none focus-visible:bg-primary/15" :style="resizeHandleStyle(edge)" role="button" tabindex="0" :aria-label="t('media_chat.resize_' + edge)" :data-chat-resize="edge" @pointerdown.stop="beginFrameGesture($event, 'resize', edge)" @lostpointercapture="endFrameGesture" @keydown.stop="resizeKeydown($event, edge)"></div>
    </section>
    <ModalDialog v-if="previewOpen" :key="viewport.width + ':' + viewport.height" :title="t('media_chat.metadata_preview')" :width="previewSize.width" :height="previewSize.height" @cancel="previewOpen = false">
      <fieldset class="text-xs space-y-3 flex-1 min-h-0 min-w-0 overflow-y-auto" :disabled="applying" data-chat-adoption-preview @keydown.esc.stop.prevent="previewOpen = false" @keydown.stop>
        <p class="opacity-60">{{ t('media_chat.adoption_hint') }}</p>
        <p v-if="suggestionJob" role="status">{{ t('media_chat.preparing_suggestions') }} <button class="btn btn-xs" @click="openTasks(false)">{{ t('media_chat.tasks') }}</button></p>
        <p v-if="previewError" role="alert" class="text-error whitespace-pre-wrap">{{ previewError }}</p>
        <label class="flex gap-2"><input v-model="onlyEmpty" type="checkbox" class="checkbox checkbox-xs" />{{ t('media_chat.only_empty') }}</label>
        <label class="flex gap-2"><input v-model="createTags" type="checkbox" class="checkbox checkbox-xs" />{{ t('media_chat.create_tags') }}</label>
        <article v-for="row in previewRows" :key="row.file.fileId" class="border border-base-content/10 rounded-box p-3 space-y-2">
          <strong class="break-words">{{ row.file.name }}</strong><p v-if="row.error" class="text-error whitespace-pre-wrap">{{ row.error }}</p><p v-if="row.applied" class="text-success">{{ t('media_chat.applied') }}</p>
          <template v-if="row.analyses.length"><select v-if="row.analyses.length > 1" v-model="row.analysisId" class="select select-bordered select-xs w-full" :disabled="row.applied" @change="selectPreviewAnalysis(row)"><option v-for="analysis in row.analyses" :key="analysis.id" :value="analysis.id">{{ mediaTime(analysis.start) }}–{{ mediaTime(analysis.end) }} · {{ analysis.title }}</option></select><template v-for="analysis in row.analyses.filter(item => item.id === row.analysisId)" :key="analysis.id"><p class="text-[10px] opacity-50">{{ analysis.model?.model?.name }} · {{ new Date(analysis.createdAt).toLocaleString() }}</p><div v-for="field in adoptionFields" :key="field" class="space-y-1"><label class="flex gap-2 font-medium"><input v-model="row.fields[field]" type="checkbox" class="checkbox checkbox-xs" :disabled="row.applied || analysis.dismissed?.includes(field)" />{{ t('media_chat.field_' + field) }}<span v-if="analysis.dismissed?.includes(field)" class="text-warning font-normal">{{ t('media_ai.dismissed') }}</span></label><p class="opacity-50 whitespace-pre-wrap break-words">{{ t('media_chat.current_value') }}: {{ field === 'title' ? row.source?.title || '—' : field === 'description' ? row.source?.comments || '—' : row.currentTags.join('、') || '—' }}</p><input v-if="field === 'title'" v-model="row.proposal.title" class="input input-bordered input-xs w-full" maxlength="256" :disabled="row.applied || analysis.dismissed?.includes(field)" :aria-label="t('media_chat.field_title')" /><textarea v-else-if="field === 'description'" v-model="row.proposal.description" rows="3" class="textarea textarea-bordered w-full" maxlength="8000" :disabled="row.applied || analysis.dismissed?.includes(field)" :aria-label="t('media_chat.field_description')"></textarea><input v-else v-model="row.tagsText" class="input input-bordered input-xs w-full" :disabled="row.applied || analysis.dismissed?.includes(field)" :aria-label="t('media_chat.field_tags')" :placeholder="t('media_chat.tags_separator')" /></div></template></template>
        </article>
        <div class="flex justify-end gap-2"><button class="btn btn-sm" @click="previewOpen = false">{{ t('media_chat.close') }}</button><button class="btn btn-sm btn-primary" :disabled="applying || !adoptionCount || !!suggestionJob" @click="applySuggestions">{{ t('media_chat.apply_to_media', { count: adoptionCount }) }}</button></div>
      </fieldset>
    </ModalDialog>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { useUIStore } from '@/stores/uiStore';
import { getDragPayload, getFileThumbById, getTagsForFile } from '@/common/api';
import { getExternalDropPaths } from '@/common/externalDrop';
import { getThumbnailDataUrl, getThumbUrl, isWin, isMac } from '@/common/utils';
import { mediaTime, openMediaAiBatch, openMediaAiSettings, type MediaCapability, type MediaConfiguration, type MediaModel } from '@/common/mediaAi';
import { mediaChat, closeMediaChat, validMediaChatIds, type ChatCandidate, type ChatReference, type ChatSession, type ChatSessionView, type ChatSource } from '@/common/mediaChat';
import { openMediaChatMedia } from '@/common/mediaChatMedia';
import { CHAT_RESIZE_EDGES, fitChatFrame, insertMediaMention, mentionAtCaret, resizeChatFrame, slashAtCaret, type ChatFrame, type ChatResizeEdge, type MentionRange } from '@/common/mediaChatUi';
import { IconFile } from '@/common/icons';
import { pendingChanges } from '@/common/pendingChanges';
import ModalDialog from './ModalDialog.vue';

const { t, te } = useI18n(), uiStore = useUIStore();
const root = ref<HTMLElement | null>(null), composer = ref<HTMLTextAreaElement | null>(null), messagesRoot = ref<HTMLElement | null>(null);
const pickerSearch = ref<HTMLInputElement | null>(null);
const viewport = ref({ width: window.innerWidth, height: window.innerHeight });
const previewSize = computed(() => ({ width: Math.max(1, Math.min(860, viewport.value.width - 24)), height: Math.max(1, Math.min(740, viewport.value.height - 24)) }));
const frame = ref<ChatFrame>({ x: 12, y: 12, width: 1000, height: 680 });
const frameStyle = computed(() => ({ left: `${frame.value.x}px`, top: `${frame.value.y}px`, width: `${frame.value.width}px`, height: `${frame.value.height}px` }));
const showHistory = ref(true);
const sessions = ref<ChatSession[]>([]), view = ref<ChatSessionView | null>(null), activeLibrary = ref(''), configuration = ref<MediaConfiguration | null>(null);
const loading = ref(false), error = ref(''), notice = ref(''), draft = ref(''), attachments = ref<ChatCandidate[]>([]), incoming = ref<ChatCandidate[]>([]), incomingIds = ref<number[]>([]);
const visual = ref(true), selectedModelId = ref(''), renaming = ref(false), renameText = ref('');
const reasoningEffort = ref<string | null>(null), actionsOpen = ref(false), actionIndex = ref(0);
let actionSlash: number | null = null;
const request = ref<{ id: string; sessionId: string; libraryId: string } | null>(null);
const sending = computed(() => !!request.value);
const activeRequestId = computed(() => view.value?.session.runningRequestId || (request.value?.sessionId === view.value?.session.id ? request.value?.id : null));
const thumbnails = ref<Record<number, string>>({});
const mention = ref<MentionRange | null>(null), pickerOpen = ref(false), pickerQuery = ref(''), candidateSource = ref<'session' | 'library'>('library'), candidates = ref<ChatCandidate[]>([]), candidateIndex = ref(0), candidateLoading = ref(false), dragOver = ref(false);
const drafts = new Map<string, { text: string; attachments: ChatCandidate[]; modelId: string; reasoningEffort: string | null }>();
let disposed = false, generation = 0, candidateGeneration = 0, messageGeneration = 0, picking = false, inputRegistered = false, candidateTimer: ReturnType<typeof setTimeout> | undefined;
let refreshTimer: ReturnType<typeof setInterval> | undefined;
const sessionMedia = computed(() => {
  const files = new Map<number, ChatCandidate>();
  for (const message of view.value?.messages || []) for (const file of message.references) files.set(file.fileId, { fileId: file.fileId, name: file.name, folderPath: file.source.path.replace(/[\\/][^\\/]*$/, ''), fileType: file.fileType, width: file.source.width, height: file.source.height, duration: file.source.duration, cloudOnly: file.source.cloudOnly, hasAnalysis: file.evidence.length > 0, sourceStamp: file.sourceStamp });
  for (const file of attachments.value) files.set(file.fileId, file);
  return [...files.values()];
});
function chatCapabilities(pixels: boolean, files = attachments.value): MediaCapability[] {
  return pixels && files.length ? [...new Set(files.map(file => file.fileType === 2 ? 'video_frames' as const : 'image' as const))] : ['text'];
}
function modelReady(model: MediaModel, pixels: boolean, files = attachments.value) {
  const provider = configuration.value?.providers.find(item => item.id === model.providerId);
  if (!provider?.enabled || !provider.allowMedia || provider.kind !== 'llm' || model.capabilities.some(cap => cap.startsWith('embed_'))) return false;
  return chatCapabilities(pixels, files).every(cap => model.capabilities.includes(cap));
}
const readyModels = computed(() => configuration.value?.models.filter(model => modelReady(model, true)) || []);
const selectedModel = computed(() => configuration.value?.models.find(model => model.id === selectedModelId.value));
const reasoningLevels = computed(() => [...new Set(selectedModel.value?.reasoning?.levels || [])]);
const defaultReasoningEffort = computed(() => selectedModel.value?.reasoning?.defaultEffort || null);
function reasoningLabel(level: string) { const key = `media_chat.reasoning_level_${level}`; return te(key) ? t(key) : level; }
const reasoningDefaultLabel = computed(() => defaultReasoningEffort.value ? t('media_chat.reasoning_model_default', { level: reasoningLabel(defaultReasoningEffort.value) }) : t('media_chat.reasoning_auto'));
function normalizeReasoningChoice() { if (reasoningEffort.value === '' && !defaultReasoningEffort.value || reasoningEffort.value !== null && reasoningEffort.value !== '' && !reasoningLevels.value.includes(reasoningEffort.value)) reasoningEffort.value = null; }
const canGenerateSuggestions = computed(() => !!configuration.value?.models.some(model => model.id === selectedModelId.value && modelReady(model, true) && [...chatCapabilities(true), 'json' as const].every(cap => model.capabilities.includes(cap) && !!model.tested[cap])));
const canSend = computed(() => !loading.value && !sending.value && !activeRequestId.value && !!view.value && !!draft.value.trim() && draft.value.length <= 8000 && attachments.value.length <= 16 && readyModels.value.some(model => model.id === selectedModelId.value) && activeLibrary.value === libConfig._libraryId);
const modelName = (model: MediaModel) => `${configuration.value?.providers.find(provider => provider.id === model.providerId)?.name || ''} / ${model.name || model.remoteId}`;
function key() { return `${activeLibrary.value}:${view.value?.session.id || ''}`; }
function rememberDraft() { if (view.value) drafts.set(key(), { text: draft.value, attachments: [...attachments.value], modelId: selectedModelId.value, reasoningEffort: reasoningEffort.value }); }
function restoreDraft() {
  const saved = drafts.get(key());
  draft.value = saved?.text || ''; attachments.value = saved?.attachments || []; visual.value = true;
  selectedModelId.value = saved?.modelId || view.value?.session.modelId || '';
  if (!selectedModelId.value || !readyModels.value.some(model => model.id === selectedModelId.value)) selectedModelId.value = readyModels.value.find(model => model.id === configuration.value?.roles[attachments.value.length ? 'media' : 'text'])?.id || readyModels.value[0]?.id || '';
  reasoningEffort.value = saved?.reasoningEffort ?? null; normalizeReasoningChoice();
  closePicker(); actionsOpen.value = false; renaming.value = false;
}
function mediaSummary(file: ChatCandidate | ChatReference) { if ('source' in file) return file.fileType === 2 ? `${mediaTime(file.start)}–${mediaTime(file.end)}` : `${file.source.width} × ${file.source.height}`; return file.fileType === 2 ? mediaTime(file.duration) : `${file.width} × ${file.height}`; }
async function openFile(file: ChatCandidate | ChatReference) { const libraryId = activeLibrary.value; try { if (libraryId !== libConfig._libraryId) return; if (await openMediaChatMedia(libraryId, file.fileId, 'start' in file && file.fileType === 2 ? file.start : undefined)) closeMediaChat(); } catch (reason) { if (libraryId === activeLibrary.value) error.value = String(reason); } }
async function refreshSessions(libraryId = activeLibrary.value) {
  const rows = await invoke<ChatSession[]>('media_chat_sessions', { libraryId });
  if (!disposed && libraryId === activeLibrary.value) sessions.value = rows;
  return rows;
}
async function selectSession(sessionId: string) {
  rememberDraft(); const libraryId = activeLibrary.value, epoch = ++generation; loading.value = true; error.value = '';
  try { const next = await invoke<ChatSessionView>('media_chat_inspect', { libraryId, sessionId }); if (!disposed && epoch === generation && libraryId === activeLibrary.value) { view.value = next; restoreDraft(); await scrollMessages(); } }
  catch (reason) { if (epoch === generation) error.value = String(reason); }
  finally { if (epoch === generation) loading.value = false; }
}
async function newSession(files: ChatCandidate[] = []) {
  if (loading.value) return; rememberDraft(); const libraryId = activeLibrary.value, epoch = ++generation; loading.value = true; error.value = '';
  try { const next = await invoke<ChatSessionView>('media_chat_create', { libraryId, title: null }); if (disposed || epoch !== generation || libraryId !== activeLibrary.value) return; view.value = next; restoreDraft(); incoming.value = []; incomingIds.value = []; await addCandidates(files); await refreshSessions(libraryId); }
  catch (reason) { if (epoch === generation) error.value = String(reason); }
  finally { if (epoch === generation) loading.value = false; }
}
async function openEntry() {
  const libraryId = mediaChat.libraryId; if (!libraryId || libraryId !== libConfig._libraryId) return;
  const epoch = ++generation; loading.value = true; error.value = ''; notice.value = '';
  try {
    if (activeLibrary.value !== libraryId) { rememberDraft(); activeLibrary.value = libraryId; view.value = null; attachments.value = []; draft.value = ''; incoming.value = []; incomingIds.value = []; thumbnails.value = {}; }
    const [config, rows] = await Promise.all([invoke<{ configuration: MediaConfiguration }>('media_ai_configuration'), invoke<ChatSession[]>('media_chat_sessions', { libraryId })]);
    if (disposed || epoch !== generation || libraryId !== mediaChat.libraryId) return;
    configuration.value = config.configuration; sessions.value = rows;
    if (!view.value) { const restored = rows[0] ? await invoke<ChatSessionView>('media_chat_inspect', { libraryId, sessionId: rows[0].id }) : await invoke<ChatSessionView>('media_chat_create', { libraryId, title: null }); if (disposed || epoch !== generation) return; view.value = restored; restoreDraft(); await refreshSessions(libraryId); }
    if (mediaChat.fileIds.length) {
      const ids = validMediaChatIds(mediaChat.fileIds);
      if (ids.length > 200) { incomingIds.value = ids; incoming.value = []; notice.value = t('media_chat.candidate_limit'); return; }
      const files = await resolveIds(ids);
      if (disposed || epoch !== generation) return;
      if (!view.value.messages.length && !draft.value && !attachments.value.length && ids.length <= 16) await addCandidates(files);
      else { incoming.value = files; incomingIds.value = ids; }
    }
    await scrollMessages();
  } catch (reason) { if (epoch === generation) error.value = String(reason); }
  finally { if (epoch === generation) loading.value = false; }
}
async function renameSession() {
  if (!view.value || !renameText.value.trim()) return;
  const libraryId = activeLibrary.value, sessionId = view.value.session.id;
  try { const renamed = await invoke<ChatSession>('media_chat_rename', { libraryId, sessionId, title: renameText.value.trim() }); if (view.value?.session.id === sessionId && activeLibrary.value === libraryId) view.value.session = renamed; renaming.value = false; await refreshSessions(libraryId); }
  catch (reason) { error.value = String(reason); }
}
async function deleteSession() {
  if (!view.value) return; const libraryId = activeLibrary.value, sessionId = view.value.session.id;
  if (!await ask(t('media_chat.delete_confirm', { name: view.value.session.title }), { kind: 'warning' })) return;
  try { await invoke('media_chat_delete', { libraryId, sessionId }); drafts.delete(`${libraryId}:${sessionId}`); if (activeLibrary.value !== libraryId || view.value?.session.id !== sessionId) return; view.value = null; const rows = await refreshSessions(libraryId); if (rows[0]) await selectSession(rows[0].id); else await newSession(); }
  catch (reason) { error.value = String(reason); }
}
async function resolveIds(ids: number[]) {
  if (ids.length > 16) notice.value = t('media_chat.reference_limit');
  const validIds = validMediaChatIds(ids);
  if (validIds.length > 200) throw new Error(t('media_chat.candidate_limit'));
  return invoke<ChatCandidate[]>('media_chat_media', { libraryId: activeLibrary.value, query: null, fileIds: validIds, limit: 200 });
}
async function addCandidates(files: ChatCandidate[]) {
  const all = new Map(attachments.value.map(file => [file.fileId, file])); for (const file of files) all.set(file.fileId, file);
  if (all.size > 16) { notice.value = t('media_chat.reference_limit'); return false; }
  attachments.value = [...all.values()]; rememberDraft(); void loadThumbnails(files); return true;
}
async function attachIds(ids: number[]) {
  const libraryId = activeLibrary.value, sessionId = view.value?.session.id;
  try {
    const validIds = validMediaChatIds(ids), files = validIds.length <= 200 ? await resolveIds(validIds) : [];
    if (disposed || libraryId !== activeLibrary.value || libraryId !== libConfig._libraryId || sessionId !== view.value?.session.id) return;
    if (validIds.length > 16) { incomingIds.value = validIds; incoming.value = files; notice.value = t(validIds.length > 200 ? 'media_chat.candidate_limit' : 'media_chat.reference_limit'); return; }
    if (files.length < validIds.length) notice.value = t('media_chat.media_unavailable');
    await addCandidates(files);
  }
  catch (reason) { if (!disposed && libraryId === activeLibrary.value && libraryId === libConfig._libraryId && sessionId === view.value?.session.id) error.value = String(reason); }
}
async function addIncoming() { if (incomingIds.value.length > 16) { notice.value = t('media_chat.reference_limit'); return; } if (await addCandidates(incoming.value)) { incoming.value = []; incomingIds.value = []; } }
function removeAttachment(fileId: number) { attachments.value = attachments.value.filter(file => file.fileId !== fileId); rememberDraft(); }
async function loadThumbnails(files: ChatCandidate[]) {
  const libraryId = activeLibrary.value;
  await Promise.all(files.slice(0, 60).filter(file => !thumbnails.value[file.fileId]).map(async file => { try { const url = isWin ? getThumbnailDataUrl(await getFileThumbById(file.fileId, 128, false), '', false, 128) : getThumbUrl(file.fileId, false, 128); if (!disposed && libraryId === activeLibrary.value && url) thumbnails.value[file.fileId] = url; } catch { /* The name remains usable without a thumbnail. */ } }));
}
function closePicker() { mention.value = null; pickerOpen.value = false; candidateGeneration++; }
function openPicker() { actionsOpen.value = false; mention.value = null; pickerOpen.value = true; pickerQuery.value = ''; candidateSource.value = sessionMedia.value.length ? 'session' : 'library'; void loadCandidates(); void nextTick(() => pickerSearch.value?.focus()); }
function updateMention() { const caret = composer.value?.selectionStart ?? draft.value.length; mention.value = mentionAtCaret(draft.value, caret); if (mention.value) { pickerOpen.value = false; actionsOpen.value = false; } else { const slash = slashAtCaret(draft.value, caret); if (slash !== null) { closePicker(); actionsOpen.value = true; actionSlash = slash; actionIndex.value = 0; } else if (actionSlash !== null) { actionsOpen.value = false; actionSlash = null; } } }
function composerKeyup(event: KeyboardEvent) { if (!['Escape', 'Enter', 'Tab', 'ArrowUp', 'ArrowDown'].includes(event.key)) updateMention(); }
async function loadCandidates() {
  const epoch = ++candidateGeneration, libraryId = activeLibrary.value, query = mention.value?.query || pickerQuery.value;
  if (!mention.value && !pickerOpen.value) return;
  candidateLoading.value = true;
  try { const rows = candidateSource.value === 'session' ? sessionMedia.value.filter(file => `${file.name} ${file.folderPath}`.toLowerCase().includes(query.toLowerCase())).slice(0, 60) : await invoke<ChatCandidate[]>('media_chat_media', { libraryId, query, fileIds: null, limit: 60 }); if (!disposed && epoch === candidateGeneration && libraryId === activeLibrary.value) { candidates.value = rows; candidateIndex.value = 0; void loadThumbnails(rows); } }
  catch (reason) { if (epoch === candidateGeneration) error.value = String(reason); }
  finally { if (epoch === candidateGeneration) candidateLoading.value = false; }
}
async function pickCandidate(file: ChatCandidate) {
  if (picking) return;
  picking = true;
  const range = mention.value, originalDraft = draft.value, sessionId = view.value?.session.id, libraryId = activeLibrary.value;
  try { const current = (await resolveIds([file.fileId]))[0]; if (libraryId !== activeLibrary.value || sessionId !== view.value?.session.id) return; if (!current) { notice.value = t('media_chat.media_unavailable'); return; } if (!await addCandidates([current])) return; if (range && draft.value === originalDraft) { const inserted = insertMediaMention(draft.value, range, current.name); draft.value = inserted.text; await nextTick(); composer.value?.setSelectionRange(inserted.caret, inserted.caret); } closePicker(); composer.value?.focus(); rememberDraft(); }
  catch (reason) { error.value = String(reason); }
  finally { picking = false; }
}
function candidateKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); closePicker(); return true; }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); if (candidates.value.length) candidateIndex.value = (candidateIndex.value + (event.key === 'ArrowDown' ? 1 : -1) + candidates.value.length) % candidates.value.length; return true; }
  if (event.key === 'Enter' || event.key === 'Tab') { event.preventDefault(); if (candidates.value[candidateIndex.value]) void pickCandidate(candidates.value[candidateIndex.value]); return true; }
  return false;
}
function composerKeydown(event: KeyboardEvent) { if (event.isComposing) return; if ((mention.value || pickerOpen.value) && candidateKeydown(event)) return; if (actionsOpen.value && actionKeydown(event)) return; if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); void send(); } }
function dialogKeydown(event: KeyboardEvent) { if (event.key === 'Escape' && uiStore.isInputActive('MediaChat')) { event.preventDefault(); if (previewOpen.value) previewOpen.value = false; else if (mention.value || pickerOpen.value) closePicker(); else if (actionsOpen.value) { actionsOpen.value = false; actionSlash = null; } else closeMediaChat(); } }
async function send() {
  if (!canSend.value || !view.value) return;
  const libraryId = activeLibrary.value, sessionId = view.value.session.id, question = draft.value, refs = attachments.value.map(file => ({ fileId: file.fileId, start: null })), modelId = selectedModelId.value, effort = reasoningEffort.value, requestId = crypto.randomUUID();
  rememberDraft(); request.value = { id: requestId, sessionId, libraryId }; error.value = ''; closePicker();
  try { const next = await invoke<ChatSessionView>('media_chat_send', { libraryId, sessionId, modelId, question, references: refs, visual: refs.length > 0, reasoningEffort: effort, requestId }); const completed = next.messages.some(message => message.requestId === requestId && message.role === 'assistant' && message.status === 'completed'); const saved = drafts.get(`${libraryId}:${sessionId}`); if (completed && saved?.text === question) saved.text = ''; if (!disposed && libraryId === activeLibrary.value && sessionId === view.value?.session.id) { messageGeneration++; view.value = next; if (draft.value === question && completed) { draft.value = ''; rememberDraft(); } await scrollMessages(); } await refreshSessions(libraryId); }
  catch (reason) { if (libraryId === activeLibrary.value && sessionId === view.value?.session.id) { error.value = String(reason); await refreshCurrent(); } }
  finally { if (request.value?.id === requestId) request.value = null; }
}
async function stopAnswer() { const requestId = activeRequestId.value; if (!requestId) return; try { await invoke('media_ai_cancel_request', { requestId }); await refreshCurrent(); } catch (reason) { error.value = String(reason); } }
async function refreshCurrent() {
  const libraryId = activeLibrary.value, sessionId = view.value?.session.id, epoch = ++messageGeneration; if (!sessionId) return;
  const next = await invoke<ChatSessionView>('media_chat_inspect', { libraryId, sessionId }); if (!disposed && epoch === messageGeneration && libraryId === activeLibrary.value && sessionId === view.value?.session.id) view.value = next;
}
async function scrollMessages() { await nextTick(); if (messagesRoot.value) messagesRoot.value.scrollTop = messagesRoot.value.scrollHeight; }
async function internalDrop(event: Event) { const detail = (event as CustomEvent<{ libraryId: string; fileIds: number[] }>).detail; if (detail?.libraryId !== activeLibrary.value) { notice.value = t('media_chat.wrong_library'); return; } await attachIds(detail.fileIds); }
async function dropFiles(event: DragEvent) {
  dragOver.value = false; const paths = getExternalDropPaths(event.dataTransfer); const hasFiles = !!event.dataTransfer?.files.length;
  const libraryId = activeLibrary.value, sessionId = view.value?.session.id;
  try { if (!paths.length && (hasFiles || isMac)) paths.push(...((await getDragPayload())?.filePaths || [])); if (!paths.length) { notice.value = t('media_chat.import_first'); return; } const uniquePaths = [...new Set(paths)]; if (uniquePaths.length > 200) { notice.value = t('media_chat.candidate_limit'); return; } const files = await invoke<ChatCandidate[]>('media_chat_media', { libraryId, query: null, fileIds: null, paths: uniquePaths, limit: 200 }); if (libraryId !== activeLibrary.value || sessionId !== view.value?.session.id) return; if (files.length < uniquePaths.length) notice.value = t('media_chat.import_first'); await addCandidates(files); } catch (reason) { error.value = String(reason); }
}
function openTasks(all: boolean) { previewOpen.value = false; openMediaAiBatch(activeLibrary.value, all ? null : attachments.value.length ? attachments.value.map(file => file.fileId) : sessionMedia.value.map(file => file.fileId), selectedModelId.value || null); }
function openIncomingTasks() { previewOpen.value = false; openMediaAiBatch(activeLibrary.value, [...incomingIds.value], selectedModelId.value || null); }

type AdoptionField = 'title' | 'description' | 'tags';
interface PreviewRow { file: ChatCandidate; source: ChatSource | null; analyses: any[]; analysisId: string; fields: Record<AdoptionField, boolean>; proposal: { title: string; description: string; tags: string[] }; tagsText: string; currentTags: string[]; error: string; applied: boolean }
const adoptionFields: AdoptionField[] = ['title', 'description', 'tags'];
const previewOpen = ref(false), previewRows = ref<PreviewRow[]>([]), previewError = ref(''), onlyEmpty = ref(true), createTags = ref(false), applying = ref(false);
const suggestionJob = ref(''); let suggestionLibrary = '', suggestionModelId = '', refreshingSuggestions = false;
function previewFields(row: PreviewRow) { const dismissed = row.analyses.find(item => item.id === row.analysisId)?.dismissed || []; return { title: row.fields.title && !dismissed.includes('title'), description: row.fields.description && !dismissed.includes('description'), tags: row.fields.tags && !dismissed.includes('tags') }; }
const adoptionCount = computed(() => previewRows.value.filter(row => !row.applied && row.analysisId && Object.values(previewFields(row)).some(Boolean)).length);
type MenuAction = 'attach' | 'title' | 'description' | 'tags' | 'review' | 'selected' | 'tasks' | 'all' | 'configure';
const menuItems = computed(() => {
  const unavailable = !attachments.value.length || !!suggestionJob.value || !canGenerateSuggestions.value || applying.value;
  const items: Array<{ id: MenuAction; label: string; disabled: boolean }> = [
    { id: 'attach', label: 'attach', disabled: false },
    { id: 'title', label: 'fill_title', disabled: unavailable },
    { id: 'description', label: 'fill_description', disabled: unavailable },
    { id: 'tags', label: 'fill_tags', disabled: unavailable },
  ];
  if (previewRows.value.length) items.push({ id: 'review', label: 'review_suggestions', disabled: false });
  if (incomingIds.value.length) items.push({ id: 'selected', label: 'selected_media_tasks', disabled: false });
  items.push({ id: 'tasks', label: 'tasks', disabled: false }, { id: 'all', label: 'all_media_tasks', disabled: false }, { id: 'configure', label: 'configure', disabled: false });
  return items;
});
function toggleActions() { const opening = !actionsOpen.value; closePicker(); actionSlash = null; actionsOpen.value = opening; actionIndex.value = 0; }
function dismissActionsOutside(event: PointerEvent) { if (!(event.target as HTMLElement)?.closest('[data-chat-actions]')) { actionsOpen.value = false; actionSlash = null; } }
async function runMenuAction(id: MenuAction) {
  const item = menuItems.value.find(item => item.id === id); if (!item || item.disabled) return;
  if (actionSlash !== null && draft.value[actionSlash] === '/') draft.value = draft.value.slice(0, actionSlash) + draft.value.slice(actionSlash + 1);
  actionsOpen.value = false; actionSlash = null;
  if (id === 'attach') openPicker();
  else if (id === 'title' || id === 'description' || id === 'tags') await generateSuggestions(id);
  else if (id === 'review') previewOpen.value = true;
  else if (id === 'selected') openIncomingTasks();
  else if (id === 'tasks' || id === 'all') openTasks(id === 'all');
  else if (id === 'configure') await openMediaAiSettings();
}
function actionKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); actionsOpen.value = false; actionSlash = null; return true; }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault(); const direction = event.key === 'ArrowDown' ? 1 : -1;
    for (let i = 0; i < menuItems.value.length; i++) { actionIndex.value = (actionIndex.value + direction + menuItems.value.length) % menuItems.value.length; if (!menuItems.value[actionIndex.value].disabled) break; }
    return true;
  }
  if (event.key === 'Enter' || event.key === 'Tab') { event.preventDefault(); const item = menuItems.value[actionIndex.value]; if (item) void runMenuAction(item.id); return true; }
  return false;
}
async function generateSuggestions(field: AdoptionField) {
  if (!attachments.value.length || !canGenerateSuggestions.value || suggestionJob.value || applying.value) return;
  suggestionLibrary = activeLibrary.value; suggestionJob.value = 'preparing'; previewOpen.value = true; previewError.value = ''; previewRows.value = attachments.value.map(file => ({ file: { ...file }, source: null, analyses: [], analysisId: '', fields: { title: field === 'title', description: field === 'description', tags: field === 'tags' }, proposal: { title: '', description: '', tags: [] }, tagsText: '', currentTags: [], error: '', applied: false }));
  const libraryId = suggestionLibrary; suggestionModelId = selectedModelId.value;
  try { const jobId = await invoke<string>('media_ai_start_job', { libraryId, fileIds: previewRows.value.map(row => row.file.fileId), options: { modelId: suggestionModelId || null, indexOnly: false, index: false, visualOnly: false, visualIndex: false, force: true, adopt: { title: false, description: false, tags: false, createTags: false, onlyEmpty: true } } }); if (libraryId === activeLibrary.value && libraryId === libConfig._libraryId) suggestionJob.value = jobId; }
  catch (reason) { previewError.value = String(reason); suggestionJob.value = ''; }
}
function selectPreviewAnalysis(row: PreviewRow) { const analysis = row.analyses.find(item => item.id === row.analysisId); if (!analysis) return; row.fields = previewFields(row); row.proposal = { title: analysis.title, description: analysis.description, tags: [...analysis.tags] }; row.tagsText = analysis.tags.join('、'); }
async function refreshSuggestions() {
  if (!suggestionJob.value || suggestionJob.value === 'preparing' || refreshingSuggestions || suggestionLibrary !== libConfig._libraryId) return;
  refreshingSuggestions = true;
  try {
  const jobId = suggestionJob.value, libraryId = suggestionLibrary, rows = previewRows.value;
  const jobs = await invoke<any[]>('media_ai_jobs', { libraryId }); const job = jobs.find(item => item.id === jobId);
  if (jobId !== suggestionJob.value || libraryId !== activeLibrary.value) return;
  if (!job || job.state === 'running') return;
  const failed = new Map<number, string>((job.items || []).filter((item: any) => item.state === 'failed' || item.message).map((item: any) => [Number(item.fileId), item.message || t('media_chat.suggestion_unavailable')]));
  const unfinished = new Set<number>((job.items || []).filter((item: any) => ['pending', 'running'].includes(item.state)).map((item: any) => Number(item.fileId)));
  for (const row of rows) {
    if (failed.has(row.file.fileId)) { row.error = failed.get(row.file.fileId)!; continue; }
    if (unfinished.has(row.file.fileId)) { row.error = t('media_chat.suggestion_unavailable'); continue; }
    try { const inspected: any = await invoke('media_ai_inspect', { libraryId, fileId: row.file.fileId }); if (libraryId !== libConfig._libraryId || jobId !== suggestionJob.value) return; row.source = inspected.source; row.analyses = [...inspected.analyses].filter((analysis: any) => analysis.model?.model?.id === suggestionModelId && analysis.createdAt >= (job.createdAt || 0)).sort((a: any, b: any) => a.segment - b.segment || b.createdAt - a.createdAt); row.analysisId = row.analyses[0]?.id || ''; row.currentTags = ((await getTagsForFile(row.file.fileId)) || []).map((tag: any) => String(tag.name || tag.tag_name || '')).filter(Boolean); selectPreviewAnalysis(row); if (!row.analysisId) row.error = t('media_chat.suggestion_unavailable'); }
    catch (reason) { row.error = String(reason); }
  }
  if (jobId === suggestionJob.value) suggestionJob.value = '';
  } finally { refreshingSuggestions = false; }
}
async function applySuggestions() {
  if (applying.value || !adoptionCount.value || suggestionLibrary !== libConfig._libraryId) return;
  const libraryId = suggestionLibrary;
  const payloads = () => previewRows.value.filter(row => !row.applied && row.analysisId && Object.values(previewFields(row)).some(Boolean)).map(row => ({ libraryId, source: JSON.parse(JSON.stringify(row.source)), analysisId: row.analysisId, options: { ...previewFields(row), proposal: { title: row.proposal.title, description: row.proposal.description, tags: [...new Set(row.tagsText.split(/[、,，;；\n]/).map(tag => tag.trim()).filter(Boolean))] }, createTags: createTags.value, onlyEmpty: onlyEmpty.value } }));
  const targets = payloads(), snapshot = JSON.stringify(targets);
  if (!await pendingChanges.confirm('media-title') || libraryId !== libConfig._libraryId || snapshot !== JSON.stringify(payloads())) return;
  applying.value = true;
  try { for (const target of targets) { if (libraryId !== libConfig._libraryId) break; const row = previewRows.value.find(row => row.analysisId === target.analysisId && row.file.fileId === target.source.fileId); if (!row) break; try { await invoke('media_ai_adopt', target); row.applied = true; row.error = ''; } catch (reason) { row.error = String(reason); } } }
  finally { applying.value = false; }
}

let gesture: { mode: 'move' | 'resize'; edge?: ChatResizeEdge; pointerId: number; x: number; y: number; frame: ChatFrame; target: HTMLElement } | undefined;
function fitFrame() { endFrameGesture(); viewport.value = { width: window.innerWidth, height: window.innerHeight }; frame.value = fitChatFrame(frame.value, viewport.value); if (frame.value.width < 700) showHistory.value = false; }
function saveFrame() { try { localStorage.setItem('lap.media-chat.window', JSON.stringify(frame.value)); } catch { /* The dialog remains usable without storage. */ } }
function resizeHandleStyle(edge: ChatResizeEdge) {
  const style: Record<string, string> = { cursor: `${edge}-resize` };
  if (edge.length === 2) { style.width = '12px'; style.height = '12px'; }
  else if (edge === 'n' || edge === 's') { style.height = '6px'; style.left = '12px'; style.right = '12px'; }
  else { style.width = '6px'; style.top = '12px'; style.bottom = '12px'; }
  if (edge.includes('n')) style.top = '0'; if (edge.includes('s')) style.bottom = '0';
  if (edge.includes('w')) style.left = '0'; if (edge.includes('e')) style.right = '0';
  return style;
}
function beginFrameGesture(event: PointerEvent, mode: 'move' | 'resize', edge?: ChatResizeEdge) { if (event.button !== 0 || !event.isPrimary || (event.target as HTMLElement)?.closest('button, input, select, a')) return; event.preventDefault(); endFrameGesture(); const target = event.currentTarget as HTMLElement; gesture = { mode, edge, pointerId: event.pointerId, x: event.clientX, y: event.clientY, frame: { ...frame.value }, target }; try { target.setPointerCapture(event.pointerId); } catch { /* Window listeners also support platforms without pointer capture. */ } window.addEventListener('pointermove', moveFrameGesture); window.addEventListener('pointerup', endFrameGesture); window.addEventListener('pointercancel', endFrameGesture); window.addEventListener('blur', endFrameGesture); }
function moveFrameGesture(event: PointerEvent) { if (!gesture || event.pointerId !== gesture.pointerId) return; const dx = event.clientX - gesture.x, dy = event.clientY - gesture.y; frame.value = gesture.mode === 'move' ? fitChatFrame({ ...gesture.frame, x: gesture.frame.x + dx, y: gesture.frame.y + dy }, viewport.value) : resizeChatFrame(gesture.frame, gesture.edge || 'se', dx, dy, viewport.value); if (frame.value.width < 700) showHistory.value = false; }
function endFrameGesture(event?: PointerEvent | Event) { if (!gesture || event && 'pointerId' in event && event.pointerId !== gesture.pointerId) return; const old = gesture; gesture = undefined; if (old.target.hasPointerCapture?.(old.pointerId)) old.target.releasePointerCapture(old.pointerId); window.removeEventListener('pointermove', moveFrameGesture); window.removeEventListener('pointerup', endFrameGesture); window.removeEventListener('pointercancel', endFrameGesture); window.removeEventListener('blur', endFrameGesture); saveFrame(); }
function resizeKeydown(event: KeyboardEvent, edge: ChatResizeEdge) { if (event.key === 'Escape') { endFrameGesture(); return; } if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return; event.preventDefault(); const step = event.shiftKey ? 16 : 8; frame.value = resizeChatFrame(frame.value, edge, event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0, event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0, viewport.value); saveFrame(); }
watch(() => [mediaChat.open, mediaChat.revision], () => { if (mediaChat.open) void openEntry(); else { rememberDraft(); closePicker(); actionsOpen.value = false; actionSlash = null; endFrameGesture(); previewOpen.value = false; } }, { immediate: true });
watch(() => mediaChat.open, open => { if (open && !inputRegistered) { uiStore.pushInputHandler('MediaChat'); inputRegistered = true; } else if (!open && inputRegistered) { uiStore.removeInputHandler('MediaChat'); inputRegistered = false; } }, { immediate: true });
watch(() => libConfig._libraryId, libraryId => { if (activeLibrary.value && libraryId !== activeLibrary.value) { rememberDraft(); generation++; suggestionJob.value = ''; suggestionLibrary = ''; previewRows.value = []; previewError.value = ''; previewOpen.value = false; closeMediaChat(); } });
watch(() => [mention.value?.query, pickerQuery.value, candidateSource.value], () => { clearTimeout(candidateTimer); candidateTimer = setTimeout(() => { void loadCandidates(); }, 150); });
watch(sessionMedia, files => { if (mediaChat.open) void loadThumbnails(files); });
watch([selectedModelId, reasoningLevels, defaultReasoningEffort], normalizeReasoningChoice, { flush: 'sync' });
watch([draft, attachments, selectedModelId, reasoningEffort], rememberDraft, { deep: true });
onMounted(() => { try { const stored = localStorage.getItem('lap.media-chat.window'); frame.value = fitChatFrame(stored ? JSON.parse(stored) : {}, { width: window.innerWidth, height: window.innerHeight }); } catch { /* Use the initial frame. */ } fitFrame(); window.addEventListener('resize', fitFrame); refreshTimer = setInterval(() => { if (suggestionJob.value) void refreshSuggestions().catch(reason => { previewError.value = String(reason); }); if (mediaChat.open && activeRequestId.value) void refreshCurrent().catch(reason => { error.value = String(reason); }); }, 1500); });
onUnmounted(() => { disposed = true; generation++; candidateGeneration++; clearInterval(refreshTimer); clearTimeout(candidateTimer); endFrameGesture(); saveFrame(); window.removeEventListener('resize', fitFrame); if (inputRegistered) uiStore.removeInputHandler('MediaChat'); });
</script>
