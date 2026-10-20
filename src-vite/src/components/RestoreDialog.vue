<template>
  <ModalDialog
    :title="$t('settings.database.restore_dialog_title')"
    :width="520"
    :height="520"
    @cancel="clickCancel"
  >
    <div v-if="completed" class="flex flex-col items-center justify-center flex-1 gap-4 text-center">
      <p>{{ $t('settings.database.restore_restart_hint') }}</p>
      <button class="t-button-primary" :disabled="isProcessing" @click="restartApp">
        {{ $t('settings.database.restore_restart') }}
      </button>
    </div>
    <div v-else-if="!parsed" class="flex flex-col items-center justify-center flex-1 gap-2">
      <button class="t-button-primary" :disabled="isProcessing" @click="selectFile">
        {{ isProcessing ? $t('settings.database.restore_checking') : $t('settings.database.restore_select_file') }}
      </button>
    </div>
    <div v-else class="flex flex-col flex-1 min-h-0 overflow-hidden select-none">
      <!-- Backup file info -->
      <div class="flex justify-between mb-2 px-1 text-sm text-base-content/30">
        <div>{{ $t('settings.database.restore_file_info', { path: backupFileName }) }}</div>
        <div>{{ $t('settings.database.restore_libraries', { count: parsed.libraries.length, size: formatFileSize(totalBackupSize) }) }}</div>
      </div>
      <div class="text-xs text-base-content/60 mb-2">{{ $t('settings.database.restore_created_at', { date: formatTimestamp(parsed.createdAt, 'date_time') }) }}</div>

      <p class="text-xs text-warning mb-2">{{ $t('media_recovery.backup_scope') }}</p>
      <p class="text-xs text-base-content/60 mb-2">{{ $t(parsed.formatVersion >= 2 ? 'settings.database.restore_checksum_hint' : 'settings.database.restore_legacy_hint') }}</p>
      <p v-if="parsed.recoveryReports?.length" class="text-xs text-warning whitespace-pre-wrap max-h-20 overflow-auto">{{ parsed.recoveryReports.join('\n') }}</p>
      <!-- Library list -->
      <div class="flex-1 overflow-y-auto border border-base-content/5 bg-base-300/30 shadow-sm rounded-box p-1">
        <div
          v-for="lib in parsed.libraries"
          :key="lib.entry"
          class="flex items-center justify-between px-3 py-2.5 rounded-box hover:bg-base-100/20 cursor-pointer transition-colors"
          :class="{ 'bg-base-100/10': restoreSelections[lib.entry] }"
          @click="toggleLib(lib.entry)"
        >
          <div class="flex items-center gap-2 min-w-0">
            <input type="checkbox" class="checkbox checkbox-xs checkbox-primary" :disabled="isProcessing" :checked="!!restoreSelections[lib.entry]" @click.stop @change="toggleLib(lib.entry)" />
            <span class="truncate text-sm">{{ lib.name }}</span>
          </div>
          <div class="shrink-0 flex items-center gap-2">
            <span
              v-if="isNewLibrary(lib.name)"
              class="text-xs px-1.5 py-0.5 rounded bg-success/10 text-success/70"
            >
              {{ $t('settings.database.restore_new_library') }}
            </span>
            <span
              v-else
              class="text-xs px-1.5 py-0.5 rounded bg-warning/10 text-warning/70"
            >
              {{ $t('settings.database.restore_conflict') }}
            </span>
            <span class="text-xs text-base-content/30 w-16 text-right">{{ formatFileSize(lib.dbSize) }}</span>
          </div>
        </div>
      </div>

      <label v-if="parsed.configFiles.length" class="flex items-start gap-2 text-sm mt-3 cursor-pointer">
        <input v-model="restoreSettings" type="checkbox" class="checkbox checkbox-xs checkbox-primary mt-0.5" :disabled="isProcessing" />
        <span>{{ $t('settings.database.restore_settings') }}<span class="block text-xs text-base-content/60 mt-1">{{ $t('settings.database.restore_settings_hint') }}</span></span>
      </label>

      <!-- Note -->
      <div class="text-sm text-warning/70 px-1 mt-2">
        ⚠ {{ $t('settings.database.restore_note') }}
      </div>
    </div>

    <!-- Buttons -->
    <div class="flex justify-end items-center gap-2 shrink-0 pt-2">
      <button
        class="px-4 py-1.5 rounded-box text-base-content/70 hover:bg-base-100/30 cursor-pointer text-sm"
        :disabled="isProcessing"
        @click="clickCancel"
      >
        {{ $t(completed ? 'settings.database.restore_close' : 'msgbox.cancel') }}
      </button>
      <button
        v-if="!completed"
        class="px-4 py-1.5 rounded-box text-sm cursor-pointer"
        :class="parsed && restoreCount > 0 && !isProcessing
          ? 'bg-primary text-primary-content hover:opacity-90'
          : 'bg-base-100/40 text-base-content/30 cursor-default'"
        :disabled="!parsed || restoreCount === 0 || isProcessing"
        @click="doRestore"
      >
        {{ isProcessing ? $t('settings.database.restore_restoring') : $t('settings.database.restore') }}
      </button>
    </div>
  </ModalDialog>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { parseBackupFile, restoreDatabases } from '@/common/api';
