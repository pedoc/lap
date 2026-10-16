<template>
  <template v-if="ready && document?.objects?.length">
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
import { computed, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { objectFocus, showObjectBoxes } from '@/common/objects';
import { usePhotoObjects } from '@/common/photoObjects';
import {
  objectBoxStyle,
  objectGeometryCompatible,
  objectClassName,
} from '@/common/objectUi';
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
  );
const name = (label: any) => objectClassName(label, t);
const active = computed(() =>
  objectFocus.value?.libraryId === libConfig._libraryId &&
  objectFocus.value?.fileId === props.fileId &&
  objectFocus.value?.profile === config.settings.ai?.detectionProfile
    ? objectFocus.value
    : null,
);
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
