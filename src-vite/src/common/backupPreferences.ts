// Only user preferences belong in a backup; derived AI/map state, active jobs,
// library identities and transient window state must be reloaded locally.
const GROUPS = ['settings', 'video', 'search', 'content', 'leftPanel', 'rightPanel', 'infoPanel', 'collectionTray', 'imageEditor', 'montage'];
const OMIT = new Set(['ai', 'mapServices', 'mapServicesRevision', 'sdkProxyRevision', 'tabIndex', '__proto__', 'constructor', 'prototype']);

function clean(value: any): any {
  if (Array.isArray(value)) return value.map(clean);
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).filter(([key]) => !OMIT.has(key)).map(([key, item]) => [key, clean(item)]));
  }
  return value;
}

export function backupPreferences(state: any) {
  return Object.fromEntries(GROUPS.filter(key => state[key] && typeof state[key] === 'object').map(key => [key, clean(state[key])]));
}

function compatible(current: any, incoming: any): any {
  if (Array.isArray(current)) return Array.isArray(incoming) ? clean(incoming) : current;
  if (current && typeof current === 'object' && incoming && typeof incoming === 'object' && !Array.isArray(incoming)) {
    return Object.fromEntries(Object.entries(current).map(([key, value]) => [key,
      !OMIT.has(key) && Object.hasOwn(incoming, key) ? compatible(value, incoming[key]) : value,
    ]));
  }
  return current !== null && typeof incoming === typeof current && (typeof incoming !== 'number' || Number.isFinite(incoming)) ? incoming : current;
}

export function applyBackupPreferences(store: any, preferences: any) {
  if (!preferences || typeof preferences !== 'object' || Array.isArray(preferences)) throw new Error('Invalid backup preferences');
  const patch = Object.fromEntries(GROUPS.filter(key => Object.hasOwn(preferences, key) && store.$state[key])
    .map(key => [key, compatible(store.$state[key], preferences[key])]));
  store.$patch(patch);
  // External applications are added lazily and may not exist in a fresh store.
  if (preferences.settings?.externalApps && typeof store.setExternalApps === 'function') {
    store.setExternalApps(clean(preferences.settings.externalApps));
  }
  store.$persist();
}
