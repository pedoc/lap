<template>
  <ModalDialog v-if="dlna.visible" :title="t('dlna.title')" :width="width" @cancel="closeDlna">
    <div ref="root" role="dialog" aria-modal="true" :aria-label="t('dlna.title')" tabindex="-1" class="flex flex-col gap-4 max-h-[75vh] overflow-y-auto p-1" @keydown.esc.stop="closeDlna" @keydown.tab="trapFocus">
      <p class="text-sm opacity-70">{{ dlna.mode === 'browse' ? t('dlna.browse_intro') : t('dlna.intro') }}</p>
      <fieldset class="grid grid-cols-2 gap-2" :disabled="dlna.busy || !!dlna.sessionId" :aria-label="t('dlna.mode')">
        <label v-for="mode in ['cast', 'browse'] as const" :key="mode" class="rounded-box border p-3 flex items-start gap-2 cursor-pointer" :class="dlna.mode === mode ? 'border-primary bg-primary/5' : 'border-base-content/15'">
          <input type="radio" name="dlna-mode" class="radio radio-sm radio-primary mt-0.5" :checked="dlna.mode === mode" @change="setDlnaMode(mode)" />
          <span><span class="block text-sm font-medium">{{ t(`dlna.mode_${mode}`) }}</span><span class="block text-xs opacity-65 mt-1">{{ t(`dlna.mode_${mode}_hint`) }}</span></span>
        </label>
      </fieldset>
      <fieldset class="rounded-box border border-base-content/15 p-3 space-y-2" :disabled="dlna.busy || !!dlna.sessionId" :aria-label="t('dlna.scope')">
        <legend class="px-1 text-xs opacity-65">{{ t('dlna.scope') }}</legend>
        <label v-if="dlna.selectedIds.length" class="flex items-center gap-2 cursor-pointer"><input type="radio" name="dlna-scope" class="radio radio-sm radio-primary" :checked="dlna.scopeKind === 'selection'" @change="setDlnaScope('selection')" /><span class="text-sm">{{ t('dlna.scope_selection', { count: dlna.selectedIds.length }) }}</span></label>
        <label v-if="dlna.albumId" class="flex items-center gap-2 cursor-pointer"><input type="radio" name="dlna-scope" class="radio radio-sm radio-primary" :checked="dlna.scopeKind === 'album'" @change="setDlnaScope('album')" /><span class="text-sm">{{ t('dlna.scope_album') }}<span v-if="dlna.albumName" class="opacity-65"> · {{ dlna.albumName }}</span></span></label>
        <p v-if="dlna.scopeKind === 'album'" class="text-xs opacity-60">{{ t('dlna.album_hint') }}</p>
      </fieldset>
      <template v-if="dlna.mode === 'cast'">
      <div class="flex items-center justify-between gap-3">
        <span class="text-sm font-semibold">{{ t('dlna.devices') }}</span>
        <button type="button" class="btn btn-sm btn-ghost" :disabled="dlna.discovering || dlna.busy || !!dlna.sessionId" @click="discoverDlna">
          <span v-if="dlna.discovering" class="loading loading-spinner loading-xs" aria-hidden="true" />
          {{ dlna.discovering ? t('dlna.searching') : t('dlna.refresh') }}
        </button>
      </div>
      <fieldset class="space-y-2" :disabled="dlna.busy || !!dlna.sessionId" :aria-label="t('dlna.devices')">
        <label v-for="device in dlna.devices" :key="device.id" class="flex items-center gap-3 rounded-box border p-3 cursor-pointer" :class="dlna.deviceId === device.id ? 'border-primary bg-primary/5' : 'border-base-content/15'">
          <input v-model="dlna.deviceId" class="radio radio-primary radio-sm" type="radio" name="dlna-device" :value="device.id" />
          <span class="min-w-0 flex-1"><span class="block break-words text-sm font-medium">{{ device.name }}</span><span class="text-xs opacity-60">{{ device.address }}</span></span>
        </label>
      </fieldset>
      <p v-if="!dlna.discovering && !dlna.devices.length" class="rounded-box bg-base-200 p-3 text-sm">{{ t('dlna.empty') }}</p>
      <div v-if="dlna.scopeKind === 'album' || dlna.selectedIds.length > 1" class="space-y-2">
        <div class="flex flex-wrap items-center gap-3">
          <label class="flex items-center gap-2 text-sm cursor-pointer"><input type="checkbox" class="checkbox checkbox-sm checkbox-primary" :checked="dlna.autoPlay" @change="changeAutoPlay" />{{ t('dlna.continuous') }}</label>
          <label class="flex items-center gap-2 text-xs opacity-80">{{ t('dlna.photo_interval') }}<select class="select select-sm w-20" :value="dlna.intervalSeconds" @change="changeInterval"><option v-for="seconds in [3, 5, 10, 15, 30]" :key="seconds" :value="seconds">{{ seconds }} s</option></select></label>
        </div>
        <p class="text-xs opacity-60">{{ t('dlna.continuous_hint') }}</p>
      </div>
      </template>
      <div class="rounded-box bg-base-200 p-3 space-y-2" aria-live="polite">
        <p v-if="dlna.mode === 'cast' && dlna.total" class="text-xs opacity-70">{{ t('dlna.counter', { current: dlna.index + 1, total: dlna.total }) }}</p>
        <p v-if="dlna.busy" class="text-sm flex items-center gap-2"><span class="loading loading-spinner loading-xs" aria-hidden="true" />{{ dlna.mode === 'browse' ? t('dlna.starting_share') : t('dlna.sending') }}</p>
        <template v-else-if="dlna.mode === 'browse' && dlna.serverName"><p class="text-sm font-semibold break-words">{{ dlna.serverName }}</p><p class="text-sm">{{ t('dlna.share_active', { count: dlna.total }) }}</p><p class="text-xs opacity-70">{{ t('dlna.tv_path') }}</p></template>
        <p v-else-if="dlna.mode === 'browse'" class="text-sm">{{ t('dlna.browse_ready') }}</p>
        <template v-else-if="dlna.title"><p class="text-sm font-medium break-words">{{ dlna.title }}</p><p class="text-xs opacity-70">{{ dlna.paused ? t('dlna.paused') : t('dlna.sent') }}</p></template>
        <p v-else class="text-sm">{{ t('dlna.ready') }}</p>
      </div>
      <p v-if="dlna.error" class="text-sm text-error break-words" role="alert">{{ dlna.error }}</p>
      <p class="text-xs opacity-60">{{ t('dlna.formats') }}</p>
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div v-if="dlna.mode === 'cast'" class="flex flex-wrap gap-2">
          <button type="button" class="btn btn-sm" :disabled="dlna.busy || !dlna.sessionId || dlna.index === 0" @click="castDlna(dlna.index - 1)">{{ t('dlna.previous') }}</button>
          <button v-if="dlna.kind === 'video'" type="button" class="btn btn-sm" :disabled="dlna.busy" @click="toggleDlnaPause">{{ dlna.paused ? t('dlna.resume') : t('dlna.pause') }}</button>
          <button type="button" class="btn btn-sm" :disabled="dlna.busy || !dlna.sessionId || dlna.index >= dlna.fileIds.length - 1" @click="castDlna(dlna.index + 1)">{{ t('dlna.next') }}</button>
        </div>
        <div class="flex gap-2">
          <button type="button" class="btn btn-sm" @click="closeDlna">{{ dlna.sessionId || dlna.busy ? dlna.mode === 'browse' ? t('dlna.stop_share') : t('dlna.stop') : t('dlna.close') }}</button>
          <button v-if="dlna.mode === 'cast'" type="button" class="btn btn-sm btn-primary" :disabled="dlna.busy || dlna.discovering || !dlna.deviceId" @click="castDlna()">{{ t('dlna.play') }}</button>
          <button v-else type="button" class="btn btn-sm btn-primary" :disabled="dlna.busy || !!dlna.sessionId" @click="shareDlna">{{ t('dlna.start_share') }}</button>
        </div>
      </div>
    </div>
  </ModalDialog>
