import { reactive } from 'vue';
export const faceDiagnosticState = reactive<{ report: any; visible: boolean }>({ report: null, visible: false });
export function faceDiagnosticNeedsAttention(result: any): boolean {
 const d = result?.diagnostics;
 return !!result?.error || Number(result?.failed || 0) > 0 || !!d?.startupError || (Number(d?.detectedFaces || 0) === 0 && Number(d?.cachedImages || 0) === 0) || Number(d?.qualityFiltered || 0) > 0 || Number(d?.annotationSuppressed || 0) > 0;
}
export function recordFaceDiagnostic(result: any, libraryId: string) {
 if (!result || result.library_id !== libraryId || !result.diagnostics) return false;
 faceDiagnosticState.report = result;
 if (result.scope === 'selection' && !result.cancelled && faceDiagnosticNeedsAttention(result)) faceDiagnosticState.visible = true;
 return true;
}
export function diagnosticExportText(report: any): string {
 // Backend contains filenames, model parameters and counters only; no pixels, embeddings or credentials.
 return JSON.stringify(report, null, 2);
}
