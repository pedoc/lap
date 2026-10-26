/** Expand optional groups only when their buttons fit beside navigation and window actions. */
export function viewerToolbarLayout(width: number, fixedButtons: number, image: boolean) {
  let free = Math.max(0, Number.isFinite(width) ? width : 0) - fixedButtons * 36 - 24;
  const take = (cost: number) => { if (free < cost) return false; free -= cost; return true; };
  const zoom = take(108);
  const overlays = image && take(104); // Replaces the compact overlay menu already counted in fixedButtons.
  const metadata = take(264);
  const files = take(120);
  return { zoom, overlays, metadata, files };
}
