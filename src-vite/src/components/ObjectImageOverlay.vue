<template>
  <template v-if="editing">
    <div
      ref="editLayer"
      class="absolute inset-0 pointer-events-none overflow-hidden"
      @keydown.escape.stop.prevent="!editing.busy && cancelObjectEdit()"
    >
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
          v-for="({ object: box, index }) in visibleEditBoxes"
          :key="box.id"
          :data-object-box="box.id"
          class="absolute"
          :class="editing.drawing ? 'pointer-events-none' : 'pointer-events-auto'"
          :style="[editBoxStyle(box.bounds, index, selected(box.id)), box.ignored ? { opacity: 0.55, borderStyle: 'dashed' } : {}]"
          @mousedown.stop.prevent="beginBox($event, box)"
        >
          <button
            type="button"
            data-object-label
            class="absolute left-0 top-0 origin-top-left bg-base-100/95 text-base-content rounded px-1 py-0.5 text-xs whitespace-nowrap shadow focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
            :style="{ transform: `scale(${1 / Math.max(0.01, zoom)})` }"
            :disabled="!canEdit"
            @mousedown.stop
            @dblclick.stop
            @click.stop="selectBox(box.id, $event)"
            >{{ index + 1 }} · {{ name(box.class) }} <span aria-hidden="true">✎</span></button
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
          </template>
        </div>
      </div>
      <p
        v-else
        class="absolute bottom-3 left-3 z-40 max-w-72 rounded-box bg-base-100/95 border border-base-content/15 p-2 text-xs text-warning"
      >
        {{ t('objects.geometry') }}
      </p>
      <section
        v-if="inlineBox"
        ref="inlinePanel"
        data-object-correction
        role="dialog"
        :aria-label="t('objects.correct_box_title', { name: name(inlineBox.class) })"
        :aria-busy="editing.busy"
        class="absolute z-50 pointer-events-auto rounded-box border border-base-content/20 bg-base-100 text-base-content p-3 shadow-xl overflow-auto space-y-2 text-sm cursor-default"
        :style="inlinePosition || { visibility: 'hidden' }"
        @pointerdown.stop
        @mousedown.stop
        @click.stop
        @dblclick.stop
        @wheel.stop
        @keydown.stop
        @keydown.escape.stop.prevent="!editing.busy && cancelObjectEdit()"
      >
        <p class="font-medium">{{ t('objects.correct_box_title', { name: name(inlineBox.class) }) }}</p>
        <select v-if="currentClassKey" v-model="boxScope" data-object-box-scope class="select select-sm w-full" :aria-label="t('objects.box_scope')" :disabled="editing.busy" @keydown.stop>
          <option value="current">{{ t('objects.boxes_only_class', { name: currentClassName }) }}</option>
          <option value="all">{{ t('objects.boxes_all_classes') }}</option>
        </select>
        <p v-if="currentClassKey && boxScope==='current'" class="text-xs text-base-content/70">{{ t('objects.boxes_edit_scope_hint') }}</p>
        <label class="block text-xs">
          {{ t('objects.set_class') }}
          <select
            ref="classSelect"
            :value="inlineBox.class.key"
            class="select select-sm w-full mt-1 focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary disabled:opacity-50 disabled:cursor-not-allowed"
            :disabled="!canEdit || inlineBox.ignored"
            @change="setBoxClass(inlineBox, ($event.target as HTMLSelectElement).value)"
          >
            <option v-for="choice in editing.choices" :key="choice.key" :value="choice.key">
              {{ name(choice) }}
            </option>
          </select>
        </label>
        <div class="flex flex-wrap items-center justify-between gap-2">
          <label class="flex items-center gap-1 text-xs">
            <input v-model="inlineBox.reviewed" type="checkbox" class="checkbox checkbox-xs" :disabled="!canEdit || inlineBox.ignored" />
            {{ t('objects.confirm') }}
          </label>
          <button type="button" class="btn btn-xs btn-ghost text-error" :disabled="!canEdit" :aria-pressed="!!inlineBox.ignored" @click="toggleIgnored(inlineBox)">
            {{ t(inlineBox.ignored ? 'objects.undo_ignore' : 'objects.ignore') }}
          </button>
        </div>
        <p v-if="inlineBox.ignored" role="status" class="text-xs text-warning">{{ t('objects.ignore_pending') }}</p>
        <p class="text-xs opacity-60">{{ t('objects.edit_hint') }}</p>
        <p v-if="editing.error || editing.stale" role="alert" class="text-xs text-warning whitespace-pre-wrap break-words">
          {{ editing.stale ? t('objects.stale') : editing.error }}
        </p>
        <div class="flex flex-wrap items-center gap-1">
          <button type="button" class="btn btn-xs" :disabled="!canEdit" @click="startDrawing">{{ t('objects.draw_box') }}</button>
          <div class="flex-1" />
          <button type="button" class="btn btn-xs" :disabled="editing.busy" @click="cancelObjectEdit">{{ t('objects.cancel') }}</button>
          <button type="button" class="btn btn-xs btn-primary min-w-20" :disabled="!canEdit" @click="saveObjectEdit">
            <span v-if="editing.busy" class="loading loading-spinner loading-xs" aria-hidden="true" />
            {{ t('objects.save') }}
          </button>
        </div>
      </section>
      <div
        v-if="!inlineBox || !inlinePosition"
        class="absolute top-3 left-3 z-40 flex flex-wrap items-center gap-1 pointer-events-auto"
        @pointerdown.stop
        @mousedown.stop
        @dblclick.stop
        @wheel.stop
        @keydown.stop
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
          :disabled="!canEdit"
          @click.stop="saveObjectEdit"
        >
          {{ t('objects.save') }}
        </button>
        <button
          class="btn btn-sm bg-base-100/90 border-base-content/15 shadow-sm"
          :class="editing.drawing ? 'text-primary' : ''"
          :aria-pressed="editing.drawing"
          :disabled="!canEdit"
          @click.stop="editing.drawing = !editing.drawing"
        >
          {{ t('objects.draw_box') }}
        </button>
        <select
          v-model="draftClass"
          class="select select-sm bg-base-100/90 border-base-content/15"
          :aria-label="t('objects.set_class')"
          :disabled="!canEdit"
        >
          <option v-for="choice in editing.choices" :key="choice.key" :value="choice.key">
            {{ name(choice) }}
          </option>
        </select>
        <select v-if="currentClassKey" v-model="boxScope" data-object-box-scope class="select select-sm bg-base-100 text-base-content" :aria-label="t('objects.box_scope')" :disabled="editing.busy" @pointerdown.stop @keydown.stop @wheel.stop>
          <option value="current">{{ t('objects.boxes_only_class', { name: currentClassName }) }}</option>
          <option value="all">{{ t('objects.boxes_all_classes') }}</option>
        </select>
        <template v-if="editing.selected.length > 1">
          <button
            class="btn btn-sm bg-base-100/90 border-base-content/15 shadow-sm"
            :disabled="!canEdit"
            @click.stop="applyClassToSelected"
          >
            {{ t('objects.apply_class') }}
          </button>
          <button
            class="btn btn-sm bg-base-100/90 border-base-content/15 shadow-sm"
            :disabled="!canEdit"
            @click.stop="confirmSelected"
          >
            {{ t('objects.confirm') }}
          </button>
          <button
            class="btn btn-sm btn-ghost text-error bg-base-100/90"
            :disabled="!canEdit"
            @click.stop="removeSelected"
          >
            {{ t('objects.delete_box') }} ({{ editing.selected.length }})
          </button>
        </template>
        <span v-else class="text-xs px-1 opacity-70">{{
          t('objects.select_hint')
        }}</span>
      </div>
      <p v-if="removedCount && !editing.drawing && !inlineBox" role="status" class="absolute bottom-3 left-3 z-40 rounded-box bg-base-100/95 px-3 py-2 text-xs">
        {{ t('objects.removed_hint', { count: removedCount }) }}
      </p>
      <p
        v-if="editing.drawing"
        class="absolute bottom-3 left-3 z-40 rounded-box bg-base-100/90 px-3 py-2 text-xs"
      >
        {{ t('objects.draw_hint') }}
      </p>
      <p
        v-if="(editing.error || editing.stale) && (!inlineBox || !inlinePosition)"
        class="absolute bottom-3 right-3 z-40 max-w-sm rounded-box bg-base-100/95 border border-base-content/15 p-2 text-xs text-warning whitespace-pre-wrap break-words"
        role="alert"
      >
        {{ editing.stale ? t('objects.stale') : editing.error }}
      </p>
    </div>
  </template>
  <template v-else-if="ready && document">
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
          v-for="({ object, index }) in visibleObjects"
          :key="object.id"
          type="button"
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
          :title="t('objects.correct_box_hint', { name: name(object.class) })"
          :aria-label="name(object.class)"
          :disabled="!canCorrect"
          @pointerdown.stop
          @mousedown.stop
          @keydown.stop
          @dblclick.stop
          @click.stop="beginCorrection(object.id)"
        >
          <span
            class="absolute left-0 top-0 origin-top-left bg-base-100/95 text-base-content rounded px-1 py-0.5 text-xs whitespace-nowrap shadow"
            :style="{ transform: `scale(${1 / Math.max(0.01, zoom)})` }"
            >{{ index + 1 }} · {{ name(object.class) }} <span aria-hidden="true">✎</span></span
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
        v-if="document.objects.length"
        class="btn btn-sm bg-base-100/90 border-base-content/15 shadow-sm"
        :class="showObjectBoxes ? 'text-primary' : ''"
        :aria-pressed="showObjectBoxes"
        @click.stop="showObjectBoxes = !showObjectBoxes"
      >
        {{ t('objects.boxes') }} · {{ visibleObjects.length }}<template v-if="currentClassKey && boxScope==='current'"> / {{ document.objects.length }}</template>
      </button>
      <select v-if="showObjectBoxes && currentClassKey && document.objects.length" v-model="boxScope" data-object-box-scope class="select select-sm max-w-64 bg-base-100 text-base-content shadow-sm" :aria-label="t('objects.box_scope')" @pointerdown.stop @keydown.stop @wheel.stop>
        <option value="current">{{ t('objects.boxes_only_class', { name: currentClassName }) }}</option>
        <option value="all">{{ t('objects.boxes_all_classes') }}</option>
      </select>
      <button
        class="btn btn-sm bg-base-100/90 border-base-content/15 shadow-sm"
        :disabled="!canCorrect"
        :title="loading || !choices.length ? t('objects.loading') : !compatible ? t('objects.geometry') : t('objects.edit_hint')"
        @click.stop="startDrawing"
      >
        {{ t('objects.draw_box') }}
      </button>
      <p
        v-if="showObjectBoxes && compatible && currentClassKey && boxScope==='current' && !visibleObjects.length && document.objects.length"
        class="max-w-64 rounded-box bg-base-100 p-2 text-xs text-base-content"
      >{{ t('objects.boxes_no_current', { name: currentClassName }) }}</p>
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
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue';
import { useI18n } from 'vue-i18n';
import { ask } from '@tauri-apps/plugin-dialog';
import { config, libConfig } from '@/common/config';
import { useUIStore } from '@/stores/uiStore';
import { useObjects, objectFocus, showObjectBoxes } from '@/common/objects';
import { usePhotoObjects } from '@/common/photoObjects';
import {
  objectBoxStyle,
  objectGeometryCompatible,
  objectClassName,
  objectEditorPosition,
  objectOverlayEntries,
} from '@/common/objectUi';
import {
  objectEdit,
  objectEditActive,
  startObjectEdit,
  confirmObjectEditReplacement,
  cancelObjectEdit,
  saveObjectEdit,
  type ObjectBoxDraft,
} from '@/common/objectEditing';
const props = defineProps<{
  fileId?: number;
  fileVersion?: number;
  objectClassKey?: string;
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
const { view } = useObjects();
const ui = useUIStore();
const inputHandler = `ObjectEdit-${Math.random().toString(36).slice(2)}`;
const { result, loading } = usePhotoObjects(() => [
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
const choices = computed(() => (view.value?.classes || []).filter((choice: any) => choice.supported));
const canCorrect = computed(() =>
  props.ready && !loading.value && compatible.value && !!result.value?.sourceKey && choices.value.length > 0 && !objectEdit.value?.busy,
);
const canEdit = computed(() =>
  props.ready && compatible.value && !!editing.value && !editing.value.busy && !editing.value.stale,
);
function beginCorrection(objectId?: string) {
  if (!canCorrect.value || editing.value) return;
  const library = libConfig._libraryId, fileId = Number(props.fileId), data = result.value;
  const open = (accepted: boolean) => {
    if (!accepted || library !== libConfig._libraryId || fileId !== Number(props.fileId) || data !== result.value || !canCorrect.value) return;
    objectFocus.value = null;
    showObjectBoxes.value = true;
    startObjectEdit(fileId, data, choices.value, objectId);
    const selectedBox = editing.value?.boxes.find((box) => box.id === objectId);
    if (selectedBox) draftClass.value = selectedBox.class.key;
    else if (choices.value.some((choice: any) => choice.key === currentClassKey.value)) draftClass.value = currentClassKey.value;
  };
  const answer = confirmObjectEditReplacement(() => ask(t('objects.discard_draft'), { kind: 'warning' }));
  return typeof answer === 'boolean' ? open(answer) : answer.then(open);
}
function startDrawing() {
  if (!editing.value) {
    const library = libConfig._libraryId, fileId = props.fileId;
    const pending = beginCorrection();
    if (pending) return pending.then(() => { if (library === libConfig._libraryId && fileId === props.fileId && canEdit.value) editing.value!.drawing = true; });
  }
  if (!canEdit.value) return;
  editing.value!.drawing = true;
}
function selectBox(id: string, event?: MouseEvent) {
  if (!canEdit.value) return;
  const box = boxById(id);
  if (!box) return;
  const session = editing.value!;
  session.selected = event && (event.ctrlKey || event.shiftKey || event.metaKey)
    ? session.selected.includes(id) ? session.selected.filter((selected) => selected !== id) : [...session.selected, id]
    : [id];
  draftClass.value = box.class.key;
}
const name = (label: any) => objectClassName(label, t);
const boxScope = ref<'current' | 'all'>('current');
const currentClassKey = computed(() => props.objectClassKey || '');
const currentClassName = computed(() => name(
  view.value?.classes?.find((label: any) => label.key === currentClassKey.value)
  || document.value?.objects?.find((object: any) => object.class.key === currentClassKey.value)?.class
  || { key: currentClassKey.value, name: currentClassKey.value },
));
const visibleObjects = computed(() => objectOverlayEntries(document.value?.objects || [], currentClassKey.value, boxScope.value));
const visibleEditBoxes = computed(() => objectOverlayEntries(editing.value?.boxes || [], currentClassKey.value, boxScope.value, editing.value?.selected || []));
watch(() => [props.objectClassKey, libConfig._libraryId, config.settings.ai?.detectionProfile], () => { boxScope.value = 'current'; });
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
const editLayer = ref<HTMLElement | null>(null);
const inlinePanel = ref<HTMLElement | null>(null);
const classSelect = ref<HTMLSelectElement | null>(null);
const inlinePosition = ref<ReturnType<typeof objectEditorPosition> | null>(null);
const inlineBox = computed(() => {
  const session = editing.value;
  if (!props.ready || !compatible.value || session?.drawing || session?.selected.length !== 1) return null;
  return session.boxes.find((box) => box.id === session.selected[0]) || null;
});
const removedCount = computed(() => editing.value
  ? (document.value?.objects || []).filter((object: any) => !boxById(object.id) || boxById(object.id)?.ignored).length
  : 0,
);
function placeInlineEditor() {
  const layer = editLayer.value, panel = inlinePanel.value;
  const frame = Array.from(layer?.querySelectorAll<HTMLElement>('[data-object-box]') || [])
    .find((element) => element.dataset.objectBox === inlineBox.value?.id);
  const anchor = frame?.querySelector('[data-object-label]')?.getBoundingClientRect();
  if (!layer || !panel || !anchor) { inlinePosition.value = null; return; }
  const viewport = layer.getBoundingClientRect();
  inlinePosition.value = objectEditorPosition({
    left: anchor.left - viewport.left,
    top: anchor.top - viewport.top,
    bottom: anchor.bottom - viewport.top,
  }, viewport, panel.offsetHeight);
}
let resizeObserver: ResizeObserver | undefined;
function observeInlineEditor() {
  resizeObserver?.disconnect();
  if (editLayer.value) resizeObserver?.observe(editLayer.value);
  if (inlinePanel.value) resizeObserver?.observe(inlinePanel.value);
  placeInlineEditor();
}
watch([editLayer, inlinePanel], observeInlineEditor, { flush: 'post' });
watch(() => [
  inlineBox.value?.id, ...(inlineBox.value?.bounds || []),
  props.x, props.y, props.width, props.height, props.zoom, props.rotate,
], placeInlineEditor, { flush: 'post' });
watch(() => inlineBox.value?.id, async (id) => {
  if (!id) return;
  await nextTick();
  classSelect.value?.focus({ preventScroll: true });
});
watch(() => !!editing.value, (active) => {
  ui.removeInputHandler(inputHandler);
  if (active) ui.pushInputHandler(inputHandler);
}, { immediate: true });
onMounted(() => {
  if (typeof ResizeObserver !== 'undefined') resizeObserver = new ResizeObserver(placeInlineEditor);
  observeInlineEditor();
});
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
  if (!session || !canEdit.value || session.drawing || event.button !== 0) return;
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
  if (!gesture || !canEdit.value) return;
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
  if (!session?.drawing || !canEdit.value || event.button !== 0) return;
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
  if (!canEdit.value || !editing.value?.drawing || !gesture || gesture.id !== '__draw__') return;
  const p = point(event);
  if (!p) return;
  const l = Math.min(gesture.anchor.x, p.x),
    t = Math.min(gesture.anchor.y, p.y),
    r = Math.max(gesture.anchor.x, p.x),
    b = Math.max(gesture.anchor.y, p.y);
  drawRect.value = [l, t, r - l, b - t];
}
function finishDraw(event: PointerEvent) {
  if (!canEdit.value || !editing.value?.drawing || !gesture || gesture.id !== '__draw__') return;
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
  session.drawing = false;
}
function cancelDraw() {
  drawRect.value = null;
  gesture = undefined;
}
function setBoxClass(box: ObjectBoxDraft, key: string) {
  const session = editing.value;
  if (!session || !canEdit.value || box.ignored) return;
  const choice = session?.choices.find((c) => c.key === key);
  if (!choice) return;
  if (choice.key !== box.class.key) {
    box.class = { ...choice };
    box.score = null;
    box.reviewed = true;
  }
  draftClass.value = box.class.key;
}
function applyClassToSelected() {
  const session = editing.value;
  const choice = session?.choices.find((c) => c.key === draftClass.value) || session?.choices[0];
  if (!session || !canEdit.value || !choice) return;
  for (const box of session.boxes.filter((b) => session.selected.includes(b.id))) {
    setBoxClass(box, choice.key);
  }
}
function confirmSelected() {
  const session = editing.value;
  if (!session || !canEdit.value) return;
  for (const box of session.boxes.filter((b) => session.selected.includes(b.id) && !b.ignored))
    box.reviewed = true;
}
function toggleIgnored(box: ObjectBoxDraft) {
  const session = editing.value;
  if (!session || !canEdit.value) return;
  box.ignored = !box.ignored;
}
function removeSelected() {
  const session = editing.value;
  if (!session || !canEdit.value) return;
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
onBeforeUnmount(() => {
  onWindowUp();
  resizeObserver?.disconnect();
  ui.removeInputHandler(inputHandler);
});
let lastFocus: any = null;
watch([active, document, () => props.ready, compatible], () => {
  if (
    !active.value ||
    active.value === lastFocus ||
    !props.ready ||
    !compatible.value
  )
    return;
  const focused = document.value?.objects.find(
    (o: any) => o.id === active.value?.objectId,
  );
  const bounds = focused?.bounds;
  if (bounds) {
    if (currentClassKey.value && focused.class.key !== currentClassKey.value) boxScope.value = 'all';
    lastFocus = active.value;
    emit('locate', bounds);
  }
});
</script>
