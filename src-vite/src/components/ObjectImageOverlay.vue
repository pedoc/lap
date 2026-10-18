<template>
  <template v-if="editing">
    <div class="absolute inset-0 pointer-events-none overflow-hidden">
      <div
        v-if="compatible"
        class="absolute"
        :style="{
          width: `${width}px`,
          height: `${height}px`,
          transform: `translate(${x}px,${y}px) scale(${zoom}) rotate(${rotate}deg)`,
        }"
      >
        <svg
          ref="editSvg"
          :width="width"
          :height="height"
          :viewBox="`0 0 ${width} ${height}`"
          class="absolute inset-0 top-0 left-0"
          :class="editing && editing.drawing ? 'pointer-events-auto cursor-crosshair' : 'pointer-events-none'"
          :aria-label="t('objects.draw_box')"
          @pointerdown.stop.prevent="beginDraw"
          @pointermove.stop.prevent="moveDraw"
          @pointerup.stop.prevent="finishDraw"
          @pointercancel="cancelDraw"
          @keydown.escape.stop.prevent="cancelDraw"
        >
          <rect
            v-if="drawRect"
            :x="drawRect[0] * width"
            :y="drawRect[1] * height"
            :width="drawRect[2] * width"
            :height="drawRect[3] * height"
            fill="currentColor"
            fill-opacity="0.12"
            stroke="currentColor"
            stroke-width="2"
            vector-effect="non-scaling-stroke"
            class="text-primary"
          />
        </svg>
        <div
          v-for="(box, index) in editing.boxes"
          :key="box.id"
          class="absolute"
          :class="editing.drawing ? 'pointer-events-none' : 'pointer-events-auto'"
          :style="editBoxStyle(box.bounds, index, selected(box.id))"
          @mousedown.stop.prevent="beginBox($event, box)"
        >
          <span
            class="absolute left-0 top-0 origin-top-left bg-base-100/95 text-base-content rounded px-1 py-0.5 text-xs whitespace-nowrap shadow"
            :style="{ transform: `scale(${1 / Math.max(0.01, zoom)})` }"
            >{{ index + 1 }} · {{ name(box.class) }}</span
          >
          <template v-if="selected(box.id)">
            <button
              v-for="(corner, ci) in cornerHandles"
              :key="ci"
              class="absolute rounded-full bg-primary border-2 border-base-100 w-2.5 h-2.5 -translate-x-1/2 -translate-y-1/2"
              :style="handleStyle(corner)"
              @mousedown.stop.prevent="beginBox($event, box, corner)"
              @dblclick.stop
            />
            <div
              v-if="editing.selected.length === 1"
              class="absolute left-0 bottom-0 translate-y-full pt-1 flex items-center gap-1 origin-top-left"
              :style="{ transform: `translateY(100%) scale(${1 / Math.max(0.01, zoom)})` }"
              @mousedown.stop
              @dblclick.stop
            >
              <select
                :value="box.class.key"
                class="select select-xs bg-base-100/95 shadow"
                :aria-label="t('objects.set_class')"
                :disabled="editing.busy || editing.stale"
                @change="
                  setBoxClass(box, ($event.target as HTMLSelectElement).value)
                "
              >
                <option
                  v-for="choice in editing.choices"
                  :key="choice.key"
                  :value="choice.key"
                >
                  {{ name(choice) }}
                </option>
              </select>
              <label class="btn btn-xs bg-base-100/95 shadow"
                ><input
                  type="checkbox"
                  class="checkbox checkbox-xs"
                  :checked="box.reviewed"
                  :disabled="editing.busy || editing.stale"
                  @change="
                    box.reviewed = ($event.target as HTMLInputElement).checked
                  "
                />{{ t('objects.confirm') }}</label
              >
              <button
                class="btn btn-xs btn-ghost text-error bg-base-100/95 shadow"
                :disabled="editing.busy || editing.stale"
                @click.stop="removeBox(box)"
              >
                {{ t('objects.delete_box') }}
              </button>
            </div>
          </template>
        </div>
      </div>
      <p
        v-else
        class="absolute bottom-3 left-3 z-40 max-w-72 rounded-box bg-base-100/95 border border-base-content/15 p-2 text-xs text-warning"
      >
        {{ t('objects.geometry') }}
      </p>
      <div
        class="absolute top-3 left-3 z-40 flex flex-wrap items-center gap-1 pointer-events-auto"
        @mousedown.stop
        @dblclick.stop
      >
        <button
          class="btn btn-sm bg-base-100/90 border-base-content/15 shadow-sm"
          :disabled="editing.busy"
          @click.stop="cancelObjectEdit"
        >
          {{ t('objects.cancel') }}
        </button>
        <button
          class="btn btn-sm btn-primary bg-base-100/90"
          :disabled="editing.busy || editing.stale"
          @click.stop="saveObjectEdit"
        >
          {{ t('objects.save') }}
        </button>
        <button
          class="btn btn-sm bg-base-100/90 border-base-content/15 shadow-sm"
          :class="editing.drawing ? 'text-primary' : ''"
          :aria-pressed="editing.drawing"
          :disabled="editing.busy || editing.stale"
          @click.stop="editing.drawing = !editing.drawing"
        >
          {{ t('objects.draw_box') }}
        </button>
        <select
          v-model="draftClass"
          class="select select-sm bg-base-100/90 border-base-content/15"
          :aria-label="t('objects.set_class')"
          :disabled="editing.busy || editing.stale"
        >
          <option v-for="choice in editing.choices" :key="choice.key" :value="choice.key">
            {{ name(choice) }}
          </option>
        </select>
        <template v-if="editing.selected.length">
          <button
            class="btn btn-sm bg-base-100/90 border-base-content/15 shadow-sm"
            @click.stop="applyClassToSelected"
          >
            {{ t('objects.apply_class') }}
          </button>
          <button
            class="btn btn-sm bg-base-100/90 border-base-content/15 shadow-sm"
            @click.stop="confirmSelected"
          >
            {{ t('objects.confirm') }}
          </button>
          <button
            class="btn btn-sm btn-ghost text-error bg-base-100/90"
            @click.stop="removeSelected"
          >
            {{ t('objects.delete_box') }} ({{ editing.selected.length }})
          </button>
        </template>
        <span v-else class="text-xs px-1 opacity-70">{{
          t('objects.select_hint')
        }}</span>
      </div>
      <p
        v-if="editing.drawing"
        class="absolute bottom-3 left-3 z-40 rounded-box bg-base-100/90 px-3 py-2 text-xs"
      >
        {{ t('objects.draw_hint') }}
      </p>
      <p
        v-if="editing.error || editing.stale"
        class="absolute bottom-3 right-3 z-40 max-w-sm rounded-box bg-base-100/95 border border-base-content/15 p-2 text-xs text-warning whitespace-pre-wrap break-words"
        role="alert"
      >
        {{ editing.stale ? t('objects.stale') : editing.error }}
      </p>
    </div>
  </template>
  <template v-else-if="ready && document?.objects?.length">
    <div
      v-if="showObjectBoxes && compatible"
      class="absolute inset-0 pointer-events-none overflow-hidden"
    >
      <div
        class="absolute"
        :style="{
          width: `${width}px`,
          height: `${height}px`,
          transform: `translate(${x}px,${y}px) scale(${zoom}) rotate(${rotate}deg)`,
          transition: transition ? 'transform 0.2s ease-out' : 'none',
        }"
      >
        <button
          v-for="(object, index) in document.objects"
          :key="object.id"
          class="absolute pointer-events-auto focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
          :style="
            objectBoxStyle(
              object.bounds,
              width,
              height,
              zoom,
              active?.objectId === object.id,
              index,
            )
          "
          :title="name(object.class)"
          :aria-label="name(object.class)"
          @mousedown.stop
          @dblclick.stop
          @click.stop="focus(object)"
        >
          <span
            class="absolute left-0 top-0 origin-top-left bg-base-100/95 text-base-content rounded px-1 py-0.5 text-xs whitespace-nowrap shadow"
            :style="{ transform: `scale(${1 / Math.max(0.01, zoom)})` }"
            >{{ index + 1 }} · {{ name(object.class) }}</span
          >
        </button>
      </div>
    </div>
    <div
      class="absolute top-14 left-3 z-30 flex flex-col items-start gap-1"
      @mousedown.stop
      @dblclick.stop
    >
      <button
        class="btn btn-sm bg-base-100/90 border-base-content/15 shadow-sm"
        :class="showObjectBoxes ? 'text-primary' : ''"
        :aria-pressed="showObjectBoxes"
        @click.stop="showObjectBoxes = !showObjectBoxes"
      >
        {{ t('objects.boxes') }} · {{ document.objects.length }}
      </button>
      <p
        v-if="showObjectBoxes && !compatible"
        class="max-w-60 rounded-box bg-base-100/90 p-2 text-xs text-warning"
      >
        {{ t('objects.geometry') }}
      </p>
    </div>
  </template>
