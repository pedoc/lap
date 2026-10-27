import React from "react";
import { Audio } from "@remotion/media";
import { AbsoluteFill, staticFile, useVideoConfig } from "remotion";
import { Opening } from "./scenes/Opening";
import { Search } from "./scenes/Search";
import { Understand } from "./scenes/Understand";
import { Chat } from "./scenes/Chat";
import { Organize } from "./scenes/Organize";
import { Places } from "./scenes/Places";
import { Ending } from "./scenes/Ending";
import { Cast } from "./scenes/Cast";
import { C } from "./visual";

export type PromoProps = { bgm: boolean };
export const TOTAL = 997;
export const BPM = 129.975;
export const beatF = (beat: number) => Math.round(((beat * 60) / BPM) * 30);

// Each scene has a distinct authored JSX node. The workbench reads these same nodes.
export const getShotElements = (fps: number) => [
  <Opening
    key="opening"
    name="01 · 回忆，不该难找"
    from={0}
    durationInFrames={111}
    premountFor={fps}
    headline={"回忆，\n不该难找。"}
    subline="让每一张照片，重新被看见。"
  />,
  <Search
    key="search"
    name="02 · 语义与组合检索"
    from={111}
    durationInFrames={111}
    premountFor={fps}
    headline={"找照片，\n也能说人话。"}
    subline={"本地语义检索\n自由组合筛选"}
  />,
  <Understand
    key="understand"
    name="03 · 本地文字与物体识别"
    from={222}
    durationInFrames={166}
    premountFor={fps}
    headline={"读出\n文字。"}
    subline="图片里的字，也能搜。"
  />,
  <Chat
    key="chat"
    name="04 · 媒体对话"
    from={388}
    durationInFrames={111}
    premountFor={fps}
    headline={"让照片，\n参与对话。"}
    subline="两图比较 · 构图建议"
  />,
  <Organize
    key="organize"
    name="05 · 智能相册"
    from={499}
    durationInFrames={110}
    premountFor={fps}
    headline={"让整理，\n自己发生。"}
    subline={"智能相册\n保存规则，持续归集。"}
  />,
  <Places
    key="places"
    name="06 · 照片地图"
    from={609}
    durationInFrames={111}
    premountFor={fps}
    headline={"把回忆，\n放回地图。"}
    subline="照片定位 · 原生 3D / 卫星视图"
  />,
  <Cast
    key="cast"
    name="07 · DLNA 电视投屏"
    from={720}
    durationInFrames={111}
    premountFor={fps}
    headline={"好照片，\n大屏一起看。"}
    subline={"DLNA 电视投屏\n照片与视频，都能分享"}
  />,
  <Ending
    key="ending"
    name="08 · 你的照片，你的掌控"
    from={831}
    durationInFrames={166}
    premountFor={fps}
    headline="你的照片。你的掌控。"
    subline="本地优先 · 开源免费"
  />,
];
export const SHOTS = getShotElements(30).map((node) => ({
  id: String(node.key),
  label: node.props.name,
  from: node.props.from,
  duration: node.props.durationInFrames,
  component: node.type,
  props: { headline: node.props.headline, subline: node.props.subline },
}));
export const SFX_CUES = [
  {
    from: 2,
    src: "audio/camera-shutter-hard.wav",
    duration: 28,
    volume: 0.25,
    label: "开场影像",
  },
  {
    from: SHOTS[1].from,
    src: "audio/sweep-fast-small.wav",
    duration: 20,
    volume: 0.38,
    label: "检索切入",
  },
  {
    from: SHOTS[2].from,
    src: "audio/air-zoom-vacuum.wav",
    duration: 25,
    volume: 0.28,
    label: "识别画面",
  },
  {
    from: SHOTS[2].from + 83,
    src: "audio/sweep-fast-small.wav",
    duration: 20,
    volume: 0.28,
    label: "OCR 切到物体",
  },
  {
    from: SHOTS[3].from,
    src: "audio/air-zoom-vacuum.wav",
    duration: 26,
    volume: 0.3,
    label: "对话推入",
  },
  {
    from: SHOTS[4].from + 45,
    src: "audio/bass-hit-short.wav",
    duration: 30,
    volume: 0.32,
    label: "相册落版",
  },
  {
    from: SHOTS[5].from,
    src: "audio/sweep-fast-small.wav",
    duration: 20,
    volume: 0.31,
    label: "地图展开",
  },
  {
    from: SHOTS[7].from + 1,
    src: "audio/air-zoom-vacuum.wav",
    duration: 40,
    volume: 0.29,
    label: "品牌归拢",
  },
  {
    from: SHOTS[7].from + 38,
    src: "audio/impact-deep-whoosh.wav",
    duration: 80,
    volume: 0.42,
    label: "品牌落点",
  },
  {
    from: SHOTS[7].from + 60,
    src: "audio/shimmer-sparkle-sweep.wav",
    duration: 65,
    volume: 0.12,
    label: "品牌余韵",
  },
  {
    from: SHOTS[6].from + 18,
    src: "audio/air-zoom-vacuum.wav",
    duration: 40,
    volume: 0.3,
    label: "投屏传送",
  },
];
export const LapPromo: React.FC<PromoProps> = ({ bgm = true }) => {
  const { fps } = useVideoConfig();
  return (
    <AbsoluteFill style={{ backgroundColor: C.bg }}>
      {getShotElements(fps)}
      {bgm && (
        <Audio
          name="BGM · Cat Walk"
          src={staticFile("audio/music.wav")}
          durationInFrames={997}
          premountFor={fps}
          volume={0.58}
        />
      )}
      <Audio
        name="SFX · 开场"
        src={staticFile(SFX_CUES[0].src)}
        from={SFX_CUES[0].from}
        durationInFrames={SFX_CUES[0].duration}
        volume={SFX_CUES[0].volume}
        premountFor={fps}
      />
      <Audio
        name="SFX · 检索"
        src={staticFile(SFX_CUES[1].src)}
        from={SFX_CUES[1].from}
        durationInFrames={SFX_CUES[1].duration}
        volume={SFX_CUES[1].volume}
        premountFor={fps}
      />
      <Audio
        name="SFX · 识别"
        src={staticFile(SFX_CUES[2].src)}
        from={SFX_CUES[2].from}
        durationInFrames={SFX_CUES[2].duration}
        volume={SFX_CUES[2].volume}
        premountFor={fps}
      />
      <Audio
        name="SFX · 识别切词"
        src={staticFile(SFX_CUES[3].src)}
        from={SFX_CUES[3].from}
        durationInFrames={SFX_CUES[3].duration}
        volume={SFX_CUES[3].volume}
        premountFor={fps}
      />
      <Audio
        name="SFX · 对话"
        src={staticFile(SFX_CUES[4].src)}
        from={SFX_CUES[4].from}
        durationInFrames={SFX_CUES[4].duration}
        volume={SFX_CUES[4].volume}
        premountFor={fps}
      />
      <Audio
        name="SFX · 相册"
        src={staticFile(SFX_CUES[5].src)}
        from={SFX_CUES[5].from}
        durationInFrames={SFX_CUES[5].duration}
        volume={SFX_CUES[5].volume}
        premountFor={fps}
      />
      <Audio
        name="SFX · 地图"
        src={staticFile(SFX_CUES[6].src)}
        from={SFX_CUES[6].from}
        durationInFrames={SFX_CUES[6].duration}
        volume={SFX_CUES[6].volume}
        premountFor={fps}
      />
      <Audio
        name="SFX · 投屏传送"
        src={staticFile(SFX_CUES[10].src)}
        from={SFX_CUES[10].from}
        durationInFrames={SFX_CUES[10].duration}
        volume={SFX_CUES[10].volume}
        premountFor={fps}
      />
      <Audio
        name="SFX · 品牌归拢"
        src={staticFile(SFX_CUES[7].src)}
        from={SFX_CUES[7].from}
        durationInFrames={SFX_CUES[7].duration}
        volume={SFX_CUES[7].volume}
        premountFor={fps}
      />
      <Audio
        name="SFX · 品牌落点"
        src={staticFile(SFX_CUES[8].src)}
        from={SFX_CUES[8].from}
        durationInFrames={SFX_CUES[8].duration}
        volume={SFX_CUES[8].volume}
        premountFor={fps}
      />
      <Audio
        name="SFX · 品牌余韵"
        src={staticFile(SFX_CUES[9].src)}
        from={SFX_CUES[9].from}
        durationInFrames={SFX_CUES[9].duration}
        volume={SFX_CUES[9].volume}
        premountFor={fps}
      />
    </AbsoluteFill>
  );
};
