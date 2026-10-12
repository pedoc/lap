export function canConfirmFace(report: any, image: any, face: any, libraryId: string): boolean {
 const r=face?.region;
 return report?.library_id===libraryId && face?.state==='quality_filtered' && /^[0-9a-f]{64}$/i.test(report?.diagnostics?.profile || '')
  && Number.isSafeInteger(image?.fileId) && image.fileId>0 && /^[0-9a-f]{64}$/i.test(image?.sourceRevision || '')
  && [image?.sourceSize,image?.pipeline?.decodedSize].every(size=>Array.isArray(size)&&size.length===2&&size.every(value=>Number.isSafeInteger(value)&&value>0))
  && !!r && [r.x,r.y,r.width,r.height].every(Number.isFinite) && r.x>=0 && r.y>=0 && r.width>0 && r.height>0 && r.x+r.width<=1.00001 && r.y+r.height<=1.00001;
}
export function faceConfirmationRequest(report: any, image: any, face: any, libraryId: string) {
 if (!canConfirmFace(report,image,face,libraryId)) throw new Error('FACE_CONFIRM_STALE: Re-detect this image before confirming a candidate');
 return {libraryId,profile:report.diagnostics.profile,fileId:image.fileId,sourceRevision:image.sourceRevision,sourceSize:[...image.sourceSize],decodedSize:[...image.pipeline.decodedSize],region:{...face.region}};
}
export function candidateCrop(image: any, face: any) {
 const r=face?.region, size=image?.pipeline?.decodedSize;
 if (!r || !size || ![r.x,r.y,r.width,r.height,...size].every(Number.isFinite) || r.width<=0 || r.height<=0 || size[0]<=0 || size[1]<=0) return null;
 const aspect=r.width*size[0]/(r.height*size[1]);
 const width=Math.min(72,110*aspect),height=width/aspect;
 return {frame:{width:`${width}px`,height:`${height}px`},image:{position:'absolute',maxWidth:'none',width:`${100/r.width}%`,height:`${100/r.height}%`,left:`${-100*r.x/r.width}%`,top:`${-100*r.y/r.height}%`}};
}
export function confirmationError(error: any,t: any) {
 const message=error?.message || String(error);
 if (message.startsWith('FACE_CONFIRM_EXCLUDED:')) return t('face_diagnostics.force_excluded');
 if (message.startsWith('FACE_CONFIRM_STALE:')) return t('face_diagnostics.force_stale');
 if (message.startsWith('FACE_CONFIRM_BUSY:')) return t('face_diagnostics.force_busy');
 return message;
}
