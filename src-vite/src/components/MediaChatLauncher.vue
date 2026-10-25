<template>
  <div ref="workspace" class="absolute inset-0 pointer-events-none z-30" data-media-chat-launcher>
    <button
      v-if="preferences.showFloatingButton"
      ref="button"
      type="button"
      class="btn btn-sm btn-circle absolute pointer-events-auto h-10 w-10 rounded-full border border-base-content/15 bg-base-100 p-0 text-base-content shadow-md hover:border-primary/40 hover:text-primary focus-visible:outline-2 focus-visible:outline-primary touch-none select-none"
      :class="dragging ? 'cursor-grabbing' : 'cursor-grab'"
      :style="{ left: `${position.x}px`, top: `${position.y}px`, visibility: ready ? 'visible' : 'hidden' }"
      :disabled="!props.libraryId"
      :aria-label="t('media_chat.open_chat')"
      :title="t('media_chat.open_chat')"
      data-media-chat-launcher-button
      @pointerdown.stop="startDrag"
      @lostpointercapture="cancelDrag"
      @click.stop="openChat"
    ><IconComment class="h-5 w-5 text-primary" aria-hidden="true" /></button>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { config } from '@/common/config';
import { IconComment } from '@/common/icons';
import { mediaChatFloatingReserve, openMediaChat } from '@/common/mediaChat';
import { getFloatingChatPosition, normalizeFloatingChatPosition, normalizeMediaChatSettings, type FloatingChatPoint } from '@/common/mediaChatPosition';

const props = defineProps<{ libraryId: string; fileIds: number[] }>();
const { t } = useI18n();
const workspace = ref<HTMLElement | null>(null), button = ref<HTMLButtonElement | null>(null);
const viewport = ref({ width: 0, height: 0 }), buttonSize = ref({ width: 0, height: 0 }), scale = ref(1);
const preferences = computed(() => normalizeMediaChatSettings(config.settings.mediaChat));
const margin = computed(() => 12 * scale.value);
const ready = computed(() => viewport.value.width > 0 && viewport.value.height > 0 && buttonSize.value.width > 0 && buttonSize.value.height > 0);
const draggedPosition = ref<FloatingChatPoint | null>(null), dragging = ref(false);
const position = computed(() => getFloatingChatPosition(draggedPosition.value
  ? { floatingPosition: normalizeFloatingChatPosition(draggedPosition.value, viewport.value, buttonSize.value, margin.value) }
  : preferences.value, viewport.value, buttonSize.value, margin.value));
let observer: ResizeObserver | undefined, disposed = false, suppressClick = false;
let drag: { pointerId: number; target: HTMLElement; startX: number; startY: number; start: FloatingChatPoint; moved: boolean } | undefined;

