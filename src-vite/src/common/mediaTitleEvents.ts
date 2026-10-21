import { onMounted, onBeforeUnmount } from 'vue';
import { listen } from '@tauri-apps/api/event';
import type { MediaTitleChange } from './mediaTitle';

/** One listener per owning view, never one listener per thumbnail. */
export function useMediaTitleEvents(apply: (change: MediaTitleChange) => void) {
  let disposed = false, stop: (() => void) | undefined;
  onMounted(async () => {
    const off = await listen<MediaTitleChange>('media-title-changed', event => {
      if (!disposed) apply(event.payload);
    });
    if (disposed) off(); else stop = off;
  });
  onBeforeUnmount(() => { disposed = true; stop?.(); });
}
