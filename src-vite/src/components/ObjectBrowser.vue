<template>
  <div class="sidebar-panel overflow-hidden" data-object-workspace>
    <div class="sidebar-panel-header">
      <h2 class="sidebar-panel-header-title">{{ t('objects.title') }}</h2>
    </div>
    <div class="flex-1 overflow-y-auto px-3 pb-3 space-y-4">
      <nav :aria-label="t('objects.states')" class="space-y-1">
        <button
          v-for="state in OBJECT_STATES"
          :key="state"
          class="w-full rounded-box px-3 py-2 flex justify-between text-sm text-left hover:bg-base-content/5"
          :class="
            !libConfig.objects.classKey && libConfig.objects.state === state
              ? 'bg-primary/10 text-primary font-medium'
              : ''
          "
          :aria-current="
            !libConfig.objects.classKey && libConfig.objects.state === state
              ? 'page'
              : undefined
          "
          @click="selectState(state)"
        >
          <span>{{ t('objects.state_' + state) }}</span
          ><span class="text-xs tabular-nums opacity-60">{{
            (state === 'all'
              ? view?.eligible
              : view?.[state]
            )?.toLocaleString() ?? '—'
          }}</span>
        </button>
      </nav>
      <section class="border-t border-base-content/10 pt-3 space-y-2">
        <label class="input input-sm w-full"
          ><input
            v-model="query"
            type="search"
            :placeholder="t('objects.search_classes')"
            :aria-label="t('objects.search_classes')"
            @keydown.stop
        /></label>
        <label class="flex items-center gap-2 text-xs"
          ><input
            v-model="allClasses"
            type="checkbox"
            class="checkbox checkbox-xs"
          />{{ t('objects.all_classes') }}</label
        >
        <label
          v-if="libConfig.objects.classKey"
          class="flex items-center justify-between gap-2 text-xs"
          >{{ t('objects.minimum')
          }}<input
            class="input input-sm w-20"
            type="number"
            min="1"
            max="10000"
            :value="libConfig.objects.minCount"
            @change="setMinimum"
            @keydown.stop
        /></label>
        <div class="space-y-1">
          <button
            v-for="label in classes"
            :key="label.key"
            class="w-full flex justify-between gap-2 rounded-box px-3 py-2 text-sm text-left hover:bg-base-content/5"
            :class="
              libConfig.objects.classKey === label.key
                ? 'bg-primary/10 text-primary'
                : ''
            "
            :aria-current="
              libConfig.objects.classKey === label.key ? 'page' : undefined
            "
            @click="selectClass(label)"
          >
            <span class="truncate"
              >{{ name(label)
              }}<span v-if="!label.supported" class="ml-1 text-xs opacity-60">{{
                t('objects.retained')
              }}</span></span
            ><span
              class="text-xs tabular-nums opacity-60 shrink-0"
              :title="t('objects.count_hint', label)"
              >{{ label.photos }} / {{ label.instances }}</span
            >
          </button>
          <p v-if="!classes.length" class="text-xs opacity-60 p-2">
            {{ t('objects.no_classes') }}
          </p>
        </div>
      </section>
      <section class="border-t border-base-content/10 pt-3 space-y-2">
        <h3 class="text-sm font-medium">
          {{ view?.modelName || t('objects.model') }}
        </h3>
        <button class="btn btn-sm btn-ghost -ml-2" @click="openObjectSettings">
          {{ t('objects.settings') }} ↗
        </button>
        <p class="text-xs opacity-60">{{ t('objects.scope_hint') }}</p>
        <p v-if="view && !view.modelInstalled" class="text-xs text-warning">
          {{ t('objects.download_first') }}
        </p>
        <p v-if="view" class="text-xs">{{ t('objects.coverage', view) }}</p>
        <template v-if="view?.status.jobId">
          <p class="text-xs break-words opacity-70">
            {{ t('ocr.scope_' + (view.status.scope || 'library'))
            }}<span v-if="view.status.scopeName">
              · {{ view.status.scopeName }}</span
            >
          </p>
          <progress
            v-if="view.status.running"
            class="progress progress-primary w-full"
            :value="view.status.completed"
            :max="Math.max(1, view.status.total)"
          />
          <p class="text-xs" aria-live="polite">
            {{
              t(
                'objects.phase_' +
                  objectTaskPhase(view.status),
              )
            }}
            · {{ t('objects.progress', view.status) }}
          </p>
          <p v-if="view.status.cancelled" class="text-xs opacity-60">
            {{ t('objects.saved') }}
          </p>
        </template>
        <p
          v-if="error || view?.status.error"
          role="alert"
          class="text-xs text-error whitespace-pre-wrap break-words"
        >
          {{ error || view.status.error }}
        </p>
        <div class="flex flex-wrap gap-2">
          <button
            class="btn btn-sm btn-primary"
            :disabled="busy || !view?.modelInstalled || view?.status.running"
            @click="run()"
          >
            {{ t('objects.resume') }}
          </button>
          <button
            class="btn btn-sm"
            :disabled="
              busy ||
              !view?.modelInstalled ||
              view?.status.running ||
              !view?.failed
            "
            @click="run(false, true)"
          >
            {{ t('objects.retry') }}
          </button>
          <button
            class="btn btn-sm"
            :disabled="busy || !view?.modelInstalled || view?.status.running"
            @click="run(true)"
          >
            {{ t('objects.rebuild') }}
          </button>
          <button
            v-if="view?.status.running"
            class="btn btn-sm"
            :disabled="busy || view.status.stopping"
            @click="cancel"
          >
            {{ t(view.status.stopping ? 'objects.stopping_short' : 'objects.stop') }}
          </button>
        </div>
        <div class="text-xs space-y-1" aria-live="polite"><p class="font-medium" :class="execution.kind==='cpu_fallback'?'text-warning':''">{{ t('objects.execution_' + execution.kind) }}<span v-if="execution.providers.length"> · {{ execution.providers.join(' / ') }}</span></p><p v-for="reason in execution.reasons" :key="reason" class="text-warning whitespace-pre-wrap break-words">{{ reason }}</p></div>
        <details v-if="view?.sessions?.length" class="text-xs">
          <summary class="cursor-pointer opacity-70">
            {{ t('objects.device') }}
          </summary>
          <pre
            class="whitespace-pre-wrap break-all mt-2 max-h-52 overflow-auto"
            >{{ JSON.stringify(view.sessions, null, 2) }}</pre
          >
        </details>
      </section>
      <ObjectTaskHistory :entries="view?.history" :error="view?.historyError" :busy="busy" :running="view?.status.running" @resume="resume" />
      <ObjectAutoSettings />
    </div>
  </div>
