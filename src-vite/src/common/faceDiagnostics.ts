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
export function recordFaceConfirmation(report: any, result: any, faceIndex: number, libraryId: string): boolean {
 if (faceDiagnosticState.report !== report || report?.library_id !== libraryId || !Number.isSafeInteger(result?.faceId) || result.faceId <= 0) return false;
 const note={fileId:result.fileId,faceId:result.faceId,personId:result.personId,faceIndex,added:!!result.added,restoredEmbedding:!!result.restoredEmbedding,storedRegions:result.storedRegions,manualRegions:result.manualRegions};
 const previous=Array.isArray(report.confirmations) ? report.confirmations : [];
 faceDiagnosticState.report={...report,confirmations:[...previous.filter((item:any)=>item.fileId!==note.fileId || item.faceIndex!==faceIndex),note]};
 return true;
}
export function diagnosticExportText(report: any): string {
 // Public metadata includes candidate geometry and confirmation IDs, never pixels, vectors or credentials.
 return JSON.stringify(report, null, 2);
}
