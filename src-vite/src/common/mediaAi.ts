import { reactive } from 'vue';
import { emit } from '@tauri-apps/api/event';

export type MediaCapability = 'text' | 'image' | 'video_frames' | 'json' | 'embed_text' | 'embed_multimodal';
export type MediaServiceKind = 'llm' | 'embedding';
export type EmbeddingProtocol = 'openai_text' | 'vllm_multimodal';
export type MediaRole = 'media' | 'text' | 'embedding' | 'multimodalEmbedding';
export interface MediaProvider {
  id: string; name: string; baseUrl: string; enabled: boolean; allowMedia: boolean;
  allowBackground: boolean; timeoutSeconds: number; credentialRevision: string;
  kind: MediaServiceKind; embeddingProtocol: EmbeddingProtocol; identityId: string;
}
export interface MediaModel {
  id: string; providerId: string; remoteId: string; name: string; group: string; revision: string;
  capabilities: MediaCapability[]; maxOutputTokens: number | null;
  embedding: { dimensions: number | null; queryInstruction: string };
  tested: Partial<Record<MediaCapability, string>>;
}
export interface MediaConfiguration {
  version: number; revision: number; providers: MediaProvider[]; models: MediaModel[];
  roles: { media: string | null; text: string | null; embedding: string | null; multimodalEmbedding: string | null };
}
export interface MediaHit {
  fileId: number; analysisId: string; title: string; description: string; start: number; end: number; score: number;
}
export const newProvider = (kind: MediaServiceKind = 'llm'): MediaProvider => ({
  id: crypto.randomUUID(), name: '', baseUrl: '', enabled: true, allowMedia: false,
  allowBackground: false, timeoutSeconds: 120, credentialRevision: '',
  kind, embeddingProtocol: 'openai_text', identityId: '',
});
export const newMediaModel = (providerId: string): MediaModel => ({
  id: crypto.randomUUID(), providerId, remoteId: '', name: '', group: '', revision: '',
  capabilities: ['text', 'image'], maxOutputTokens: 2048,
  embedding: { dimensions: null, queryInstruction: '' }, tested: {},
});
export function outputTokenError(value: unknown) {
  if (value === null || value === undefined) return null;
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 1) return 'output_tokens_invalid';
  if (value > 0xFFFFFFFF) return 'output_tokens_range';
  return null;
}
export function modelsForRole(configuration: MediaConfiguration, role: MediaRole) {
  const cap: MediaCapability = role === 'media' ? 'image' : role === 'text' ? 'text' : role === 'embedding' ? 'embed_text' : 'embed_multimodal';
  const kind: MediaServiceKind = role === 'media' || role === 'text' ? 'llm' : 'embedding';
  return configuration.models.filter(m => (m.capabilities.includes(cap) || (role === 'media' && m.capabilities.includes('video_frames')))
    && configuration.providers.some(p => p.id === m.providerId && p.kind === kind && (role !== 'multimodalEmbedding' || p.embeddingProtocol === 'vllm_multimodal')));
}
export function eligibleModels(configuration: MediaConfiguration, role: MediaRole) {
  const cap: MediaCapability = role === 'media' ? 'image' : role === 'text' ? 'text' : role === 'embedding' ? 'embed_text' : 'embed_multimodal';
  return modelsForRole(configuration, role).filter(m => ((m.capabilities.includes(cap) && !!m.tested[cap])
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
export const mediaAiSearch = reactive({ active: false, libraryId: '', modelId: '', source: 'analysis' as 'analysis' | 'media', albumId: null as number | null, revision: 0, error: '', hits: [] as MediaHit[] });
export const mediaAiBatch = reactive({ open: false, libraryId: '', fileIds: null as number[] | null });
export function openMediaAiBatch(libraryId: string, fileIds: number[] | null) {
  Object.assign(mediaAiBatch, { open: true, libraryId, fileIds: fileIds ? [...fileIds] : null });
}
export function openMediaAiSettings() { return emit('app-open-preferences', { aiTask: 'media' }); }
export const QWEN_MEDIA_QUERY = 'Retrieve media descriptions relevant to the search query.';
export const QWEN_VISUAL_QUERY = "Retrieve images or text relevant to the user's query.";
export const mediaTime = (seconds: number) => `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`;