</template>
<script setup lang="ts">
import { ref, computed, watch, onBeforeUnmount } from 'vue';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { objectFocus, showObjectBoxes } from '@/common/objects';
import { usePhotoObjects } from '@/common/photoObjects';
import {
  objectBoxStyle,
  objectGeometryCompatible,
  objectClassName,
} from '@/common/objectUi';
import {
  objectEdit,
  objectEditActive,
  cancelObjectEdit,
  saveObjectEdit,
} from '@/common/objectEditing';
const props = defineProps<{
  fileId?: number;
  fileVersion?: number;
  width: number;
  height: number;
  x: number;
  y: number;
  zoom: number;
  rotate: number;
  ready: boolean;
  transition: boolean;
}>();
const emit = defineEmits<{ locate: [bounds: number[]] }>(),
  { t } = useI18n();
const { result } = usePhotoObjects(() => [
  Number(props.fileId),
  props.fileVersion,
]);
const document = computed(() => result.value?.document),
  compatible = computed(() =>
    objectGeometryCompatible(props.width, props.height, document.value),
  ),
  editing = computed(() =>
    objectEditActive(Number(props.fileId)) ? objectEdit.value : null,
  );
const name = (label: any) => objectClassName(label, t);
const active = computed(() =>
  objectFocus.value?.libraryId === libConfig._libraryId &&
  objectFocus.value?.fileId === props.fileId &&
  objectFocus.value?.profile === config.settings.ai?.detectionProfile
    ? objectFocus.value
    : null,
);
const cornerHandles = ['nw', 'ne', 'sw', 'se'] as const;
const draftClass = ref('');
const drawRect = ref<[number, number, number, number] | null>(null);
const editSvg = ref<SVGSVGElement | null>(null);
let gesture:
  | {
      type: 'move' | 'resize';
      id: string;
      start: { x: number; y: number };
      original: [number, number, number, number];
      anchor: { x: number; y: number };
    }
  | undefined;

