import { emit, emitTo, listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { getAllWebviewWindows, getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { ask } from '@tauri-apps/plugin-dialog';
import { pendingChanges, coordinateClose } from './pendingChanges';
import { objectEdit, hasObjectEditChanges } from './objectEditing';
import { useToast } from './toast';

export async function installDesktopCloseProtection(t: (key: string) => string) {
  const window = getCurrentWebviewWindow(), label = window.label;
  const tokens = new Map<string, NonNullable<Awaited<ReturnType<typeof pendingChanges.prepare>>>>();
  const cancelled = new Set<string>();
  const votes = new Map<string, (approved: boolean) => void>();
  let localClose: Promise<boolean> | null = null, quitting: Promise<void> | null = null;
  let blocker: HTMLElement | null = null;
  const block = (event: Event) => { if (tokens.size) { event.preventDefault(); event.stopImmediatePropagation(); } };
  function updateLock() {
    if (tokens.size && !blocker) {
      blocker = document.createElement('div');
      blocker.className = 'fixed inset-0 z-[2500] flex items-center justify-center bg-base-100/70 text-base-content p-6';
      blocker.setAttribute('role', 'status'); blocker.textContent = t('workflow_close.preparing');
      document.body.appendChild(blocker);
      document.addEventListener('pointerdown', block, true); document.addEventListener('keydown', block, true);
    } else if (!tokens.size && blocker) {
      blocker.remove(); blocker = null;
      document.removeEventListener('pointerdown', block, true); document.removeEventListener('keydown', block, true);
    }
  }
  async function prepare(id: string) {
    if (cancelled.has(id)) return false;
    if (tokens.has(id)) return true;
    if (tokens.size) return false;
    if (pendingChanges.hasPending()) { await window.show(); await window.setFocus(); }
    const token = await pendingChanges.prepare();
    if (!token || cancelled.has(id)) return false;
    tokens.set(id, token); updateLock(); return true;
  }
  function release(id: string) { cancelled.add(id); tokens.delete(id); updateLock(); }
  const valid = (id: string) => !!tokens.get(id) && pendingChanges.valid(tokens.get(id)!);
  pendingChanges.register('object-annotations', {
    dirty: () => hasObjectEditChanges(), busy: () => !!objectEdit.value?.busy,
    revision: () => JSON.stringify([objectEdit.value?.libraryId, objectEdit.value?.fileId, objectEdit.value?.boxes]),
    confirm: () => ask(t('workflow_close.unsaved'), { kind: 'warning' }),
  });
  await listen<any>('app-close-check', async event => {
    const { requestId, stage } = event.payload || {};
    if (typeof requestId !== 'string' || !['prepare', 'validate', 'release'].includes(stage)) return;
    if (stage === 'release') { release(requestId); return; }
    const approved = stage === 'prepare' ? await prepare(requestId) : valid(requestId);
    await emit('app-close-vote', { requestId, label, stage, approved });
  });
  await listen<any>('app-close-vote', event => {
    const p = event.payload;
    votes.get(`${p?.requestId}:${p?.label}:${p?.stage}`)?.(p.approved === true);
  });
  async function peer(target: string, id: string, stage: string) {
    const key = `${id}:${target}:${stage}`;
    return new Promise<boolean>((resolve, reject) => {
      // An unresponsive window cancels closing; it never grants permission.
      const timer = setTimeout(() => { votes.delete(key); reject(new Error('A window did not respond to the close request')); }, 120_000);
      votes.set(key, approved => { clearTimeout(timer); votes.delete(key); resolve(approved); });
      void emitTo(target, 'app-close-check', { requestId: id, stage }).catch(reason => { clearTimeout(timer); votes.delete(key); reject(reason); });
    });
  }
  async function quit(id: string) {
    let committed = false;
    try {
      const windows = await getAllWebviewWindows();
      const labels = windows.map(item => item.label);
      const participants = labels.map(target => target === label ? {
        prepare: () => prepare(id), validate: async () => valid(id), release: () => release(id),
      } : {
        prepare: () => peer(target, id, 'prepare'), validate: () => peer(target, id, 'validate'),
        release: () => emitTo(target, 'app-close-check', { requestId: id, stage: 'release' }),
      });
      committed = await coordinateClose(participants, async () => {
        const now = await getAllWebviewWindows();
        if (now.some(item => !labels.includes(item.label))) throw new Error('A window was opened during exit confirmation');
        await invoke('confirm_app_exit', { requestId: id });
      });
    } catch { useToast().error(t('workflow_close.failed')); }
    finally { if (!committed) await invoke('cancel_app_exit', { requestId: id }).catch(() => {}); }
  }
  if (label === 'main') {
    await listen<{ requestId: string }>('app-request-quit', event => {
      if (typeof event.payload?.requestId !== 'string') return;
      if (!quitting) quitting = quit(event.payload.requestId).finally(() => { quitting = null; });
      return quitting;
    });
    const pending = await invoke<string | null>('get_pending_app_exit');
    if (pending && !quitting) quitting = quit(pending).finally(() => { quitting = null; });
  } else {
    await window.onCloseRequested(event => {
      event.preventDefault();
      if (localClose || tokens.size) return;
      const id = crypto.randomUUID();
      localClose = (async () => {
        try {
          if (!await prepare(id) || !valid(id)) return false;
          await window.destroy(); return true;
        } catch { useToast().error(t('workflow_close.failed')); return false; }
        finally { release(id); }
      })().finally(() => { localClose = null; });
      return localClose;
    });
  }
  return { quit, prepare, release, valid };
}
