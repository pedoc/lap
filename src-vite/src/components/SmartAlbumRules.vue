<template>
  <div class="border border-base-content/10 rounded-box p-2 space-y-2">
    <div class="flex items-center gap-2 text-xs">
      <select
        v-model="group.operator"
        class="select select-sm flex-1"
        :aria-label="$t('smart_albums.match')"
      >
        <option value="all">{{ $t("album.smart_edit.match_all") }}</option>
        <option value="any">{{ $t("album.smart_edit.match_any") }}</option>
      </select>
      <button
        type="button"
        class="btn btn-sm"
        :disabled="total >= 100"
        @click="group.value.rules.push(makeRule())"
      >
        {{ $t("album.smart_edit.add_rule") }}
      </button>
      <button
        type="button"
        class="btn btn-sm"
        :disabled="depth >= 5 || total >= 98"
        @click="addGroup"
      >
        {{ $t("smart_albums.add_group") }}
      </button>
    </div>
    <div
      v-for="(rule, index) in group.value.rules"
      :key="rule.id"
      class="flex items-start gap-1"
    >
      <SmartAlbumRules
        v-if="rule.field === 'group'"
        :group="rule"
        :depth="depth + 1"
        :total="total"
        :make-rule="makeRule"
        class="flex-1 min-w-0"
      >
        <template #rule="slotProps"
          ><slot name="rule" v-bind="slotProps"
        /></template>
      </SmartAlbumRules>
      <div v-else class="flex-1 min-w-0"><slot name="rule" :rule="rule" /></div>
      <button
        type="button"
        class="btn btn-ghost btn-sm text-error"
        :aria-label="$t('album.smart_edit.remove_rule')"
        :disabled="group.value.rules.length <= 1"
        @click="group.value.rules.splice(index, 1)"
      >
        ×
      </button>
    </div>
  </div>
</template>
<script setup lang="ts">
const props = defineProps<{
  group: any;
  makeRule: (field?: string) => any;
  depth: number;
  total: number;
}>();
function addGroup() {
  props.group.value.rules.push({
    id: crypto.randomUUID(),
    field: "group",
    operator: "all",
    value: { rules: [props.makeRule()] },
  });
}
</script>
