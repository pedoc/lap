import { computed, shallowRef } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { libConfig } from './config';
import { createMediaJobTracker, type MediaJobSummary } from './mediaJobTracker';
export const mediaJobSummaries = shallowRef<MediaJobSummary[]>([]);
export const mediaJobDetailsRequest = shallowRef<string | null>(null);
const reader = shallowRef<
  | null
  | (() => { libraryId: string; canRead: boolean; sessionToken: string | null })
>(null);
export const canReadMediaJobDetails = computed(() => {
  const context = reader.value?.();
  return !!context?.canRead && context.libraryId === libConfig._libraryId;
});
export function bindMediaJobPrivateContext(
  read: NonNullable<typeof reader.value>,
) {
  reader.value = read;
  return () => {
    if (reader.value === read) {
      reader.value = null;
      mediaJobDetailsRequest.value = null;
    }
  };
}
export function mediaJobSession() {
  return canReadMediaJobDetails.value
    ? reader.value?.().sessionToken || null
    : null;
}
export const mediaJobs = createMediaJobTracker({
  library: () => libConfig._libraryId,
  invoke,
  listen,
  uuid: () => crypto.randomUUID(),
  changed: (rows) => {
    mediaJobSummaries.value = rows;
  },
});
export function startMediaJob(
  action: string,
  data: any = {},
  sessionToken: string | null = null,
) {
  return mediaJobs.start({ action, ...data }, sessionToken);
}
export function getMediaJobFailures(
  jobId: string,
  sessionToken: string | null,
) {
  return invoke<any[]>('get_media_job_failures', {
    libraryId: libConfig._libraryId,
    jobId,
    sessionToken,
  });
}
