<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-600 bg-black/60">
      <section
        ref="dialog"
        role="dialog"
        aria-modal="true"
        :aria-label="name"
        tabindex="-1"
        class="absolute inset-4 flex flex-col gap-2 rounded-box bg-base-200 p-3 shadow-2xl outline-none"
      >
        <header class="flex items-center gap-2 flex-wrap">
          <h3 class="flex-1 min-w-0 break-all font-semibold">{{ name }}</h3>
          <span class="text-xs">{{ index + 1 }} / {{ total }}</span>
          <button
            class="btn btn-sm"
            :disabled="navigationBusy || index === 0"
            @click="$emit('previous')"
          >
            {{ $t('private_viewer.previous') }}
          </button>
          <button
            class="btn btn-sm"
            :disabled="navigationBusy || index + 1 >= total"
            @click="$emit('next')"
          >
            {{ $t('private_viewer.next') }}
          </button>
          <button class="btn btn-sm" @click="$emit('close')">
            {{ $t('private_viewer.close') }}
          </button>
        </header>
        <p class="text-xs text-base-content/60">
          {{ $t('private_viewer.readonly') }}
        </p>
        <div class="flex gap-2 items-center flex-wrap">
          <button
            class="btn btn-xs"
            :disabled="!media?.companionType && !companion"
            @click="companion = !companion"
          >
            {{
              $t(
                companion
                  ? 'private_viewer.primary'
                  : media?.companionType === 2
                    ? 'private_viewer.live_video'
                    : 'private_viewer.pair',
              )
            }}
          </button>
          <template v-if="media?.fileType === 3 && !companion">
            <label class="text-xs"
              >RAW
              <select class="select select-xs" v-model="rawMode">
                <option value="embedded">
                  {{ $t('private_viewer.embedded') }}
                </option>
                <option value="rendered">
                  {{ $t('private_viewer.rendered') }}
                </option>
              </select>
            </label>
            <label class="text-xs flex items-center gap-1"
              ><input
                type="checkbox"
                class="checkbox checkbox-xs"
                v-model="autoBright"
              />{{ $t('private_viewer.bright') }}</label
            >
          </template>
          <template v-if="media && media.fileType !== 2">
            <button class="btn btn-xs" @click="zoom(-1)">−</button>
            <span class="text-xs">{{ Math.round(scale * 100) }}%</span>
            <button class="btn btn-xs" @click="zoom(1)">+</button>
            <button class="btn btn-xs" @click="resetView">
              {{ $t('private_viewer.fit') }}
            </button>
          </template>
        </div>
        <p
          v-if="media?.warning"
          role="alert"
          class="text-xs text-warning break-all"
        >
          {{ media.warning }}
        </p>
        <div
          v-if="error"
          role="alert"
          class="text-xs text-error whitespace-pre-wrap break-all"
        >
          {{ $t('private_viewer.failed', { error }) }}
          <button class="link" @click="loadMedia">
            {{ $t('media_lifecycle.retry') }}
          </button>
        </div>
        <p
          v-if="error && previewUrl && !cacheBroken"
          class="text-xs text-warning"
        >
          {{ $t('private_viewer.cache_only') }}
        </p>
        <div
          class="relative flex-1 min-h-0 overflow-hidden bg-black/90 rounded-box flex items-center justify-center"
          @wheel.prevent="onWheel"
          @pointerdown="startDrag"
          @pointermove="drag"
          @pointerup="endDrag"
          @pointercancel="endDrag"
        >
          <span
            v-if="loading || navigationBusy"
            class="loading loading-spinner text-primary"
          ></span>
          <Video
            v-else-if="media?.fileType === 2"
            :key="media.ticket || media.name"
            :file-path="media.name || name"
            :private-ticket="media.ticket || ''"
            :private-url="media.streamUrl || ''"
            :rotate="media.rotate"
            :thumbnail-src="previewUrl"
            :read-only="true"
            :is-zoom-fit="true"
          />
          <img
            v-else-if="imageUrl"
            :key="imageUrl"
            :src="imageUrl"
            :alt="media?.name || name"
            draggable="false"
            class="max-w-full max-h-full object-contain select-none"
            :style="{
              transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale}) rotate(${media?.rotate || 0}deg)`,
            }"
            @error="imageFailed"
          />
          <p v-else-if="!loading" class="text-sm text-white/60">
            {{ $t('private_viewer.no_preview') }}
          </p>
        </div>
      </section>
    </div>
  </Teleport>
</template>
<script setup lang="ts">
import {
  computed,
  defineAsyncComponent,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from 'vue';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { readPrivateMedia } from '@/common/api';
import { convertFileSrc } from '@tauri-apps/api/core';
const Video = defineAsyncComponent(() => import('./Video.vue'));
const props = defineProps<{
  sessionToken: string | null;
  libraryId: string;
  fileId: number;
  mode: 'hidden' | 'deleted';
  groupId?: string;
  name: string;
  previewUrl: string;
  index: number;
  total: number;
  navigationBusy: boolean;
}>();
const emit = defineEmits(['previous', 'next', 'close']);
const { t } = useI18n();
const dialog = ref<HTMLElement | null>(null);
type PrivateMedia = {
  name: string;
  path: string;
  ticket?: string | null;
  streamUrl?: string | null;
  imageData?: string | null;
  fileType: number;
  rotate: number;
  companionType?: number | null;
  warning?: string | null;
  decodeError?: string | null;
};
const media = ref<PrivateMedia | null>(null);
const error = ref('');
const cacheBroken = ref(false);
const loading = ref(false);
const companion = ref(false);
const rawMode = ref('embedded');
const autoBright = ref(false);
const scale = ref(1);
const pan = ref({ x: 0, y: 0 });
let request = 0;
let disposed = false;
let dragging: { x: number; y: number; panX: number; panY: number } | null =
  null;
let previousFocus: HTMLElement | null = null;
const imageUrl = computed(() =>
  loading.value
    ? ''
    : error.value
      ? cacheBroken.value
        ? ''
        : props.previewUrl
      : media.value && media.value.fileType !== 2
        ? media.value.ticket
          ? convertFileSrc(media.value.ticket, 'private-media')
          : media.value.imageData || ''
        : '',
);
function resetView() {
  scale.value = 1;
  pan.value = { x: 0, y: 0 };
  dragging = null;
}
function zoom(direction: number) {
  scale.value = Math.max(
    1,
    Math.min(10, scale.value * (direction > 0 ? 1.25 : 0.8)),
  );
  if (scale.value === 1) resetView();
}
function onWheel(event: WheelEvent) {
  if (media.value?.fileType !== 2) zoom(event.deltaY < 0 ? 1 : -1);
}
function startDrag(event: PointerEvent) {
  if (event.button !== 0 || scale.value <= 1 || media.value?.fileType === 2)
    return;
  dragging = {
    x: event.clientX,
    y: event.clientY,
    panX: pan.value.x,
    panY: pan.value.y,
  };
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}
function drag(event: PointerEvent) {
  if (dragging)
    pan.value = {
      x: dragging.panX + event.clientX - dragging.x,
      y: dragging.panY + event.clientY - dragging.y,
    };
}
function endDrag() {
  dragging = null;
}
function imageFailed() {
  if (!error.value) error.value = t('private_viewer.local_decode_failed');
  else {
    cacheBroken.value = true;
    error.value = t('private_viewer.cache_failed');
  }
}
async function loadMedia() {
  const token = ++request;
  const library = props.libraryId;
  const file = props.fileId;
  const mode = props.mode;
  const group = props.groupId;
  const session = props.sessionToken;
  const isCurrent = () =>
    !disposed &&
    token === request &&
    library === libConfig._libraryId &&
    library === props.libraryId &&
    file === props.fileId &&
    mode === props.mode &&
    group === props.groupId &&
    session === props.sessionToken;
  media.value = null;
  error.value = '';
  cacheBroken.value = false;
  loading.value = true;
  resetView();
  try {
    const result = await readPrivateMedia(
      library,
      file,
      mode,
      group ?? null,
      companion.value,
      { mode: rawMode.value, autoBright: autoBright.value, preferPair: false },
      props.sessionToken,
    );
    if (isCurrent()) {
      media.value = result;
      error.value = result.decodeError || '';
    }
  } catch (reason) {
    if (isCurrent()) error.value = String(reason);
  } finally {
    if (!disposed && token === request) loading.value = false;
  }
}
function onKey(event: KeyboardEvent) {
  if (event.key === 'Tab' && dialog.value) {
    const focusable = Array.from(
      dialog.value.querySelectorAll<HTMLElement>(
        'button:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex="0"]',
      ),
    );
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (
      event.shiftKey &&
      (document.activeElement === first ||
        document.activeElement === dialog.value)
    ) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
    return;
  }
  if (event.key === 'Escape') {
    event.preventDefault();
    event.stopImmediatePropagation();
    emit('close');
    return;
  }
  const target = event.target as HTMLElement | null;
  if (
    target?.isContentEditable ||
    target?.closest('input, textarea, select, .video-js')
  )
    return;
  if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
    event.preventDefault();
    event.stopImmediatePropagation();
    if (props.navigationBusy) return;
    if (event.key === 'ArrowLeft' && props.index > 0) emit('previous');
    if (event.key === 'ArrowRight' && props.index + 1 < props.total)
      emit('next');
  } else if (event.key === '+' || event.key === '=' || event.key === '-') {
    event.preventDefault();
    event.stopImmediatePropagation();
    zoom(event.key === '-' ? -1 : 1);
  } else if (event.key === '0') {
    event.preventDefault();
    event.stopImmediatePropagation();
    resetView();
  }
}
watch(
  () => [
    props.libraryId,
    props.fileId,
    props.mode,
    props.groupId,
    props.sessionToken,
  ],
  () => {
    companion.value = false;
    rawMode.value = 'embedded';
    autoBright.value = false;
  },
  { immediate: true },
);
watch(
  () => [
    props.libraryId,
    props.fileId,
    props.mode,
    props.groupId,
    props.sessionToken,
    companion.value,
    rawMode.value,
    autoBright.value,
  ],
  () => {
    void loadMedia();
  },
  { immediate: true },
);
onMounted(async () => {
  previousFocus = document.activeElement as HTMLElement | null;
  document.addEventListener('keydown', onKey, true);
  await nextTick();
  if (!disposed) dialog.value?.focus();
});
onBeforeUnmount(() => {
  disposed = true;
  request++;
  media.value = null;
  document.removeEventListener('keydown', onKey, true);
  previousFocus?.focus();
});
</script>