function selected(id: string) {
  return editing.value?.selected.includes(id) || false;
}
function boxById(id: string) {
  return editing.value?.boxes.find((box) => box.id === id);
}
function clampBounds(bounds: [number, number, number, number]) {
  const min = 0.02;
  let [x, y, w, h] = bounds;
  w = Math.max(min, Math.min(1, w));
  h = Math.max(min, Math.min(1, h));
  x = Math.max(0, Math.min(1 - w, x));
  y = Math.max(0, Math.min(1 - h, y));
  return [x, y, w, h] as [number, number, number, number];
}
function point(event: PointerEvent) {
  const matrix = editSvg.value?.getScreenCTM();
  if (!matrix || !(props.width > 0 && props.height > 0)) return null;
  const local = new DOMPoint(event.clientX, event.clientY).matrixTransform(
    matrix.inverse(),
  );
  return {
    x: Math.max(0, Math.min(1, local.x / props.width)),
    y: Math.max(0, Math.min(1, local.y / props.height)),
  };
}
function cornerOf(corner: string, bounds: number[]) {
  const [x, y, w, h] = bounds;
  if (corner === 'nw') return { x, y };
  if (corner === 'ne') return { x: x + w, y };
  if (corner === 'sw') return { x, y: y + h };
  return { x: x + w, y: y + h };
}
function editBoxStyle(
  bounds: number[],
  index: number,
  isSelected: boolean,
) {
  if (
    !Array.isArray(bounds) ||
    bounds.length !== 4 ||
    !bounds.every(Number.isFinite) ||
    bounds[2] <= 0 ||
    bounds[3] <= 0
  )
    return null;
  const [x, y, w, h] = bounds,
    scale = Number.isFinite(props.zoom) && props.zoom > 0 ? props.zoom : 1;
  return {
    left: `${Math.max(0, x) * props.width}px`,
    top: `${Math.max(0, y) * props.height}px`,
    width: `${Math.min(1, x + w) * props.width - Math.max(0, x) * props.width}px`,
    height: `${Math.min(1, y + h) * props.height - Math.max(0, y) * props.height}px`,
    border: `${(isSelected ? 3 : 1.5) / scale}px solid ${isSelected ? '#f59e0b' : '#0e7490'}`,
    boxSizing: 'border-box' as const,
    boxShadow: `0 0 0 ${1 / scale}px #f8fafc, 0 0 0 ${2 / scale}px #0f172a`,
    background: isSelected ? '#f59e0b18' : '#0e749012',
    cursor: 'move',
  };
}
function handleStyle(corner: string) {
  return {
    left: corner.endsWith('w') ? '0%' : '100%',
    top: corner.startsWith('n') ? '0%' : '100%',
    cursor: corner === 'nw' || corner === 'se' ? 'nwse-resize' : 'nesw-resize',
  };
}
function beginBox(
  event: PointerEvent,
  box: { id: string; bounds: [number, number, number, number] },
  corner?: string,
) {
  const session = editing.value;
  if (!session || session.busy || session.stale || session.drawing || event.button !== 0) return;
  const p = point(event);
  if (!p) return;
  const id = box.id;
  if (!event.shiftKey && !event.ctrlKey && !event.metaKey) {
    if (!session.selected.includes(id)) session.selected = [id];
  } else if (session.selected.includes(id)) {
    session.selected = session.selected.filter((value) => value !== id);
    if (corner) return;
  } else {
    session.selected = [...session.selected, id];
  }
  if (corner) {
    const anchor = cornerOf(
      corner === 'nw'
        ? 'se'
        : corner === 'ne'
          ? 'sw'
          : corner === 'sw'
            ? 'ne'
            : 'nw',
      box.bounds,
    );
    gesture = {
      type: 'resize',
      id,
      start: p,
      original: [...box.bounds],
      anchor,
    };
  } else {
    gesture = {
      type: 'move',
      id,
      start: p,
      original: [...box.bounds],
      anchor: { x: 0, y: 0 },
    };
  }
  window.addEventListener('pointermove', onWindowMove);
  window.addEventListener('pointerup', onWindowUp);
}
function onWindowMove(event: PointerEvent) {
  if (!gesture || editing.value?.busy || editing.value?.stale) return;
  const box = boxById(gesture.id);
  const p = point(event);
  if (!box || !p) return;
  if (gesture.type === 'move') {
    const dx = p.x - gesture.start.x,
      dy = p.y - gesture.start.y;
    box.bounds = clampBounds([
      gesture.original[0] + dx,
      gesture.original[1] + dy,
      gesture.original[2],
      gesture.original[3],
    ]);
  } else {
    const l = Math.min(gesture.anchor.x, p.x),
      t = Math.min(gesture.anchor.y, p.y),
      r = Math.max(gesture.anchor.x, p.x),
      b = Math.max(gesture.anchor.y, p.y);
    box.bounds = clampBounds([l, t, r - l, b - t]);
  }
}
function onWindowUp() {
  if (typeof window !== 'undefined') {
    window.removeEventListener('pointermove', onWindowMove);
    window.removeEventListener('pointerup', onWindowUp);
  }
  gesture = undefined;
}
function beginDraw(event: PointerEvent) {
  const session = editing.value;
  if (!session?.drawing || session.busy || session.stale || event.button !== 0) return;
  const p = point(event);
  if (!p) return;
  drawRect.value = null;
  gesture = {
    type: 'resize',
    id: '__draw__',
    start: p,
    original: [p.x, p.y, 0, 0],
    anchor: p,
  };
  editSvg.value?.setPointerCapture(event.pointerId);
}
function moveDraw(event: PointerEvent) {
  if (!editing.value?.drawing || !gesture || gesture.id !== '__draw__') return;
  const p = point(event);
  if (!p) return;
  const l = Math.min(gesture.anchor.x, p.x),
    t = Math.min(gesture.anchor.y, p.y),
    r = Math.max(gesture.anchor.x, p.x),
    b = Math.max(gesture.anchor.y, p.y);
  drawRect.value = [l, t, r - l, b - t];
}
function finishDraw(event: PointerEvent) {
  if (!editing.value?.drawing || !gesture || gesture.id !== '__draw__') return;
  moveDraw(event);
  gesture = undefined;
  if (editSvg.value?.hasPointerCapture(event.pointerId))
    editSvg.value.releasePointerCapture(event.pointerId);
  const session = editing.value;
  const rect = drawRect.value;
  drawRect.value = null;
  if (!session || !rect || rect[2] * props.width < 4 || rect[3] * props.height < 4) {
    if (session) session.error = t('objects.draw_too_small');
    return;
  }
  const choice = session.choices.find((c) => c.key === draftClass.value) || session.choices[0];
  if (!choice) return;
  session.boxes.push({
    id: `manual-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
    class: { ...choice },
    bounds: rect,
    score: null,
    reviewed: true,
  });
  session.selected = [
    session.boxes[session.boxes.length - 1].id,
  ];
}
function cancelDraw() {
  drawRect.value = null;
  gesture = undefined;
}
function setBoxClass(box: { class: { key: string; name: string; sourceId: string } }, key: string) {
  const session = editing.value;
  if (!session || session.busy || session.stale) return;
  const choice = session?.choices.find((c) => c.key === key);
  if (!choice) return;
  if (choice.key !== box.class.key) {
    box.class = { ...choice };
    box.score = null;
  }
}
function applyClassToSelected() {
  const session = editing.value;
  const choice = session?.choices.find((c) => c.key === draftClass.value) || session?.choices[0];
  if (!session || session.busy || session.stale || !choice) return;
  for (const box of session.boxes.filter((b) => session.selected.includes(b.id))) {
    if (choice.key !== box.class.key) {
      box.class = { ...choice };
      box.score = null;
    }
  }
}
function confirmSelected() {
  const session = editing.value;
  if (!session || session.busy || session.stale) return;
  for (const box of session.boxes.filter((b) => session.selected.includes(b.id)))
    box.reviewed = true;
}
function removeBox(box: { id: string }) {
  const session = editing.value;
  if (!session || session.busy || session.stale) return;
  session.boxes = session.boxes.filter((b) => b.id !== box.id);
  session.selected = session.selected.filter((id) => id !== box.id);
}
function removeSelected() {
  const session = editing.value;
  if (!session || session.busy || session.stale) return;
  const ids = new Set(session.selected);
  session.boxes = session.boxes.filter((b) => !ids.has(b.id));
  session.selected = [];
}
watch(
  [editing, () => props.ready],
  () => {
    onWindowUp();
    drawRect.value = null;
    const session = editing.value;
    if (session && !session.choices.some((c) => c.key === draftClass.value))
      draftClass.value = session.choices[0]?.key || '';
  },
  { immediate: true },
);
onBeforeUnmount(onWindowUp);
function focus(object: any) {
  showObjectBoxes.value = true;
  objectFocus.value = {
    libraryId: libConfig._libraryId,
    fileId: Number(props.fileId),
    profile: config.settings.ai?.detectionProfile || '',
    objectId: object.id,
  };
}
let lastFocus: any = null;
watch([active, document, () => props.ready, compatible], () => {
  if (
    !active.value ||
    active.value === lastFocus ||
    !props.ready ||
    !compatible.value
  )
    return;
  const bounds = document.value?.objects.find(
    (o: any) => o.id === active.value?.objectId,
  )?.bounds;
  if (bounds) {
    lastFocus = active.value;
    emit('locate', bounds);
  }
});
</script>
