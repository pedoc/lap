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
export const EMBEDDING_SERVICE_PRESETS: Omit<BuiltinServicePreset, 'kind'>[] = [
  { id: 'dashscope_text', label: 'preset_dashscope_text', baseUrl: 'https://your-workspace-id.cn-beijing.maas.aliyuncs.com/compatible-mode/v1', protocol: 'openai_text', modelId: 'text-embedding-v4', modelList: false },
  { id: 'dashscope_multimodal', label: 'preset_dashscope_multimodal', baseUrl: 'https://your-workspace-id.cn-beijing.maas.aliyuncs.com/api/v1', protocol: 'dashscope_multimodal', modelId: 'qwen3-vl-embedding' },
  { id: 'ark_multimodal', label: 'preset_ark_multimodal', baseUrl: 'https://ark.cn-beijing.volces.com/api/v3', protocol: 'ark_multimodal', modelId: 'doubao-embedding-vision-251215' },
  { id: 'ark_agent_plan', label: 'preset_ark_agent_plan_embedding', baseUrl: 'https://ark.cn-beijing.volces.com/api/plan/v3', protocol: 'ark_multimodal', modelId: 'doubao-embedding-vision', modelList: false, accessMode: 'agent_plan', usageHint: 'access_ark_agent_embedding_hint' },
  { id: 'siliconflow_text', label: 'preset_siliconflow_text', baseUrl: 'https://api.siliconflow.cn/v1', protocol: 'openai_text', modelId: 'BAAI/bge-m3' },
  { id: 'siliconflow_multimodal', label: 'preset_siliconflow_multimodal', baseUrl: 'https://api.siliconflow.cn/v1', protocol: 'siliconflow_multimodal', modelId: 'Qwen/Qwen3-VL-Embedding-8B' },
];
export interface BuiltinServicePreset {
  id: string; kind: MediaServiceKind; label: string; baseUrl: string; protocol: EmbeddingProtocol; modelId: string;
  capabilities?: MediaCapability[]; modelList?: boolean; deduplicateByEndpoint?: boolean;
  accessMode?: 'payg' | 'agent_plan' | 'coding_plan' | 'token_personal' | 'token_team'; usageHint?: string;
}
export const LLM_SERVICE_PRESETS: BuiltinServicePreset[] = [
  { id: 'builtin-llm-dashscope', kind: 'llm', label: 'preset_llm_dashscope', baseUrl: 'https://your-workspace-id.cn-beijing.maas.aliyuncs.com/compatible-mode/v1', protocol: 'openai_text', modelId: 'qwen3.8-max', capabilities: ['text', 'image'], modelList: false, accessMode: 'payg', usageHint: 'access_payg_hint' },
  { id: 'builtin-llm-dashscope-coding-plan', kind: 'llm', label: 'preset_llm_dashscope_coding_plan', baseUrl: 'https://coding.dashscope.aliyuncs.com/v1', protocol: 'openai_text', modelId: 'qwen3.7-plus', capabilities: ['text', 'image'], modelList: false, accessMode: 'coding_plan', usageHint: 'access_dashscope_coding_hint' },
  { id: 'builtin-llm-dashscope-token-personal', kind: 'llm', label: 'preset_llm_dashscope_token_personal', baseUrl: 'https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1', protocol: 'openai_text', modelId: 'qwen3.8-max', capabilities: ['text', 'image'], modelList: false, deduplicateByEndpoint: false, accessMode: 'token_personal', usageHint: 'access_token_personal_hint' },
  { id: 'builtin-llm-dashscope-token-team', kind: 'llm', label: 'preset_llm_dashscope_token_team', baseUrl: 'https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1', protocol: 'openai_text', modelId: 'qwen3.8-max', capabilities: ['text', 'image'], modelList: false, deduplicateByEndpoint: false, accessMode: 'token_team', usageHint: 'access_token_team_hint' },
  { id: 'builtin-llm-siliconflow', kind: 'llm', label: 'preset_llm_siliconflow', baseUrl: 'https://api.siliconflow.cn/v1', protocol: 'openai_text', modelId: 'deepseek-ai/DeepSeek-V4-Flash', accessMode: 'payg', usageHint: 'access_standard_api_hint' },
  { id: 'builtin-llm-ark', kind: 'llm', label: 'preset_llm_ark', baseUrl: 'https://ark.cn-beijing.volces.com/api/v3', protocol: 'openai_text', modelId: 'doubao-seed-2-1-pro-260915', capabilities: ['text', 'image', 'video_frames'], modelList: false, accessMode: 'payg', usageHint: 'access_payg_hint' },
  { id: 'builtin-llm-ark-agent-plan', kind: 'llm', label: 'preset_llm_ark_agent_plan', baseUrl: 'https://ark.cn-beijing.volces.com/api/plan/v3', protocol: 'openai_text', modelId: 'ark-code-latest', modelList: false, accessMode: 'agent_plan', usageHint: 'access_ark_agent_hint' },
  { id: 'builtin-llm-ark-coding-plan', kind: 'llm', label: 'preset_llm_ark_coding_plan', baseUrl: 'https://ark.cn-beijing.volces.com/api/coding/v3', protocol: 'openai_text', modelId: 'ark-code-latest', modelList: false, accessMode: 'coding_plan', usageHint: 'access_ark_coding_hint' },
  { id: 'builtin-llm-deepseek', kind: 'llm', label: 'preset_llm_deepseek', baseUrl: 'https://api.deepseek.com/v1', protocol: 'openai_text', modelId: 'deepseek-flash', capabilities: ['text', 'image'], modelList: false, accessMode: 'payg', usageHint: 'access_standard_api_hint' },
  { id: 'builtin-llm-longcat', kind: 'llm', label: 'preset_llm_longcat', baseUrl: 'https://api.longcat.chat/openai/v1', protocol: 'openai_text', modelId: 'LongCat-2.0', accessMode: 'payg', usageHint: 'access_standard_api_hint' },
];
export const BUILTIN_SERVICE_PRESETS: BuiltinServicePreset[] = [
  ...LLM_SERVICE_PRESETS,
  ...EMBEDDING_SERVICE_PRESETS.map(preset => ({ ...preset, id: `builtin-embedding-${preset.id.replaceAll('_', '-')}`, kind: 'embedding' as const, accessMode: preset.accessMode || 'payg', usageHint: preset.usageHint || 'access_payg_hint' })),
];
const BUILTIN_SERVICE_IDS = new Set(BUILTIN_SERVICE_PRESETS.map(preset => preset.id));
export const isBuiltinService = (provider: MediaProvider) => BUILTIN_SERVICE_IDS.has(provider.id);
export function hasWorkspacePlaceholder(value: string) { try { return new URL(value).hostname.startsWith('your-workspace-id.'); } catch { return false; } }
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
  capabilities: ['text', 'image'], maxOutputTokens: null,
  embedding: { dimensions: null, queryInstruction: '' }, tested: {},
});
function serviceEndpoint(value: string) {
  try {
    const url = new URL(value.trim());
    let path = url.pathname.replace(/\/+$/, '');
    if (!/\/v\d+$/.test(path)) path += '/v1';
    return `${url.origin}${path}${url.search}${url.hash}`;
  } catch { return value.trim().replace(/\/+$/, ''); }
}
export function appendBuiltinServices(configuration: MediaConfiguration, label: (key: string) => string) {
  for (const preset of BUILTIN_SERVICE_PRESETS) {
    if (configuration.providers.some(p => p.id === preset.id)) continue;
    if (preset.deduplicateByEndpoint !== false && configuration.providers.some(p => (p.kind === preset.kind || preset.kind === 'embedding' && preset.protocol === 'openai_text' && p.kind === 'llm' && p.openaiEmbeddings)
      && (p.kind === 'llm' ? 'openai_text' : p.embeddingProtocol) === preset.protocol && serviceEndpoint(p.baseUrl) === serviceEndpoint(preset.baseUrl))) continue;
    const provider = { ...newProvider(preset.kind), id: preset.id, name: label(preset.label), baseUrl: preset.baseUrl, embeddingProtocol: preset.protocol, enabled: false };
    configuration.providers.push(provider);
    if (!preset.modelId) continue;
    const model = { ...newMediaModel(provider.id), id: `${preset.id}-example`, remoteId: preset.modelId, name: preset.modelId };
    if (configuration.models.some(current => current.id === model.id)) continue;
    model.capabilities = preset.kind === 'llm' ? [...(preset.capabilities || ['text'])] : preset.protocol === 'openai_text' ? ['embed_text'] : ['embed_text', 'embed_multimodal'];
    model.maxOutputTokens = null;
    configuration.models.push(model);
  }
}
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
