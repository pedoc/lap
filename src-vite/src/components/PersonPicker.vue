<template>
  <div class="space-y-2">
    <input v-model="search" type="search" class="input input-bordered input-sm w-full" :placeholder="t('face_editor.search')" :disabled="disabled" autocomplete="off" :aria-label="t('face_editor.search')" />
    <p v-if="error" class="text-xs text-error" role="alert">{{ error }}</p>
    <p v-if="loading" class="text-xs opacity-70">{{ t('face_editor.searching') }}</p>
    <div class="max-h-44 overflow-auto space-y-1">
      <button v-for="person in people" :key="person.id" type="button" class="btn btn-sm w-full justify-start" :class="modelValue?.id === person.id ? 'btn-primary' : 'btn-ghost'" :disabled="disabled || loading" @click="emit('update:modelValue', { ...person })">
        {{ person.name || t('face_actions.unknown') }} · #{{ person.id }}
      </button>
      <p v-if="!loading && !people.length" class="text-xs opacity-70">{{ t('face_editor.no_results') }}</p>
    </div>
  </div>
</template>
<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
const props = defineProps<{ libraryId: string; modelValue: { id: number; name: string | null } | null; exclude?: number[]; disabled?: boolean }>();
const emit = defineEmits(['update:modelValue']);
const { t } = useI18n();
const search = ref(''), people = ref<any[]>([]), loading = ref(false), error = ref('');
let alive = true, requestId = 0, timer: ReturnType<typeof setTimeout> | null = null;
async function load() {
  const ticket = ++requestId, libraryId = props.libraryId;
  loading.value = true; error.value = '';
  try {
    const result = await invoke<any[]>('get_face_people', { libraryId, search: search.value.trim() });
    if (alive && ticket === requestId && libraryId === props.libraryId) people.value = result.filter(person => !(props.exclude || []).includes(person.id));
  } catch (e: any) { if (alive && ticket === requestId) { people.value = []; error.value = e?.message || String(e); } }
  finally { if (alive && ticket === requestId) loading.value = false; }
}
watch(search, () => { requestId++; emit('update:modelValue', null); if (timer) clearTimeout(timer); timer = setTimeout(load, 200); });
watch(() => props.libraryId, () => { requestId++; people.value = []; emit('update:modelValue', null); void load(); });
onMounted(load);
onBeforeUnmount(() => { alive = false; requestId++; if (timer) clearTimeout(timer); });
</script>
