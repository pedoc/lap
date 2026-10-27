import { LapPromo, SHOTS, SFX_CUES, TOTAL } from "./Composition";
import { C } from "./visual";
export const WORKBENCH = {
  name: "Lap · 你的照片，你的掌控",
  fps: 30,
  width: 3840,
  height: 2160,
  total: TOTAL,
  background: C.bg,
  shots: SHOTS.map((shot) => ({
    ...shot,
    schema: [
      {
        type: "textarea",
        key: "headline",
        label: "主文案",
        default: shot.props.headline,
      },
      {
        type: "textarea",
        key: "subline",
        label: "辅助文案",
        default: shot.props.subline,
      },
    ],
  })),
  sfx: SFX_CUES.map((cue) => ({
    from: cue.from,
    duration: cue.duration,
    src: cue.src,
    volume: cue.volume,
  })),
  bgm: [{ from: 0, duration: TOTAL, src: "audio/music.wav", volume: 0.58 }],
  original: LapPromo,
};
