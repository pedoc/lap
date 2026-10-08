// Pure helpers shared by settings and WebView configuration synchronization.
export function effectiveParameter(definition: any, instance: any, spec: any) {
  return instance.parameters?.[spec.key] ?? definition.defaults?.[spec.key] ?? spec.default;
}
export function applyAiConfiguration(config: any, view: any) {
  const selected = (task: string) => view.instances.find((item: any) => item.instance.id === view.bindings[task]);
  const semantic = selected('semantic');
  const face = selected('face');
  const definition = semantic && view.models.find((item: any) => item.definition.id === semantic.instance.modelId)?.definition;
  config.settings.ai = {
    semanticParameters: semantic?.values || {},
    faceParameters: face?.values || {},
    semanticLanguages: definition?.languages || [],
    libraryId: view.libraryId,
    semanticInstance: view.bindings.semantic,
    faceInstance: view.bindings.face,
    semanticProfile: view.semanticProfile || '',
    faceProfile: view.faceProfile || '',
  };
}
export function supportsNonLatin(languages: string[]) {
  // Do not guess restrictions for an unspecified custom model.
  return !(languages?.length === 1 && languages[0] === 'en');
}

export function requiresIndexRebuild(definition: any, saved: any, draft: any, schema: any[]) {
  if (saved.modelId !== draft.modelId || ['endpoint', 'remoteModel', 'remoteRevision'].some(key => saved[key] !== draft[key])) return true;
  return schema.some(spec => spec.affectsIndex && effectiveParameter(definition, saved, spec) !== effectiveParameter(definition, draft, spec));
}
