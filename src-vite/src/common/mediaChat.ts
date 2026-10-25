import { reactive } from 'vue';

export interface ChatCandidate {
  fileId: number; name: string; folderPath: string; fileType: number;
  width: number; height: number; duration: number; cloudOnly: boolean;
  hasAnalysis: boolean; sourceStamp: string;
}
export interface ChatEvidence { analysisId: string; start: number; end: number; title: string; description: string; tags: string[] }
export interface ChatSource {
  fileId: number; path: string; modified: number | null; size: number; orientation: number;
  kind: number; width: number; height: number; duration: number; title: string | null;
  comments: string | null; tagIds: number[]; cloudOnly: boolean;
}
export interface ChatReference {
  fileId: number; name: string; fileType: number; sourceStamp: string;
  start: number; end: number; analysisIds: string[]; source: ChatSource; evidence: ChatEvidence[];
}
export interface ChatSession {
  id: string; libraryId: string; title: string; modelId: string | null;
  createdAt: number; updatedAt: number; messageCount: number; lastMessage: string;
  runningRequestId: string | null;
}
export interface ChatMessage {
  id: string; role: 'user' | 'assistant'; status: 'pending' | 'completed' | 'failed' | 'cancelled';
  content: string; modelId: string | null; modelName: string; visual: boolean;
  reasoningEffort: string | null;
  references: ChatReference[]; error: string | null; requestId: string | null; createdAt: number;
}
export interface ChatSessionView { session: ChatSession; messages: ChatMessage[] }
export interface ChatSendReference { fileId: number; start?: number | null }

// Shared within the active WebView: opening chat never opens or focuses another app window.
export const mediaChat = reactive({ open: false, libraryId: '', fileIds: [] as number[], revision: 0 });
export const mediaChatFloatingReserve = reactive({ bottom: 0 });
export const MEDIA_CHAT_DROP_EVENT = 'lap-media-chat-files';
export function validMediaChatIds(values: number[]): number[] {
  return [...new Set(values.filter(id => Number.isSafeInteger(id) && id > 0))];
}
export function openMediaChat(libraryId: string, fileIds: number[] = []) {
  if (!libraryId) return;
  mediaChat.libraryId = libraryId;
  mediaChat.fileIds = validMediaChatIds(fileIds);
  mediaChat.revision++;
  mediaChat.open = true;
}
export function closeMediaChat() { mediaChat.open = false; }
