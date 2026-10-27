<template>
  <ModalDialog v-if="dlna.visible" :title="t('dlna.title')" :width="width" @cancel="closeDlna">
    <div ref="root" role="dialog" aria-modal="true" :aria-label="t('dlna.title')" tabindex="-1" class="flex flex-col gap-4 max-h-[75vh] overflow-y-auto p-1" @keydown.esc.stop="closeDlna" @keydown.tab="trapFocus">
      <p class="text-sm opacity-70">{{ t('dlna.intro') }}</p>
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
      <div class="rounded-box bg-base-200 p-3 space-y-2" aria-live="polite">
        <p class="text-xs opacity-70">{{ t('dlna.counter', { current: dlna.index + 1, total: dlna.fileIds.length }) }}</p>
        <p v-if="dlna.busy" class="text-sm flex items-center gap-2"><span class="loading loading-spinner loading-xs" aria-hidden="true" />{{ t('dlna.sending') }}</p>
        <template v-else-if="dlna.title"><p class="text-sm font-medium break-words">{{ dlna.title }}</p><p class="text-xs opacity-70">{{ dlna.paused ? t('dlna.paused') : t('dlna.sent') }}</p></template>
        <p v-else class="text-sm">{{ t('dlna.ready') }}</p>
      </div>
      <p v-if="dlna.error" class="text-sm text-error break-words" role="alert">{{ dlna.error }}</p>
      <p class="text-xs opacity-60">{{ t('dlna.formats') }}</p>
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex flex-wrap gap-2">
          <button type="button" class="btn btn-sm" :disabled="dlna.busy || !dlna.sessionId || dlna.index === 0" @click="castDlna(dlna.index - 1)">{{ t('dlna.previous') }}</button>
          <button v-if="dlna.kind === 'video'" type="button" class="btn btn-sm" :disabled="dlna.busy" @click="toggleDlnaPause">{{ dlna.paused ? t('dlna.resume') : t('dlna.pause') }}</button>
          <button type="button" class="btn btn-sm" :disabled="dlna.busy || !dlna.sessionId || dlna.index >= dlna.fileIds.length - 1" @click="castDlna(dlna.index + 1)">{{ t('dlna.next') }}</button>
        </div>
        <div class="flex gap-2">
          <button type="button" class="btn btn-sm" @click="closeDlna">{{ dlna.sessionId || dlna.busy ? t('dlna.stop') : t('dlna.close') }}</button>
          <button type="button" class="btn btn-sm btn-primary" :disabled="dlna.busy || dlna.discovering || !dlna.deviceId" @click="castDlna()">{{ t('dlna.play') }}</button>
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
import { dlna, discoverDlna, castDlna, toggleDlnaPause, closeDlna } from '@/common/dlna';
import ModalDialog from '@/components/ModalDialog.vue';

const { t } = useI18n(), ui = useUIStore();
const root = ref<HTMLElement | null>(null), width = Math.min(560, window.innerWidth - 24);
let previousFocus: HTMLElement | null = null;
function trapFocus(event: KeyboardEvent) {
  const controls = Array.from(root.value?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)') || []);
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
