<template>
  <ModalDialog
    :title="$t('smart_albums.choose_reference')"
    :width="600"
    @cancel="$emit('cancel')"
  >
    <form
      class="flex gap-2 mb-2"
      @submit.prevent="
        page = 0;
        load();
      "
    >
      <input
        v-model="search"
        class="input input-sm flex-1"
        :placeholder="$t('smart_albums.photo_name')"
      /><button class="btn btn-sm" :disabled="busy">
        {{ $t("smart_albums.preview") }}
      </button>
    </form>
    <p v-if="error" role="alert" class="text-error text-xs break-all">
      {{ error }}
    </p>
    <p v-if="busy" class="text-xs">
      <span class="loading loading-spinner loading-xs"></span>
    </p>
    <div class="grid grid-cols-4 gap-2 max-h-80 overflow-auto">
      <button
        v-for="file in files"
        :key="file.id"
        class="p-1 rounded-box hover:bg-base-content/10 text-left"
        @click="$emit('select', file.id)"
      >
        <img
          v-if="file.previewUrl"
          :src="file.previewUrl"
          class="w-full aspect-square object-cover rounded-box"
        /><span
          v-else
          class="block w-full aspect-square bg-base-content/5"
        ></span>
        <span class="block truncate text-xs" :title="file.name">{{
          file.name
        }}</span
        ><span class="text-[10px] text-base-content/50">#{{ file.id }}</span>
      </button>
    </div>
    <div class="flex justify-between mt-3">
      <button
        class="btn btn-sm"
        :disabled="busy || page === 0"
        @click="
          page--;
          load();
        "
      >
        ←</button
      ><span class="text-xs">{{ page + 1 }}</span
      ><button
        class="btn btn-sm"
        :disabled="busy || !hasNext"
        @click="
          page++;
          load();
        "
      >
        →
      </button>
    </div>
  </ModalDialog>
</template>
<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from "vue";
import { getSmartQueryFiles, getFileThumbById } from "@/common/api";
import { libConfig } from "@/common/config";
import { getThumbUrl, isWin, getThumbnailDataUrl } from "@/common/utils";
import ModalDialog from "./ModalDialog.vue";
const emit = defineEmits(["select", "cancel"]);
const search = ref("");
const page = ref(0);
const files = ref<any[]>([]);
const busy = ref(false);
const error = ref("");
const hasNext = ref(false);
let request = 0;
let disposed = false;
const library = libConfig._libraryId;
async function load() {
  const token = ++request;
  busy.value = true;
  error.value = "";
  const rules: any[] = [
    { id: "image", field: "file_type", operator: "is", value: 5 },
  ];
  if (search.value.trim())
    rules.push({
      id: "name",
      field: "name",
      operator: "contains",
      value: search.value.trim(),
    });
  try {
    const next = await getSmartQueryFiles(
      { version: 2, match: "all", rules, sortType: 0, sortOrder: 1 },
      page.value * 20,
      21,
    );
    const rendered = await Promise.all(
      next.slice(0, 20).map(async (file: any) => {
        try {
          return {
            ...file,
            previewUrl: isWin
              ? getThumbnailDataUrl(
                  await getFileThumbById(file.id, 128, false),
                  "",
                  false,
                  128,
                )
              : getThumbUrl(file.id, false, 128),
          };
        } catch {
          return { ...file, previewUrl: "" };
        }
      }),
    );
    if (!disposed && token === request && library === libConfig._libraryId) {
      files.value = rendered;
      hasNext.value = next.length > 20;
    }
  } catch (reason) {
    if (!disposed && token === request) error.value = String(reason);
  } finally {
    if (token === request) busy.value = false;
  }
}
watch(
  () => libConfig._libraryId,
  () => emit("cancel"),
);
onMounted(() => {
  void load();
});
onBeforeUnmount(() => {
  disposed = true;
  request++;
});
</script>
