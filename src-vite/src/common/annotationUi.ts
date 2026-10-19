// Contextual annotation controls stay upright and inside the visible viewport.
export function annotationEditorPosition(
  anchor: { left: number; top: number; bottom: number },
  viewport: { width: number; height: number },
  panelHeight = 230,
) {
  const margin = 8, gap = 6;
  const width = Math.max(0, Math.min(320, viewport.width - margin * 2));
  const maxHeight = Math.max(0, viewport.height - margin * 2);
  const height = Math.min(panelHeight, maxHeight);
  const below = anchor.bottom + gap;
  const preferredTop = below + height <= viewport.height - margin ? below : anchor.top - height - gap;
  return {
    left: `${Math.max(margin, Math.min(anchor.left, viewport.width - width - margin))}px`,
    top: `${Math.max(margin, Math.min(preferredTop, viewport.height - height - margin))}px`,
    width: `${width}px`, maxHeight: `${maxHeight}px`,
  };
}
