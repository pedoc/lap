<template>
  <div class="flex items-center gap-2">
    <progress class="progress min-w-0 flex-1" :class="{ 'progress-primary': tone === 'primary', 'progress-success': tone === 'success', 'progress-warning': tone === 'warning' }" :value="percent === null ? undefined : value" :max="Math.max(1, max)" :aria-label="ariaLabel" />
    <span v-if="percent !== null" class="shrink-0 text-xs tabular-nums" data-task-percent>{{ percent }}%</span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { progressPercent } from '@/common/progress';

const props = withDefaults(defineProps<{
  value?: number;
  max: number;
  tone?: 'primary' | 'success' | 'warning';
  ariaLabel?: string;
}>(), { tone: 'primary' });
const percent = computed(() => progressPercent(props.value, props.max));
</script>
