import { nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { libConfig } from './config';
import { i18n } from './i18n';
import { pendingChanges } from './pendingChanges';
import { closeMediaChat, mediaChat } from './mediaChat';

let navigationGeneration = 0;
export async function openMediaChatMedia(libraryId: string, fileId: number, start?: number): Promise<boolean> {
  const revision = mediaChat.revision;
  if (!libraryId || libraryId !== libConfig._libraryId || !Number.isSafeInteger(fileId) || fileId <= 0) return false;
  const generation = ++navigationGeneration;
  const current = () => generation === navigationGeneration && libraryId === libConfig._libraryId && revision === mediaChat.revision;
  const files = await invoke<any[]>('get_home_preview_files', { libraryId, fileIds: [fileId] });
  if (!current()) return false;
  if (!files.some(file => Number(file.id) === fileId)) throw new Error(i18n.global.t('media_chat.media_unavailable'));
  const main = await WebviewWindow.getByLabel('main');
  if (!main) throw new Error(i18n.global.t('media_chat.media_unavailable'));
  if (!current()) return false;
  if (!await pendingChanges.confirm('media-title') || !current()) return false;
  const wasOpen = mediaChat.open;
  closeMediaChat();
  await nextTick(); // The viewer must receive navigation after chat releases its keyboard handler.
  if (!current()) return false;
  try {
    await main.emit('message-from-image-viewer', {
      message: 'open-chat-media', libraryId, fileId,
      start: typeof start === 'number' && Number.isFinite(start) && start >= 0 ? start : null,
    });
    return true;
  } catch (error) {
    if (wasOpen && current()) mediaChat.open = true;
    throw error;
  }
}
