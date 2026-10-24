import { reactive } from 'vue';
import { emit } from '@tauri-apps/api/event';

export type MediaCapability = 'text' | 'image' | 'video_frames' | 'json' | 'embed_text' | 'embed_multimodal';
export interface MediaProvider {
  id: string; name: string; baseUrl: string; enabled: boolean; allowMedia: boolean;
  allowBackground: boolean; timeoutSeconds: number; credentialRevision: string;
}
export interface MediaModel {
  id: string; providerId: string; remoteId: string; name: string; group: string; revision: string;
  capabilities: MediaCapability[]; maxOutputTokens: number;
  embedding: { dimensions: number | null; queryInstruction: string };
  tested: Partial<Record<MediaCapability, string>>;
}
export interface MediaConfiguration {
  version: number; revision: number; providers: MediaProvider[]; models: MediaModel[];
  roles: { media: string | null; text: string | null; embedding: string | null };
}
export interface MediaHit {
  fileId: number; analysisId: string; title: string; description: string; start: number; end: number; score: number;
}
export const newProvider = (): MediaProvider => ({
  id: crypto.randomUUID(), name: '', baseUrl: '', enabled: true, allowMedia: false,
  allowBackground: false, timeoutSeconds: 120, credentialRevision: '',
});
export const newMediaModel = (providerId: string): MediaModel => ({
  id: crypto.randomUUID(), providerId, remoteId: '', name: '', group: '', revision: '',
  capabilities: ['text', 'image'], maxOutputTokens: 2048,
  embedding: { dimensions: null, queryInstruction: '' }, tested: {},
});
export function eligibleModels(configuration: MediaConfiguration, role: 'media' | 'text' | 'embedding') {
  const cap: MediaCapability = role === 'media' ? 'image' : role === 'text' ? 'text' : 'embed_text';
  return configuration.models.filter(m => ((m.capabilities.includes(cap) && !!m.tested[cap])
    || (role === 'media' && m.capabilities.includes('video_frames') && !!m.tested.video_frames))
    && configuration.providers.some(p => p.id === m.providerId && p.enabled && p.allowMedia));
}
/** Rank one media once, keeping its best matching video segment. */
export function rankedMediaHits(hits: MediaHit[]): MediaHit[] {
  const seen = new Set<number>();
  return [...hits].sort((a, b) => b.score - a.score).filter(hit => {
    if (seen.has(hit.fileId)) return false;
    seen.add(hit.fileId); return true;
  });
}
export const mediaAiSearch = reactive({ active: false, libraryId: '', modelId: '', albumId: null as number | null, revision: 0, error: '', hits: [] as MediaHit[] });
export const mediaAiBatch = reactive({ open: false, libraryId: '', fileIds: null as number[] | null });
export function openMediaAiBatch(libraryId: string, fileIds: number[] | null) {
  Object.assign(mediaAiBatch, { open: true, libraryId, fileIds: fileIds ? [...fileIds] : null });
}
export function openMediaAiSettings() { return emit('app-open-preferences', { aiTask: 'media' }); }
export const QWEN_MEDIA_QUERY = 'Retrieve media descriptions relevant to the search query.';
export const mediaTime = (seconds: number) => `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`;