import { formatFileSize, formatTimestamp } from '@/common/utils';
import ModalDialog from '@/components/ModalDialog.vue';
import { open } from '@tauri-apps/plugin-dialog';
import { useToast } from '@/common/toast';
import { invoke } from '@tauri-apps/api/core';

const emit = defineEmits(['done', 'cancel']);
const { t, locale, messages } = useI18n();
const toast = useToast();
const localeMsg = computed(() => messages.value[locale.value] as any);

interface BackupLib {
  name: string;
  dbSize: number;
  entry: string;
}

interface ParsedBackup {
  createdAt: number;
  libraries: BackupLib[];
  formatVersion: number;
  configFiles: string[];
  recoveryReports: string[];
}

const parsed = ref<ParsedBackup | null>(null);
const backupFileName = ref('');
const backupFullPath = ref('');
const existingNames = ref<Set<string>>(new Set());
const restoreSelections = ref<Record<string, boolean>>({});
const isProcessing = ref(false);
const restoreSettings = ref(false);
const completed = ref(false);

const totalBackupSize = computed(() => {
  if (!parsed.value) return 0;
  return parsed.value.libraries.reduce((sum, lib) => sum + lib.dbSize, 0);
});

const restoreCount = computed(() =>
  Object.values(restoreSelections.value).filter(Boolean).length
);

const isNewLibrary = (name: string) => !existingNames.value.has(name);

const toggleLib = (name: string) => {
  if (isProcessing.value) return;
  restoreSelections.value = {
    ...restoreSelections.value,
    [name]: !restoreSelections.value[name],
  };
};

// Load existing library names for conflict detection
import { getAppConfig } from '@/common/api';
const loadExistingNames = async () => {
  try {
    const config = await getAppConfig();
    if (config?.libraries) {
      existingNames.value = new Set(config.libraries.map((l: any) => l.name));
    }
  } catch {
    // ignore
  }
};
loadExistingNames();

const selectFile = async () => {
  if (isProcessing.value) return;
  isProcessing.value = true;
  try {
    const filePath = await open({
      title: localeMsg.value.settings?.database?.restore_select_file || 'Select Backup File',
      filters: [{ name: 'ZIP Archive', extensions: ['zip'] }],
      multiple: false,
    });

    if (!filePath) return;

    const meta = await parseBackupFile(filePath);
    backupFullPath.value = filePath as string;
    // Extract just the filename for display
    const parts = (filePath as string).split(/[\\/]/);
    backupFileName.value = parts[parts.length - 1];

    parsed.value = {
      createdAt: meta.createdAt,
      libraries: meta.libraries,
      formatVersion: meta.formatVersion,
      configFiles: meta.configFiles || [],
      recoveryReports: meta.recoveryReports || [],
    };

    // Default: select all libraries
    restoreSelections.value = {};
    restoreSettings.value = false;
    for (const lib of meta.libraries) {
      restoreSelections.value[lib.entry] = true;
    }
  } catch (error: any) {
    toast.error(error.message || error.toString());
  } finally {
    isProcessing.value = false;
  }
};

const doRestore = async () => {
  if (!parsed.value || restoreCount.value === 0) return;
  isProcessing.value = true;

  try {
    const selections = parsed.value.libraries
      .filter(lib => restoreSelections.value[lib.entry])
      .map(lib => ({
        libraryName: lib.name,
        entry: lib.entry,
        shouldRename: !isNewLibrary(lib.name),
      }));

    const result = await restoreDatabases(backupFullPath.value, selections, restoreSettings.value);
    if(result.warnings?.length)toast.warning(result.warnings.join('\n'),{duration:10000});
    toast.success(t('settings.database.restore_success', {
      count: result.restoredCount,
      names: result.restoredNames.join(', '),
    }));
    if (result.restartRequired) completed.value = true;
    else emit('done');
  } catch (error: any) {
    toast.error(error.message || error.toString());
  } finally {
    isProcessing.value = false;
  }
};

const clickCancel = () => {
  if (isProcessing.value) return;
  emit(completed.value ? 'done' : 'cancel');
};

const restartApp = async () => {
  isProcessing.value = true;
  try { await invoke('request_app_restart'); }
  catch (error: any) { toast.error(error.message || error.toString()); }
  finally { isProcessing.value = false; }
};
</script>
