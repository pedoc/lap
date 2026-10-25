// Keep vendor abbreviations verbatim when their documentation does not define an exact token count.
export type TokenLimit = number | string | null;
export interface ModelSpec {
  ids: readonly string[];
  contextTokens: TokenLimit;
  inputTokens: TokenLimit;
  outputTokens: TokenLimit;
  source: string;
  verifiedAt: string;
  notes?: 'qwen_thinking' | 'dynamic_alias' | 'weights_capacity';
  reasoning?: { levels: readonly string[]; defaultEffort: string | null; source?: string };
}

export const MODEL_SPECS: readonly ModelSpec[] = [
  { ids: ['gpt-6.1-sol'], contextTokens: 1050000, inputTokens: null, outputTokens: 128000, source: 'https://developers.openai.com/api/docs/models/gpt-6.1-sol', verifiedAt: '2026-10-25', reasoning: { levels: ['low', 'medium', 'high', 'xhigh', 'max'], defaultEffort: 'medium' } },
  { ids: ['gpt-6-sol'], contextTokens: 1050000, inputTokens: null, outputTokens: 128000, source: 'https://developers.openai.com/api/docs/models/gpt-6-sol', verifiedAt: '2026-10-25', reasoning: { levels: ['none', 'low', 'medium', 'high', 'xhigh', 'max'], defaultEffort: 'medium' } },
  { ids: ['gpt-6-astra'], contextTokens: 1050000, inputTokens: null, outputTokens: 128000, source: 'https://developers.openai.com/api/docs/models/gpt-6-astra', verifiedAt: '2026-10-25', reasoning: { levels: ['low', 'medium', 'high', 'xhigh', 'max'], defaultEffort: null } },
  { ids: ['gpt-5.4', 'gpt-5.4-2026-03-05'], contextTokens: 1050000, inputTokens: null, outputTokens: 128000, source: 'https://developers.openai.com/api/docs/models/gpt-5.4', verifiedAt: '2026-10-25', reasoning: { levels: ['none', 'low', 'medium', 'high', 'xhigh'], defaultEffort: 'none' } },
  { ids: ['gpt-5-mini', 'gpt-5-mini-2025-08-07'], contextTokens: 400000, inputTokens: null, outputTokens: 128000, source: 'https://developers.openai.com/api/docs/models/gpt-5-mini', verifiedAt: '2026-10-25' },
  { ids: ['gpt-4.1', 'gpt-4.1-2025-04-14'], contextTokens: 1047576, inputTokens: null, outputTokens: 32768, source: 'https://developers.openai.com/api/docs/models/gpt-4.1', verifiedAt: '2026-10-25' },
  { ids: ['gpt-4o'], contextTokens: 128000, inputTokens: null, outputTokens: 16384, source: 'https://developers.openai.com/api/docs/models/gpt-4o', verifiedAt: '2026-10-25' },
  { ids: ['qwen3.8-max', 'qwen3.8-max-0902', 'qwen3.8-max-2026-09-02'], contextTokens: 1000000, inputTokens: 991808, outputTokens: 131072, source: 'https://help.aliyun.com/zh/model-studio/qwen3-8-max', verifiedAt: '2026-10-25', notes: 'qwen_thinking' },
  { ids: ['qwen3.7-plus', 'qwen3.7-plus-2026-05-26'], contextTokens: 1000000, inputTokens: 991808, outputTokens: 131072, source: 'https://help.aliyun.com/zh/model-studio/qwen3-7-plus', verifiedAt: '2026-10-25', notes: 'qwen_thinking' },
  { ids: ['deepseek-flash', 'deepseek-v4-flash', 'deepseek-v4-pro'], contextTokens: '1M', inputTokens: null, outputTokens: 393216, source: 'https://api-docs.deepseek.com/quick_start/pricing/', verifiedAt: '2026-10-25', notes: 'dynamic_alias', reasoning: { levels: ['low', 'high', 'max'], defaultEffort: 'high', source: 'https://api-docs.deepseek.com/guides/thinking_mode' } },
  { ids: ['deepseek-ai/DeepSeek-V4-Flash'], contextTokens: 1048576, inputTokens: null, outputTokens: null, source: 'https://huggingface.co/deepseek-ai/DeepSeek-V4-Flash/blob/main/config.json', verifiedAt: '2026-10-25', notes: 'weights_capacity' },
  { ids: ['doubao-seed-2-1-pro-260915'], contextTokens: '1024k', inputTokens: '1024k', outputTokens: '256k', source: 'https://console.volcengine.com/ark/region%3Acn-beijing/docs/model-list', verifiedAt: '2026-10-25' },
  { ids: ['ark-code-latest'], contextTokens: null, inputTokens: null, outputTokens: null, source: 'https://console.volcengine.com/ark/region%3Acn-beijing/docs/coding-plan-personal-plan-overview', verifiedAt: '2026-10-25', notes: 'dynamic_alias' },
  { ids: ['LongCat-2.0'], contextTokens: 1048576, inputTokens: null, outputTokens: 131072, source: 'https://longcat.chat/platform/docs/api/model-params', verifiedAt: '2026-10-25' },
];

export function modelSpec(remoteId: string): ModelSpec | undefined {
  return MODEL_SPECS.find(spec => spec.ids.includes(remoteId.trim()));
}
