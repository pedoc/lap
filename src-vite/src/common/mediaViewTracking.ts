import { onMounted, onBeforeUnmount, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { applyMediaViewChange, createMediaViewTracker, type MediaViewChange, type ViewedMedia } from './mediaViews';
import { libConfig } from './config';

export function useMediaViewTracking(file: () => ViewedMedia | null | undefined, active: () => boolean, ready: () => boolean) {
  const focused = ref(false), visible = ref(false);
  let disposed = false;
  const tracker = createMediaViewTracker(
    (libraryId, fileId) => invoke<MediaViewChange>('record_media_view', { libraryId, fileId }),
    change => { applyMediaViewChange(file(), libConfig._libraryId, change); },
    error => { if (!disposed) console.warn('Failed to record media view:', error); },
  );
  const update = () => { focused.value = document.hasFocus(); visible.value = !document.hidden; };
  watch(() => [libConfig._libraryId, file()?.id, active(), ready(), focused.value, visible.value], () => {
    tracker.update({ library: libConfig._libraryId, fileId: Number(file()?.id || 0),
      active: active() && focused.value && visible.value, ready: ready() });
  }, { immediate: true });
  onMounted(() => {
    update(); window.addEventListener('focus', update); window.addEventListener('blur', update);
    document.addEventListener('visibilitychange', update);
  });
  onBeforeUnmount(() => {
    disposed = true; tracker.dispose(); window.removeEventListener('focus', update); window.removeEventListener('blur', update);
    document.removeEventListener('visibilitychange', update);
  });
}

/** Owners update their loaded rows in place; thumbnails never subscribe individually. */
export function useMediaViewEvents(apply: (change: MediaViewChange) => void) {
  let disposed = false, stop: (() => void) | undefined;
  onMounted(async () => {
    const off = await listen<MediaViewChange>('media-view-changed', event => { if (!disposed) apply(event.payload); });
    if (disposed) off(); else stop = off;
  });
  onBeforeUnmount(() => { disposed = true; stop?.(); });
}