</template>

<script setup lang="ts">
import { ref, watch, nextTick, onBeforeUnmount } from 'vue';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { useUIStore } from '@/stores/uiStore';
import { dlna, discoverDlna, castDlna, toggleDlnaPause, closeDlna, shareDlna, setDlnaMode, setDlnaScope, setDlnaAutoPlay, setDlnaInterval } from '@/common/dlna';
import ModalDialog from '@/components/ModalDialog.vue';

const { t } = useI18n(), ui = useUIStore();
const root = ref<HTMLElement | null>(null), width = Math.min(560, window.innerWidth - 24);
let previousFocus: HTMLElement | null = null;
function changeAutoPlay(event: Event) { setDlnaAutoPlay((event.target as HTMLInputElement).checked); }
function changeInterval(event: Event) { setDlnaInterval(Number((event.target as HTMLSelectElement).value)); }
function trapFocus(event: KeyboardEvent) {
  const controls = Array.from(root.value?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled)') || []);
  const first = controls[0], last = controls.at(-1);
  if (!first || !last) { event.preventDefault(); return; }
  if (event.shiftKey && (document.activeElement === first || document.activeElement === root.value)) {
    event.preventDefault(); last.focus();
  } else if (!event.shiftKey && (document.activeElement === last || document.activeElement === root.value)) {
    event.preventDefault(); first.focus();
  }
}
watch(() => dlna.visible, async visible => {
  if (visible) {
    previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    ui.pushInputHandler('Dlna');
    await nextTick();
    if (dlna.visible) root.value?.focus({ preventScroll: true });
  } else {
    ui.removeInputHandler('Dlna');
    previousFocus?.focus({ preventScroll: true });
  }
});
watch(() => libConfig._libraryId, libraryId => { if (dlna.visible && dlna.libraryId !== libraryId) closeDlna(); });
onBeforeUnmount(() => { closeDlna(); ui.removeInputHandler('Dlna'); });
</script>
