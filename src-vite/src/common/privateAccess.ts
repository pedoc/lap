import { computed, reactive, ref, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// Tokens are RAM-only. Polling checks expiry but never counts as user activity.
export function createPrivateAccess(
  library: () => string,
  invalidateView: () => void,
  deps = { invoke, listen, now: Date.now, setInterval, clearInterval },
) {
  const state = reactive({
    ready: false,
    enabled: false,
    locked: true,
    idleMinutes: 5,
    lockOnBlur: true,
    retryAfter: 0,
    busy: false,
    error: '',
  });
  const sessionToken = ref<string | null>(null);
  const generation = ref(0);
  const canRead = computed(
    () =>
      state.ready &&
      (!state.enabled || (!state.locked && !!sessionToken.value)),
  );
  let disposed = false;
  let refreshing = false;
  let refreshAgain = false;
  let scope = '';
  let request = 0;
  let lastActivity = -Infinity;
  let timer: ReturnType<typeof setInterval> | null = null;
  let stop: (() => void) | null = null;
  function invalidate() {
    generation.value++;
    request++;
    sessionToken.value = null;
    state.locked = true;
    invalidateView();
  }
  function apply(result: any) {
    state.enabled = result.enabled;
    state.locked = result.locked;
    state.idleMinutes = result.idleMinutes;
    state.lockOnBlur = result.lockOnBlur;
    state.retryAfter = result.retryAfter || 0;
    state.ready = true;
    if (result.locked) {
      sessionToken.value = null;
      invalidateView();
    }
  }
  async function refresh() {
    if (disposed) return;
    if (refreshing) {
      refreshAgain = true;
      return;
    }
    refreshing = true;
    const id = library();
    const token = ++request;
    const epoch = generation.value;
    try {
      const result: any = await deps.invoke('private_lock_status', {
        libraryId: id,
        sessionToken: sessionToken.value,
        beginView: !state.ready && !sessionToken.value,
      });
      if (
        disposed ||
        token !== request ||
        epoch !== generation.value ||
        id !== library()
      )
        return;
      if (result.locked && canRead.value) invalidate();
      if (!state.ready) state.error = '';
      if (result.sessionToken) sessionToken.value = result.sessionToken;
      apply(result);
    } catch (reason) {
      if (!disposed && token === request && id === library()) {
        invalidate();
        state.ready = false;
        state.error = String(reason);
      }
    } finally {
      refreshing = false;
      if (refreshAgain && !disposed) {
        refreshAgain = false;
        void refresh();
      }
    }
  }
  function scopeChanged() {
    if (scope === library()) return;
    scope = library();
    invalidate();
    state.ready = false;
    state.busy = false;
    state.error = '';
    void refresh();
  }
  async function unlock(password: string) {
    if (state.busy) return;
    const id = library();
    const epoch = generation.value;
    state.busy = true;
    state.error = '';
    try {
      const result: any = await deps.invoke('unlock_private_view', {
        libraryId: id,
        password,
      });
      if (disposed || epoch !== generation.value || id !== library()) return;
      generation.value++;
      sessionToken.value = result.sessionToken;
      apply(result);
      lastActivity = deps.now();
    } catch (reason) {
      if (!disposed && epoch === generation.value && id === library()) {
        state.error = String(reason);
        await refresh();
      }
    } finally {
      if (!disposed && id === library()) state.busy = false;
    }
  }
  async function configure(settings: {
    currentPassword: string;
    newPassword: string | null;
    enabled: boolean;
    idleMinutes: number;
    lockOnBlur: boolean;
  }) {
    if (state.busy) return;
    const id = library();
    const epoch = generation.value;
    state.busy = true;
    state.error = '';
    try {
      await deps.invoke('configure_private_lock', {
        ...settings,
        libraryId: id,
      });
      if (disposed || id !== library()) return;
      invalidate();
      state.ready = false;
      await refresh();
    } catch (reason) {
      if (!disposed && epoch === generation.value && id === library()) {
        state.error = String(reason);
        await refresh();
      }
    } finally {
      if (!disposed && id === library()) state.busy = false;
    }
  }
  async function lockNow() {
    const id = library();
    const token = sessionToken.value;
    invalidate();
    try {
      await deps.invoke('lock_private_view', {
        libraryId: id,
        sessionToken: token,
      });
    } catch (reason) {
      if (!disposed && id === library()) {
        state.ready = false;
        state.error = String(reason);
      }
    }
  }
  async function activity() {
    if (!canRead.value || !state.enabled || deps.now() - lastActivity < 5000)
      return;
    lastActivity = deps.now();
    const epoch = generation.value;
    const id = library();
    try {
      await deps.invoke('touch_private_view', {
        libraryId: id,
        sessionToken: sessionToken.value,
      });
    } catch {
      if (!disposed && epoch === generation.value && id === library()) {
        invalidate();
        void refresh();
      }
    }
  }
  async function start() {
    const unlisten = await deps.listen('private-lock-changed', () => {
      if (!disposed) {
        invalidate();
        state.ready = false;
        void refresh();
      }
    });
    if (disposed) {
      unlisten();
      return;
    }
    stop = unlisten;
    timer = deps.setInterval(() => {
      if (
        state.enabled &&
        !state.busy &&
        (sessionToken.value || state.retryAfter)
      )
        void refresh();
    }, 1000);
  }
  function dispose() {
    if (disposed) return;
    const id = scope || library();
    const token = sessionToken.value;
    disposed = true;
    invalidate();
    state.ready = false;
    state.error = '';
    if (timer !== null) deps.clearInterval(timer);
    stop?.();
    // Scope/token matching on the backend prevents this delayed logout revoking a newer unlock.
    void deps
      .invoke('lock_private_view', { libraryId: id, sessionToken: token })
      .catch(() => {});
  }
  return {
    state,
    sessionToken,
    generation,
    canRead,
    refresh,
    scopeChanged,
    unlock,
    configure,
    lockNow,
    activity,
    start,
    dispose,
  };
}
export function usePrivateAccess(
  library: () => string,
  invalidate: () => void,
) {
  const access = createPrivateAccess(library, invalidate);
  const activity = () => {
    void access.activity();
  };
  const blur = () => {
    if (access.state.enabled && access.state.lockOnBlur) void access.lockNow();
  };
  const visibility = () => {
    if (document.visibilityState === 'hidden') blur();
  };
  onMounted(() => {
    void access.start().catch(() => {
      void access.lockNow();
    });
    window.addEventListener('blur', blur);
    document.addEventListener('visibilitychange', visibility);
    document.addEventListener('pointerdown', activity, true);
    document.addEventListener('pointermove', activity, true);
    document.addEventListener('keydown', activity, true);
  });
  onBeforeUnmount(() => {
    window.removeEventListener('blur', blur);
    document.removeEventListener('visibilitychange', visibility);
    document.removeEventListener('pointerdown', activity, true);
    document.removeEventListener('pointermove', activity, true);
    document.removeEventListener('keydown', activity, true);
    access.dispose();
  });
  return access;
}
