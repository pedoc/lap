<template>
  <ModalDialog :title="t('face_diagnostics.title')" :width="width" :height="height" @cancel="close">
    <div ref="root" tabindex="-1" class="p-4 min-h-0 h-full flex flex-col gap-3" @keydown.esc.stop="close">
      <div class="text-sm">
        <p class="font-semibold">{{ d.modelName }} · {{ d.modelId }} · {{ d.modelVersion }}</p>
        <p class="text-xs opacity-70">{{ t('face_diagnostics.thresholds', { detection: d.parameters.detection_threshold, blur: d.parameters.blur_threshold, size: d.parameters.detector_size }) }}</p>
        <p v-for="device in d.devices" :key="device.role" class="text-xs opacity-70">{{ device.role }}: {{ device.requested }} → {{ device.selected }}<span v-if="device.deviceId != null"> #{{ device.deviceId }}</span><span v-if="device.fallbackReason"> · {{ device.fallbackReason }}</span></p>
      </div>
      <p class="text-xs rounded-box bg-base-300 p-3">{{ t('face_diagnostics.summary', d) }}</p>
      <p class="text-xs opacity-70">{{ t('face_diagnostics.explanation') }}</p>
      <p v-if="report.error || d.startupError" class="text-xs text-error" role="alert">{{ report.error || d.startupError }}</p>
      <div class="flex-1 min-h-0 overflow-auto space-y-3">
        <article v-for="image in d.images" :key="image.fileId" class="rounded-box border border-base-content/15 p-3 space-y-1 text-xs">
          <p class="font-semibold">{{ image.fileName }} · #{{ image.fileId }} · {{ t(`face_diagnostics.outcome_${image.outcome}`) }}</p>
          <p>{{ t('face_diagnostics.dimensions', { source: image.sourceSize.join(' × '), decoded: image.pipeline.decodedSize?.join(' × ') || '—', format: image.format }) }}</p>
          <p>{{ t('face_diagnostics.image_counts', { detected: image.pipeline.detectedFaces, quality: image.pipeline.qualityFiltered, suppressed: image.annotationSuppressed, stored: image.storedRegions, manual: image.manualRegions }) }}</p>
          <p>{{ t('face_diagnostics.maximum_score', { score: image.pipeline.maximumCandidateConfidence == null ? '—' : image.pipeline.maximumCandidateConfidence.toFixed(6) }) }}</p>
          <p v-if="image.error" class="text-error whitespace-pre-wrap">{{ t('face_diagnostics.error_stage', { stage: t(`face_diagnostics.stage_${image.pipeline.stage}`) }) }}: {{ image.error }}</p>
          <div v-for="(face, index) in image.pipeline.samples" :key="index" class="border-t border-base-content/10 pt-1">
            {{ t('face_diagnostics.face_sample', { index: index + 1, confidence: face.confidence.toFixed(6), blur: face.blurScore.toFixed(4), width: face.width.toFixed(1), height: face.height.toFixed(1), state: t(`face_diagnostics.outcome_${face.state}`) }) }}
          </div>
          <p v-if="image.outcome === 'quality_filtered'" class="text-warning">{{ t('face_diagnostics.quality_hint') }}</p>
          <p v-if="image.outcome === 'annotation_suppressed'" class="text-warning">{{ t('face_diagnostics.annotation_hint') }}</p>
          <p v-if="image.format === 'heic' || image.format === 'heif'" class="opacity-70">{{ t('face_diagnostics.heic_hint') }}</p>
        </article>
        <p v-if="d.cachedImages" class="text-xs opacity-70">{{ t('face_diagnostics.cached_hint', { count: d.cachedImages }) }}</p>
        <p v-if="d.omittedImages" class="text-xs opacity-70">{{ t('face_diagnostics.omitted', { count: d.omittedImages }) }}</p>
        <details>
          <summary class="text-xs cursor-pointer">{{ t('face_diagnostics.raw') }}</summary>
          <textarea readonly class="textarea textarea-bordered w-full h-52 font-mono text-xs" :value="text" :aria-label="t('face_diagnostics.raw')" />
        </details>
      </div>
      <div class="flex items-center justify-end gap-2">
        <span v-if="copied" class="text-xs">{{ t('face_diagnostics.copied') }}</span>
        <button type="button" class="btn btn-sm" @click="copy">{{ t('face_diagnostics.copy') }}</button>
        <button type="button" class="btn btn-sm btn-primary" @click="close">{{ t('face_diagnostics.close') }}</button>
      </div>
    </div>
  </ModalDialog>
</template>
<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { useUIStore } from '@/stores/uiStore';
import { diagnosticExportText } from '@/common/faceDiagnostics';
import ModalDialog from '@/components/ModalDialog.vue';
const props = defineProps<{ report: any }>(), emit = defineEmits(['cancel']);
const { t } = useI18n(), ui = useUIStore();
const d = computed(() => props.report.diagnostics), text = computed(() => diagnosticExportText(props.report));
const root = ref<HTMLElement | null>(null), copied = ref(false), width = Math.min(800, window.innerWidth - 24), height = Math.min(780, window.innerHeight - 32);
const handler = `FaceDiagnostics:${Math.random().toString(36).slice(2)}`; let alive = true;
function close() { emit('cancel'); }
async function copy() { try { await navigator.clipboard.writeText(text.value); if (alive) copied.value = true; } catch { if (alive) root.value?.querySelector('textarea')?.select(); } }
onMounted(async () => { ui.pushInputHandler(handler); await nextTick(); if (alive) root.value?.focus({ preventScroll: true }); });
onBeforeUnmount(() => { alive = false; ui.removeInputHandler(handler); });
</script>
