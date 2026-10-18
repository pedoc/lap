import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { config, libConfig } from '@/common/config';
import { objectEdit, markObjectEditStale } from './objectEditing';
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
      const data = await invoke<any>('get_photo_objects', { libraryId, fileId });
      if (
        !disposed &&
        request === sequence &&
        libraryId === libConfig._libraryId &&
        profile === config.settings.ai?.detectionProfile
      ) {
        const session = objectEdit.value;
        if (
          session?.libraryId === libraryId && session.fileId === fileId &&
          (session.profile !== data.profile || session.sourceKey !== data.sourceKey ||
            session.expectedRevision !== (data.annotation?.revision || ''))
        ) markObjectEditStale(fileId, libraryId);
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
    (_value, previous) => {
      if (previous) markObjectEditStale(Number(previous[1]), String(previous[0]));
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
        const eventLibrary = event.payload?.libraryId || event.payload?.library_id;
        const eventFile = event.payload?.fileId ?? event.payload?.file_id;
        if (
          eventLibrary &&
          eventLibrary !== libConfig._libraryId
        )
          return;
        if (
          eventFile &&
          Number(eventFile) !== source()[0]
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
        markObjectEditStale(source()[0], libConfig._libraryId);
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
