import { reactive } from 'vue';
import { emit } from '@tauri-apps/api/event';

export type MediaCapability = 'text' | 'image' | 'video_frames' | 'json' | 'embed_text' | 'embed_multimodal';
export type MediaServiceKind = 'llm' | 'embedding';
export type EmbeddingProtocol = 'openai_text' | 'vllm_multimodal' | 'dashscope_multimodal' | 'ark_multimodal' | 'siliconflow_multimodal';
export const EMBEDDING_PROTOCOLS: Record<EmbeddingProtocol, { label: string; hint: string; modelList: boolean; queryInstruction: boolean; queryPresets: boolean }> = {
  openai_text: { label: 'protocol_openai_text', hint: 'embedding_hint', modelList: true, queryInstruction: true, queryPresets: true },
  vllm_multimodal: { label: 'protocol_vllm_multimodal', hint: 'multimodal_protocol_hint', modelList: true, queryInstruction: true, queryPresets: true },
  dashscope_multimodal: { label: 'protocol_dashscope_multimodal', hint: 'dashscope_protocol_hint', modelList: false, queryInstruction: true, queryPresets: false },
  ark_multimodal: { label: 'protocol_ark_multimodal', hint: 'ark_protocol_hint', modelList: false, queryInstruction: true, queryPresets: false },
  siliconflow_multimodal: { label: 'protocol_siliconflow_multimodal', hint: 'siliconflow_protocol_hint', modelList: true, queryInstruction: false, queryPresets: false },
};
export const EMBEDDING_SERVICE_PRESETS: { id: string; label: string; baseUrl: string; protocol: EmbeddingProtocol; modelId: string }[] = [
  { id: 'dashscope_text', label: 'preset_dashscope_text', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', protocol: 'openai_text', modelId: 'text-embedding-v4' },
  { id: 'dashscope_multimodal', label: 'preset_dashscope_multimodal', baseUrl: 'https://dashscope.aliyuncs.com/api/v1', protocol: 'dashscope_multimodal', modelId: 'qwen3-vl-embedding' },
  { id: 'ark_multimodal', label: 'preset_ark_multimodal', baseUrl: 'https://ark.cn-beijing.volces.com/api/v3', protocol: 'ark_multimodal', modelId: 'doubao-embedding-vision-251215' },
  { id: 'siliconflow_text', label: 'preset_siliconflow_text', baseUrl: 'https://api.siliconflow.cn/v1', protocol: 'openai_text', modelId: 'BAAI/bge-m3' },
  { id: 'siliconflow_multimodal', label: 'preset_siliconflow_multimodal', baseUrl: 'https://api.siliconflow.cn/v1', protocol: 'siliconflow_multimodal', modelId: 'Qwen/Qwen3-VL-Embedding-8B' },
];
export type MediaRole = 'media' | 'text' | 'embedding' | 'multimodalEmbedding';
export interface MediaProvider {
  id: string; name: string; baseUrl: string; enabled: boolean; allowMedia: boolean;
  allowBackground: boolean; timeoutSeconds: number; credentialRevision: string;
  kind: MediaServiceKind; embeddingProtocol: EmbeddingProtocol; identityId: string;
  openaiEmbeddings: boolean;
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
  kind, embeddingProtocol: 'openai_text', identityId: '', openaiEmbeddings: false,
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
  return configuration.models.filter(m => (m.capabilities.includes(cap) || (role === 'media' && m.capabilities.includes('video_frames')))
    && configuration.providers.some(p => p.id === m.providerId && (role === 'embedding'
      ? p.kind === 'embedding' || p.kind === 'llm' && p.openaiEmbeddings && p.embeddingProtocol === 'openai_text' && isTextEmbeddingModel(m)
      : role === 'multimodalEmbedding' ? p.kind === 'embedding' && p.embeddingProtocol !== 'openai_text' : p.kind === 'llm')));
}
export function isTextEmbeddingModel(model: MediaModel) { return model.capabilities.length === 1 && model.capabilities[0] === 'embed_text'; }
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
