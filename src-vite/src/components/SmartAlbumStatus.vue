<template>
  <div
    v-if="semantic || ocr || objects || error || statusError"
    class="text-xs space-y-1 rounded-box bg-base-200 p-2"
  >
    <p v-if="error || statusError" role="alert" class="text-error break-all">
      {{ $t("smart_albums.error", { error: error || statusError }) }}
      <button class="link" @click="retry">
        {{ $t("smart_albums.retry") }}
      </button>
    </p>
    <template v-if="semantic && status">
      <p>
        {{ $t("smart_albums.status", status) }}
        <span v-if="status.pending" class="text-warning">{{
          $t("smart_albums.incomplete")
        }}</span>
      </p>
      <p v-if="job.total">{{ $t("smart_albums.index_progress", job) }}</p>
      <p v-if="job.error" class="text-warning break-all">{{ job.error }}</p>
      <button v-if="job.running" class="btn btn-xs" @click="stop">
        {{ $t("smart_albums.stop") }}
      </button>
      <button
        v-else-if="status.pending"
        class="btn btn-xs"
        @click="confirmAnalysis = true"
      >
        {{ $t("smart_albums.analyse") }}
      </button>
    </template>
    <SmartRecognitionStatus v-if="ocr" task="ocr" />
    <SmartRecognitionStatus v-if="objects" task="objects" />
    <MessageBox
      v-if="confirmAnalysis"
      :title="$t('smart_albums.analyse')"
      :message="$t('smart_albums.analyse_confirm')"
      :okText="$t('msgbox.ok')"
      :cancelText="$t('msgbox.cancel')"
      @ok="start"
      @cancel="confirmAnalysis = false"
    />
  </div>
</template>
<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import {
  getSmartAlbumStatus,
  startSmartAlbumIndex,
  stopSmartAlbumIndex,
} from "@/common/api";
import { hasSemanticRules, hasOcrRules, hasObjectRules } from "@/common/smartAlbums";
import { config, libConfig } from "@/common/config";
import MessageBox from "./MessageBox.vue";
import SmartRecognitionStatus from './SmartRecognitionStatus.vue';
const props = defineProps<{ params: any; error?: string }>();
const emit = defineEmits(["retry"]);
const semantic = computed(() => hasSemanticRules(props.params?.rules || []));
const ocr = computed(() => hasOcrRules(props.params?.rules || []));
const objects = computed(() => hasObjectRules(props.params?.rules || []));
const status = ref<any>(null);
const statusError = ref("");
const confirmAnalysis = ref(false);
const job = computed(() => status.value?.job || {});
let request = 0;
let disposed = false;
let timer: ReturnType<typeof setTimeout> | undefined;
const stops: (() => void)[] = [];
async function refresh() {
  const token = ++request;
  const library = libConfig._libraryId;
  const signature = JSON.stringify(props.params);
  if (!semantic.value) {
    status.value = null;
    return;
  }
  try {
    const next = await getSmartAlbumStatus(props.params);
    if (
      !disposed &&
      token === request &&
      library === libConfig._libraryId &&
      signature === JSON.stringify(props.params)
    ) {
      status.value = next;
      statusError.value = "";
    }
  } catch (error) {
    if (!disposed && token === request) {
      statusError.value = String(error);
      status.value = null;
    }
  }
}
function schedule() {
  clearTimeout(timer);
  timer = setTimeout(() => {
    void refresh();
  }, 500);
}
function retry() {
  void refresh();
  emit("retry");
}
async function start() {
  confirmAnalysis.value = false;
  try {
    await startSmartAlbumIndex(libConfig._libraryId);
    await refresh();
  } catch (error) {
    statusError.value = String(error);
  }
}
async function stop() {
  try {
    await stopSmartAlbumIndex(libConfig._libraryId, job.value.jobId);
  } catch (error) {
    statusError.value = String(error);
  }
}
watch(
  () => [
    JSON.stringify(props.params),
    libConfig._libraryId,
    config.settings.ai?.semanticProfile,
  ],
  () => {
    request++;
    status.value = null;
    statusError.value = "";
    schedule();
  },
  { immediate: true },
);
onMounted(async () => {
  for (const name of [
    "smart-album-index-progress",
    "semantic-index-changed",
    "index_progress",
    "index_finished",
  ]) {
    const stop = await listen(name, (event: any) => {
      const library = event.payload?.libraryId || event.payload?.library_id;
      if (library && library !== libConfig._libraryId) return;
      if (
        name === "smart-album-index-progress" &&
        status.value &&
        (!status.value.job?.running ||
          status.value.job?.jobId === event.payload.jobId)
      )
        status.value.job = event.payload;
      schedule();
    });
    if (disposed) {
      stop();
      return;
    }
    stops.push(stop);
  }
});
onBeforeUnmount(() => {
  disposed = true;
  request++;
  clearTimeout(timer);
  stops.forEach((stop) => stop());
});
</script>
