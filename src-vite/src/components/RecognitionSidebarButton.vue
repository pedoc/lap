<template>
  <TButton
    buttonSize="large"
    :icon="icon"
    :tooltip="hideTooltip ? '' : summary"
    tooltipPlacement="right"
    :tooltipAutoHide="!status"
    :tooltipAlways="!!status && !hideTooltip"
    :selected="selected"
    :disabled="disabled"
    @click="emit('click', $event)"
  >
    <template #badge>
      <span
        v-if="status"
        class="pointer-events-none absolute right-1 top-1 size-2 rounded-full ring-2 ring-base-200"
        :class="status.tone === 'running' ? 'bg-success animate-pulse motion-reduce:animate-none' : 'bg-warning'"
        :data-recognition-indicator="status.tone"
        aria-hidden="true"
      ></span>
    </template>
    <template #tooltip>
      <div v-if="status" class="w-64 max-w-full space-y-1 text-left leading-5 [overflow-wrap:anywhere]" data-recognition-tooltip>
        <p class="font-medium">{{ label }}</p>
        <p :class="status.tone === 'warning' ? 'text-warning' : 'text-success'">{{ status.title }}</p>
        <TaskProgressBar v-if="status.progress" :tone="status.tone === 'warning' ? 'warning' : 'success'" :value="status.progress.value" :max="status.progress.max" :aria-label="status.title" />
        <p v-for="(line, index) in status.lines" :key="index" class="line-clamp-4 text-base-content/70">{{ line }}</p>
      </div>
      <template v-else>{{ label }}</template>
    </template>
  </TButton>
</template>

<script setup lang="ts">
import { computed, type Component } from 'vue';
import type { RecognitionIndicator } from '@/common/recognitionStatus';
import TButton from './TButton.vue';
import TaskProgressBar from './TaskProgressBar.vue';

const props = defineProps<{
  icon: Component;
  label: string;
  selected?: boolean;
  disabled?: boolean;
  status?: RecognitionIndicator | null;
  hideTooltip?: boolean;
}>();
const emit = defineEmits<{ click: [event: Event] }>();
const summary = computed(() => props.status
  ? [props.label, props.status.title, ...props.status.lines,
    props.status.progress ? `${props.status.progress.value ?? ''}/${props.status.progress.max}` : '',
  ].filter(Boolean).join(' · ')
  : props.label);
</script>
