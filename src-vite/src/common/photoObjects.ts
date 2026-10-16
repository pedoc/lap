import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { config, libConfig } from '@/common/config';
export function usePhotoObjects(source: () => [number, unknown]) {
  const result = ref<any>(null),
    error = ref(''),
    revision = ref(0),
    loading = ref(false);
  let disposed = false,
    sequence = 0;
  const stops: Array<() => void> = [];
  async function refresh() {
    const [fileId] = source(),
      libraryId = libConfig._libraryId,
      profile = config.settings.ai?.detectionProfile,
      request = ++sequence;
    if (!(fileId > 0) || !libraryId) {
      result.value = null;
      loading.value = false;
      return;
    }
    loading.value = true;
    try {
      const data = await invoke('get_photo_objects', { libraryId, fileId });
      if (
        !disposed &&
        request === sequence &&
        libraryId === libConfig._libraryId &&
        profile === config.settings.ai?.detectionProfile
      ) {
        result.value = data;
        error.value = '';
      }
    } catch (e) {
      if (!disposed && request === sequence) {
        error.value = String(e);
        result.value = null;
      }
    } finally {
      if (!disposed && request === sequence) loading.value = false;
    }
  }
  watch(
    () => [
      libConfig._libraryId,
      ...source(),
      config.settings.ai?.detectionProfile,
    ],
    () => {
      result.value = null;
      error.value = '';
      revision.value++;
      void refresh();
    },
    { immediate: true },
  );
  onMounted(async () => {
    for (const name of [
      'object-index-updated',
      'object-annotations-changed',
      'ai-configuration-changed',
      'media-lifecycle-changed',
      'files-deleted',
    ]) {
      const off = await listen(name, (event: any) => {
        if (
          event.payload?.libraryId &&
          event.payload.libraryId !== libConfig._libraryId
        )
          return;
        if (
          event.payload?.fileId &&
          Number(event.payload.fileId) !== source()[0]
        )
          return;
        if (
          [
            'media-lifecycle-changed',
            'files-deleted',
            'ai-configuration-changed',
          ].includes(name)
        )
          result.value = null;
        revision.value++;
        void refresh();
      });
      if (disposed) off();
      else stops.push(off);
    }
  });
  onBeforeUnmount(() => {
    disposed = true;
    sequence++;
    stops.forEach((off) => off());
  });
  return { result, error, loading, revision, refresh };
}
