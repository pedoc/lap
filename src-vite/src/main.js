import { libConfig as faceLibrary } from '@/common/config'
import { recordFaceDiagnostic, faceDiagnosticNeedsAttention } from '@/common/faceDiagnostics'
import { useToast } from '@/common/toast'
import { createApp } from 'vue'
import { createI18n } from 'vue-i18n'
import { createPinia } from 'pinia'
import piniaPersistedState from 'pinia-plugin-persistedstate'
import { emit, listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { locale as getOsLocale } from '@tauri-apps/plugin-os'
import 'cally'
import router from '@/common/router'
import App from '@/App.vue'
import { useConfigStore } from '@/stores/configStore'
import '@/assets/app.css'
import { applyAiConfiguration } from '@/common/aiModels'
import { applyMapServices, migrateLegacyMapServices } from '@/common/mapServices'
import { fileInfoRevision } from '@/common/fileInfoRefresh'

// I18n
import en from '@/locales/en.json'
import de from '@/locales/de.json'
import es from '@/locales/es.json'
import fr from '@/locales/fr.json'
import it from '@/locales/it.json'
import hu from '@/locales/hu.json'
import nl from '@/locales/nl.json'
import pl from '@/locales/pl.json'
import pt from '@/locales/pt.json'
import ru from '@/locales/ru.json'
import uk from '@/locales/uk.json'
import zhCN from '@/locales/zh-CN.json'
import zhTW from '@/locales/zh-TW.json'
import ja from '@/locales/ja.json'
import ko from '@/locales/ko.json'

// Create the app instance
const app = createApp(App)

// Create Pinia store and use the persisted state plugin
const pinia = createPinia()
pinia.use(piniaPersistedState)
app.use(pinia) // Use Pinia
const config = useConfigStore() // Use the config store
const currentWindowLabel = getCurrentWebviewWindow().label
const isMainWindow = currentWindowLabel === 'main'
const isSettingsWindow = currentWindowLabel === 'settings'
async function refreshMapServices() {
  try {
    let state = await invoke('get_map_services');
    if (!state.configured && isMainWindow) {
      try { state = await invoke('save_map_services', { settings: migrateLegacyMapServices(config.settings), expectedRevision: state.revision }); }
      catch { state = await invoke('get_map_services'); }
    }
    applyMapServices(config, state);
  } catch { console.error('Failed to load map service configuration'); }
}
void refreshMapServices();
listen('map-services-changed', event => { applyMapServices(config, event.payload); });
listen('map-location-changed', event => { if (event.payload.library_id === faceLibrary._libraryId) fileInfoRevision.value++; });

let aiRefreshRequest = 0
async function refreshAiConfiguration() {
  const request = ++aiRefreshRequest
  try {
    const view = await invoke('get_ai_configuration')
    if (request === aiRefreshRequest) applyAiConfiguration(config, view)
  } catch (error) { console.error('Failed to load AI configuration:', error) }
}
void refreshAiConfiguration()
listen('ai-configuration-changed', () => { void refreshAiConfiguration() })
listen('library-switched', () => { void refreshAiConfiguration() })

// Fetch the OS locale once so "follow system" date/time formatting has a value.
void getOsLocale().then((loc) => config.setSystemLocale(loc)).catch(() => {})

if (isMainWindow) {
  config.$subscribe((_mutation, state) => {
    void emit('config-settings-synced', JSON.parse(JSON.stringify(state.settings)))
  })
} else {
  listen('config-settings-synced', (event) => {
    if (isSettingsWindow) {
      // Settings also needs language updates made from the welcome screen.
      config.setLanguage(event.payload.language)
    } else {
      Object.assign(config.settings, event.payload)
    }
  })
}

// Create the I18n instance
const i18n = createI18n({
  legacy: false, // Disable legacy mode
  locale: config.settings.language, // Use language setting from config store
  fallbackLocale: "en",
  messages: {
    en,
    de,
    es,
    fr,
    it,
    hu,
    nl,
    pl,
    pt,
    ru,
    uk,
    zh: zhCN,
    'zh-TW': zhTW,
    ja,
    ko
  },
})

listen('face-person-changed', event => {
  const change = event.payload || {}
  if (change.library_id !== faceLibrary._libraryId) return
  if (change.mode === 'rename' && faceLibrary.person?.id === change.personId) faceLibrary.person.name = change.name || null
})

// Face jobs are background operations; errors must not disappear outside the People panel.
if (!isSettingsWindow) {
  listen('face_index_finished', event => {
    const result = event.payload || {}
    if (result.library_id !== faceLibrary._libraryId) return
    recordFaceDiagnostic(result, faceLibrary._libraryId)
    const toast = useToast()
    if (result.error) toast.error(String(result.error))
    else if (result.cancelled) toast.info(i18n.global.t('face_actions.cancelled'))
    else {
      const message = result.diagnostics
        ? i18n.global.t('face_diagnostics.toast', result.diagnostics)
        : i18n.global.t('face_actions.finished', { faces: result.total_faces || 0, cached: result.cached || 0, failed: result.failed || 0 })
      if (faceDiagnosticNeedsAttention(result)) toast.warning(message)
      else toast.success(message)
    }
  })
}

// Set up global properties
app.config.globalProperties.$invoke = invoke

// Use the router and i18n
app.use(router)
app.use(i18n)

// Mount the app
app.mount('#app')

// Listen for events
if (isMainWindow) {
  listen('settings-appearance-changed', (event) => {
    config.setAppearance(event.payload)
  })
  listen('settings-lightTheme-changed', (event) => {
    config.setLightTheme(event.payload)
  })
  listen('settings-darkTheme-changed', (event) => {
    config.setDarkTheme(event.payload)
  })
  listen('settings-scale-changed', (event) => {
    config.setScale(event.payload)
  })
  listen('settings-externalApps-changed', (event) => {
    config.setExternalApps(event.payload)
  })
  listen('settings-language-changed', (event) => {
    config.setLanguage(event.payload)
  })
  listen('settings-showToolTip-changed', (event) => {
    config.setShowToolTip(event.payload)
  })
  listen('settings-showStatusBar-changed', (event) => {
    config.setShowStatusBar(event.payload)
  })
  listen('settings-autoCheckUpdates-changed', (event) => {
    config.setAutoCheckUpdates(event.payload)
  })
  listen('settings-debugMode-changed', (event) => {
    config.setDebugMode(event.payload)
  })
  listen('settings-settingsTabIndex-changed', (event) => {
    config.setSettingsTabIndex(event.payload)
  })
  listen('settings-folderSort-changed', (event) => {
    config.setFolderSort(event.payload)
  })
  listen('settings-dateTimeFormat-changed', (event) => {
    config.setDateTimeFormat(event.payload)
  })
  listen('settings-showSubfolderFiles-changed', (event) => {
    config.setShowSubfolderFiles(event.payload)
  })
  listen('settings-thumbnailSize-changed', (event) => {
    config.setThumbnailSize(event.payload)
  })
  listen('settings-rawPairDisplaySource-changed', (event) => {
    config.setRawPairDisplaySource(event.payload)
  })
  listen('settings-rawPreviewSource-changed', (event) => {
    config.setRawPreviewSource(event.payload)
  })
  listen('settings-rawRenderBrightness-changed', (event) => {
    config.setRawRenderBrightness(event.payload)
  })

  listen('settings-mapProvider-changed', (event) => {
    config.setMapProvider(event.payload)
  })
  listen('settings-tiandituToken-changed', (event) => {
    config.setTiandituToken(event.payload)
  })
  listen('settings-mapMarkerSize-changed', (event) => {
    config.setMapMarkerSize(event.payload)
  })
  listen('settings-gridStyle-changed', (event) => {
    config.setGridStyle(event.payload)
  })
  listen('settings-gridScaling-changed', (event) => {
    config.setGridScaling(event.payload)
  })
  listen('settings-gridThumbnailCorners-changed', (event) => {
    config.setGridThumbnailCorners(event.payload)
  })
  listen('settings-gridLabelPrimary-changed', (event) => {
    config.setGridLabelPrimary(event.payload)
  })
  listen('settings-gridLabelSecondary-changed', (event) => {
    config.setGridLabelSecondary(event.payload)
  })
  listen('settings-gridThumbnailBadge-changed', (event) => {
    config.setGridThumbnailBadge(event.payload)
  })
  listen('settings-filmStripViewPreviewPosition-changed', (event) => {
    config.setFilmStripViewPreviewPosition(event.payload)
  })
  listen('settings-mouseWheelMode-changed', (event) => {
    config.setMouseWheelMode(event.payload)
  })
  listen('settings-slideShowInterval-changed', (event) => {
    config.setSlideShowInterval(event.payload)
  })
  listen('settings-autoPlayVideo-changed', (event) => {
    config.setAutoPlayVideo(event.payload)
  })
  listen('settings-loopVideo-changed', (event) => {
    config.settings.loopVideo = event.payload
  })
  listen('settings-groupRawJpegPairs-changed', (event) => {
    config.settings.groupRawJpegPairs = event.payload
  })
  listen('settings-navigatorViewMode-changed', (event) => {
    config.setNavigatorViewMode(event.payload)
  })
  listen('settings-navigatorViewSize-changed', (event) => {
    config.setNavigatorViewSize(event.payload)
  })
  listen('settings-navigatorSharpnessGrid-changed', (event) => {
    config.settings.navigatorSharpnessGrid = event.payload
  })
  listen('settings-viewBackground-changed', (event) => {
    config.setViewBackground(event.payload)
  })
  listen('settings-slideShowTransition-changed', (event) => {
    config.setSlideShowTransition(event.payload)
  })
  listen('settings-imageSearchThresholdIndex-changed', (event) => {
    config.setImageSearchThresholdIndex(event.payload)
  })
  listen('settings-similarPhotoGroupingThresholdIndex-changed', (event) => {
    config.setSimilarPhotoGroupingThresholdIndex(event.payload)
  })
  listen('settings-faceEnabled-changed', (event) => {
    config.setFaceEnabled(event.payload)
  })
  listen('libraries-changed', () => {
    config.notifyLibrariesChanged()
  })
}