</template>
<script setup lang="ts">
import { ref, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { ask } from '@tauri-apps/plugin-dialog';
import { config, libConfig } from '@/common/config';
import ObjectTaskHistory from './ObjectTaskHistory.vue';
import ObjectAutoSettings from './ObjectAutoSettings.vue';
import { OBJECT_STATES, objectClassName, objectTaskPhase, objectExecution } from '@/common/objectUi';
import { useObjects, openObjectSettings } from '@/common/objects';
const { t } = useI18n();
const { view, error, busy, start, stop } = useObjects();
const execution = computed(() => objectExecution(view.value?.sessions));
const query = ref(''),
  allClasses = ref(false);
const name = (label: any) => objectClassName(label, t);
const classes = computed(() =>
  (view.value?.classes || []).filter(
    (label: any) =>
      (allClasses.value ||
        label.photos > 0 ||
        label.key === libConfig.objects.classKey) &&
      (name(label) + ' ' + label.name)
        .toLowerCase()
        .includes(query.value.toLowerCase()),
  ),
);
function selectState(state: string) {
  Object.assign(libConfig.objects, {
    state,
    classKey: '',
    className: '',
    minCount: 1,
  });
}
function selectClass(label: any) {
  Object.assign(libConfig.objects, {
    state: 'all',
    classKey: label.key,
    className: label.name,
  });
}
function setMinimum(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  libConfig.objects.minCount = Number.isSafeInteger(value)
    ? Math.max(1, Math.min(10000, value))
    : 1;
  (event.target as HTMLInputElement).value = String(libConfig.objects.minCount);
}
async function run(force = false, retryFailed = false) {
  const libraryId = libConfig._libraryId,
    profile = config.settings.ai?.detectionProfile;
  try {
    if (
      force &&
      !(await ask(t('objects.rebuild_confirm'), { kind: 'warning' }))
    )
      return;
    if (
      libraryId !== libConfig._libraryId ||
      profile !== config.settings.ai?.detectionProfile
    )
      return;
    await start({ force, retryFailed });
  } catch {
    /* Visible inline. */
  }
}
async function resume(jobId:string,failed:boolean){try{await start({resumeJobId:jobId,resumeFailed:failed});}catch{/* Shared error is visible. */}}
async function cancel() {
  try {
    await stop();
  } catch {
    /* Visible inline. */
  }
}
</script>