function updateReserve() {
  if (disposed || !preferences.value.showFloatingButton || !ready.value || !button.value) { mediaChatFloatingReserve.bottom = 0; return; }
  const rect = button.value.getBoundingClientRect();
  mediaChatFloatingReserve.bottom = rect.width > 0 && rect.height > 0
    && rect.right >= window.innerWidth - 360 * scale.value && rect.top + rect.height / 2 >= window.innerHeight / 2
    ? Math.max(0, window.innerHeight - rect.top + 12 * scale.value) : 0;
}
function measureLayout() {
  if (disposed || !workspace.value) return;
  const rect = workspace.value.getBoundingClientRect(), buttonRect = button.value?.getBoundingClientRect();
  const measuredScale = Number.parseFloat(getComputedStyle(document.documentElement).fontSize) / 16;
  const nextScale = Number.isFinite(measuredScale) && measuredScale > 0 ? measuredScale : 1;
  const nextViewport = { width: rect.width, height: Math.max(0, rect.height - (config.settings.showStatusBar ? 32 * nextScale : 0)) };
  const nextButton = buttonRect ? { width: buttonRect.width, height: buttonRect.height } : buttonSize.value;
  if (drag && (nextViewport.width !== viewport.value.width || nextViewport.height !== viewport.value.height
    || nextButton.width !== buttonSize.value.width || nextButton.height !== buttonSize.value.height || nextScale !== scale.value)) stopDrag(false);
  scale.value = nextScale;
  viewport.value = nextViewport;
  buttonSize.value = nextButton;
  void nextTick(updateReserve);
}
function moveDrag(event: PointerEvent) {
  if (!drag || event.pointerId !== drag.pointerId) return;
  const x = event.clientX - drag.startX, y = event.clientY - drag.startY;
  if (!drag.moved && Math.hypot(x, y) < 4 * scale.value) return;
  event.preventDefault();
  drag.moved = true;
  draggedPosition.value = { x: drag.start.x + x, y: drag.start.y + y };
}
function stopDrag(save: boolean, event?: PointerEvent | Event) {
  if (!drag || event && 'pointerId' in event && event.pointerId !== drag.pointerId) return;
  const previous = drag, finalPosition = position.value;
  drag = undefined;
  dragging.value = false;
  draggedPosition.value = null;
  suppressClick = previous.moved || !save;
  window.removeEventListener('pointermove', moveDrag);
  window.removeEventListener('pointerup', finishDrag);
  window.removeEventListener('pointercancel', cancelDrag);
  window.removeEventListener('blur', cancelDrag);
  if (previous.target.hasPointerCapture?.(previous.pointerId)) previous.target.releasePointerCapture(previous.pointerId);
  if (save && previous.moved) config.setMediaChat({ floatingPosition: normalizeFloatingChatPosition(finalPosition, viewport.value, buttonSize.value, margin.value) });
  void nextTick(updateReserve);
}
function finishDrag(event: PointerEvent) { stopDrag(true, event); }
function cancelDrag(event?: PointerEvent | Event) { stopDrag(false, event); }
function startDrag(event: PointerEvent) {
  if (event.button !== 0 || !event.isPrimary || !props.libraryId || !preferences.value.showFloatingButton) return;
  cancelDrag();
  measureLayout();
  if (!ready.value) return;
  suppressClick = false;
  event.preventDefault();
  const target = event.currentTarget as HTMLElement;
  target.focus({ preventScroll: true });
  drag = { pointerId: event.pointerId, target, startX: event.clientX, startY: event.clientY, start: { ...position.value }, moved: false };
  dragging.value = true;
  try { target.setPointerCapture(event.pointerId); } catch { /* Window listeners still finish a drag without pointer capture. */ }
  window.addEventListener('pointermove', moveDrag);
  window.addEventListener('pointerup', finishDrag);
  window.addEventListener('pointercancel', cancelDrag);
  window.addEventListener('blur', cancelDrag);
}
function openChat(event: MouseEvent) {
  if (suppressClick && event.detail !== 0) { suppressClick = false; event.preventDefault(); return; }
  suppressClick = false;
  if (preferences.value.showFloatingButton) openMediaChat(props.libraryId, props.fileIds);
}
watch(button, (element, previous) => {
  if (previous) observer?.unobserve(previous);
  if (element) observer?.observe(element);
  measureLayout();
}, { flush: 'post' });
watch(position, updateReserve, { flush: 'post' });
watch(() => config.settings.mediaChat, () => { cancelDrag(); if (!preferences.value.showFloatingButton) mediaChatFloatingReserve.bottom = 0; void nextTick(measureLayout); }, { deep: true });
watch(() => [config.settings.scale, config.settings.showStatusBar], () => { cancelDrag(); void nextTick(measureLayout); });
watch(() => props.libraryId, () => cancelDrag());
onMounted(() => {
  observer = new ResizeObserver(measureLayout);
  if (workspace.value) observer.observe(workspace.value);
  if (button.value) observer.observe(button.value);
  window.addEventListener('resize', measureLayout);
  window.addEventListener('scroll', updateReserve, true);
  measureLayout();
});
onBeforeUnmount(() => {
  disposed = true;
  cancelDrag();
  observer?.disconnect();
  window.removeEventListener('resize', measureLayout);
  window.removeEventListener('scroll', updateReserve, true);
  mediaChatFloatingReserve.bottom = 0;
});
</script>
