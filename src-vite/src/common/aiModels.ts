// Pure helpers shared by settings and WebView configuration synchronization.
export function effectiveParameter(definition: any, configuration: any, spec: any) {
  return configuration.parameters?.[spec.key] ?? definition.defaults?.[spec.key] ?? spec.default;
}
export function applyAiConfiguration(config: any, view: any) {
  const selected = (task: string) => view.models.find((item: any) => item.definition.id === view.selection[task]);
  const semantic = selected('semantic');
  const face = selected('face');
  config.settings.ai = {
    semanticParameters: semantic?.values || {},
    faceParameters: face?.values || {},
    semanticLanguages: semantic?.definition.languages || [],
    semanticModel: view.selection.semantic,
    faceModel: view.selection.face,
    semanticProfile: view.semanticProfile || '',
    faceProfile: view.faceProfile || '',
  };
}
export function supportsNonLatin(languages: string[]) {
  // Do not guess restrictions for an unspecified custom model.
  return !(languages?.length === 1 && languages[0] === 'en');
}
export function requiresIndexRebuild(definition: any, saved: any, draft: any, schema: any[]) {
  if (['endpoint', 'remoteModel', 'remoteRevision'].some(key => saved[key] !== draft[key])) return true;
  return schema.some(spec => spec.affectsIndex && effectiveParameter(definition, saved, spec) !== effectiveParameter(definition, draft, spec));
}
export function requiresSelectionRebuild(view: any, task: string, modelId: string) {
  const current = view.models.find((row: any) => row.definition.id === view.selection[task]);
  const next = view.models.find((row: any) => row.definition.id === modelId);
  return !current || !next || current.profile !== next.profile;
}

export function formatGpuMemory(bytes: number | null | undefined) {
  if (typeof bytes !== 'number' || !Number.isFinite(bytes) || bytes < 0) return null;
  return `${(bytes / 1024 ** 3).toFixed(1)} GiB`;
}

export function physicalGpuDevices(runtime: any) {
  // Old/partial diagnostics must never turn logical adapter rows into a physical count.
  return (runtime?.devices || []).filter((device: any) => typeof device.physicalId === 'string' && device.physicalId.length > 0);
}

export function canActivateSavedModel(model: any) {
  if (!model?.installed) return false;
  if (model.definition.adapter !== 'jina_embeddings') return true;
  return !!(model.configuration?.allowCloud && model.credentialStored && model.contractTested);
}
