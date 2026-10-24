<template>
  <div v-if="!isReady || (isMainWindow && !uiStore.startupReady)" class="fixed inset-0 z-[1000] flex items-center justify-center bg-base-300" role="status" aria-label="Loading" data-tauri-drag-region>
    <span class="loading loading-spinner loading-lg text-primary" aria-hidden="true"></span>
  </div>
  <template v-if="isReady">
    <router-view />
    <ToastContainer />
    <MediaJobPanel />
    <MediaAiBatch />
    <FaceDiagnosticsDialog v-if="faceDiagnosticState.visible && faceDiagnosticState.report?.library_id === libConfig._libraryId" :report="faceDiagnosticState.report" @cancel="faceDiagnosticState.visible = false" />
  </template>
</template>
 
<script setup>
import { useOcrActivity } from '@/common/ocrAutomation';
import { ref, watch, onMounted, onUnmounted, nextTick } from 'vue';
import { emit } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useConfigStore } from '@/stores/configStore';
import { useLibraryStore } from '@/stores/libraryStore';
import { useUIStore } from '@/stores/uiStore';
import { isMac, setTheme, SCALE_VALUES } from '@/common/utils';
import { matchesShortcut } from '@/common/shortcuts';
import ToastContainer from '@/components/ToastContainer.vue';
import MediaJobPanel from '@/components/MediaJobPanel.vue';
import MediaAiBatch from '@/components/MediaAiBatch.vue';
import FaceDiagnosticsDialog from '@/components/FaceDiagnosticsDialog.vue';
import { mediaAiSearch } from '@/common/mediaAi';
import { faceDiagnosticState } from '@/common/faceDiagnostics';

useOcrActivity();
const libConfig = useLibraryStore();
watch(() => libConfig._libraryId, libraryId => {
  Object.assign(mediaAiSearch, { active: false, libraryId, modelId: '', albumId: null, error: '', hits: [], revision: 0 });
});
const isReady = ref(false);
const config = useConfigStore();
const uiStore = useUIStore();
const isMainWindow = getCurrentWebviewWindow().label === 'main';

// Auto-save library state when any config changes
watch(() => libConfig.$state, () => {
  if (libConfig._initialized) {
    libConfig.save();
  }
}, { deep: true });

watch(
  () => Number(config.settings.scale || 1),
  (newScale) => {
    const win = getCurrentWebviewWindow();
    if (win.label === 'main') {
      applyMainWindowScale(newScale);
    }
  }
);

onMounted(async () => {
  const win = getCurrentWebviewWindow();
  if (win.label === 'main') {
    window.addEventListener('keydown', handleKeyDown, { capture: true });
    applyMainWindowScale(Number(config.settings.scale || 1));
    if (import.meta.env.PROD) {
      window.addEventListener('contextmenu', handleContextMenu);
    }
  }

  setTheme(config.settings.appearance, 
    config.settings.appearance === 0 ? config.settings.lightTheme : config.settings.darkTheme);

  // Show the themed loading screen while library and first-content loading run.
  if (isMainWindow) {
    await nextTick();
    await win.show();
    performance.mark('startup-window-shown');
    void invoke('record_startup_stage', { stage: 'window-shown' }).catch(console.error);
  }

  // Initialize library state from backend
  try {
    await libConfig.init();
  } catch (error) {
    console.error('[App] Library initialization failed:', error);
    // Continue anyway - user can retry from UI
  } finally {
    isReady.value = true;
    if (win.label === 'main') {
      performance.mark('startup-library-state-ready');
      void invoke('record_startup_stage', { stage: 'library-state-ready' }).catch(console.error);
    }
  }
});

onUnmounted(async () => {
  const win = getCurrentWebviewWindow();
  if (win.label === 'main') {
    window.removeEventListener('keydown', handleKeyDown, { capture: true });
    document.documentElement.style.fontSize = '';
    if (import.meta.env.PROD) {
      window.removeEventListener('contextmenu', handleContextMenu);
    }
  }
});

const handleKeyDown = (event) => {
  if (handleMainWindowScaleShortcut(event)) {
    return;
  }

  if (!isMac && matchesShortcut('app.preferences', event)) {
    event.preventDefault();
    event.stopPropagation();
    emit('app-open-preferences');
    return;
  }

  // Inputs that handle their own keys must not trigger global shortcuts (Enter is file.rename on
  // macOS). SearchBox/MoveTo/ImportOrganize are deliberately excluded — they rely on this channel.
  const uiStore = useUIStore();
  // MoveTo buttons activate locally; forwarding Enter would also confirm the move.
  if (
    event.key === 'Enter' && uiStore.isInputActive('MoveTo') &&
    event.target instanceof Element && event.target.closest('button, [role="button"]')
  ) return;
  if (
    uiStore.isInputActive('MessageBox') ||
    uiStore.isInputActive('FileInfo-rename') ||
    uiStore.isInputActive('AlbumFolder-new') ||
    uiStore.isInputActive('AlbumFolder-rename')
  ) return;

  emit('global-keydown', {
    key: event.key,
    code: event.code,
    altKey: event.altKey,
    ctrlKey: event.ctrlKey,
    metaKey: event.metaKey,
    shiftKey: event.shiftKey,
  });
};

function normalizeScale(value) {
  return SCALE_VALUES.find((item) => item === Number(value)) ?? 1;
}

function applyMainWindowScale(scale) {
  const normalizedScale = normalizeScale(scale);
  document.documentElement.style.fontSize = `${normalizedScale * 16}px`;
}

function handleMainWindowScaleShortcut(event) {
  const win = getCurrentWebviewWindow();
  if (win.label !== 'main') return false;

  const isScaleUp = matchesShortcut('app.scale.increase', event);
  const isScaleDown = matchesShortcut('app.scale.decrease', event);
  const isScaleReset = matchesShortcut('app.scale.reset', event);

  if (!isScaleUp && !isScaleDown && !isScaleReset) return false;

  event.preventDefault();
  event.stopPropagation();

  const currentScale = normalizeScale(config.settings.scale || 1);
  const currentIndex = SCALE_VALUES.indexOf(currentScale);
  let nextScale = currentScale;

  if (isScaleReset) {
    nextScale = 1;
  } else if (isScaleUp) {
    nextScale = SCALE_VALUES[Math.min(currentIndex + 1, SCALE_VALUES.length - 1)];
  } else if (isScaleDown) {
    nextScale = SCALE_VALUES[Math.max(currentIndex - 1, 0)];
  }

  if (nextScale !== currentScale) {
    config.setScale(nextScale);
  }
  applyMainWindowScale(nextScale);
  return true;
}

const handleContextMenu = (e) => {
  e.preventDefault();
};

</script>
