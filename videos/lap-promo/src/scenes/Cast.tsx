import React from "react";
import { Interactive, useCurrentFrame } from "remotion";
import {
  Brand,
  C,
  Chapter,
  Kicker,
  Picture,
  Stage,
  Subline,
  Title,
  Window,
  mix,
  ramp,
  sceneSchema,
  type SceneProps,
} from "../visual";

export const Television: React.FC<{ w: number; h: number; lit?: number }> = ({
  w,
  h,
  lit = 1,
}) => (
  <div
    style={{
      position: "relative",
      width: w,
      height: h,
      padding: 12,
      background:
        "linear-gradient(145deg,#555a63,#181b21 6%,#08090c 70%,#3b3f48)",
      borderRadius: 20,
      border: "1px solid #8c93a05c",
      boxShadow: "0 32px 70px #0009,inset 0 1px 0 #ffffff50",
    }}
  >
    <div
      style={{
        position: "relative",
        width: w - 24,
        height: h - 24,
        overflow: "hidden",
        borderRadius: 8,
        background: "#05070a",
      }}
    >
      <Picture
        src="photos/waterfall.jpg"
        w={w - 24}
        h={h - 24}
        style={{ opacity: lit }}
      />
      <div
        style={{
          position: "absolute",
          inset: 0,
          background: "linear-gradient(140deg,#ffffff12,transparent 48%)",
        }}
      />
    </div>
    <div
      style={{
        position: "absolute",
        right: 30,
        bottom: 4,
        width: 5,
        height: 3,
        borderRadius: 2,
        background: C.light,
      }}
    />
    <div
      style={{
        position: "absolute",
        left: w * 0.18,
        top: h - 3,
        width: 72,
        height: 45,
        background: "linear-gradient(115deg,#414650,#101217)",
        clipPath: "polygon(42% 0,56% 0,98% 100%,70% 100%)",
      }}
    />
    <div
      style={{
        position: "absolute",
        right: w * 0.18,
        top: h - 3,
        width: 72,
        height: 45,
        background: "linear-gradient(115deg,#343943,#0e1014)",
        clipPath: "polygon(44% 0,58% 0,30% 100%,2% 100%)",
      }}
    />
  </div>
);

const CastInner: React.FC<SceneProps> = ({
  headline = "好照片，\n大屏一起看。",
  subline = "DLNA 电视投屏\n照片与视频，都能分享",
  style,
}) => {
  const frame = useCurrentFrame();
  const transfer = ramp(frame, 18, 35);
  const flying = 1 - ramp(frame, 52, 10);
  return (
    <Stage style={style}>
      <Brand />
      <div style={{ position: "absolute", left: 104, top: 247, width: 690 }}>
        <Kicker>FROM LAP TO YOUR LIVING ROOM</Kicker>
        <Title text={headline} size={90} style={{ marginTop: 24 }} />
        <Subline style={{ marginTop: 28, fontSize: 36 }}>{subline}</Subline>
      </div>
      <div
        style={{
          position: "absolute",
          left: 820,
          top: 823,
          width: 1080,
          height: 110,
          background: "radial-gradient(ellipse,#635bff25,transparent 68%)",
        }}
      />
      <div
        style={{
          position: "absolute",
          left: 875,
          top: 252,
          transform: `perspective(1800px) rotateY(${mix(-6, 0, ramp(frame, 0, 30))}deg)`,
          transformOrigin: "50% 70%",
          opacity: ramp(frame, 0, 16),
          translate: `0 ${mix(45, 0, ramp(frame, 0, 25))}px`,
        }}
      >
        <Television w={940} h={529} lit={ramp(frame, 46, 14)} />
      </div>
      <Window
        style={{
          left: 735,
          top: 624,
          width: 330,
          height: 247.5,
          borderRadius: 12,
          transform: "perspective(1000px) rotateY(8deg)",
          opacity: ramp(frame, 3, 17),
        }}
      >
        <Picture src="screens/home.webp" w={330} h={247.5} />
      </Window>
      <svg
        style={{
          position: "absolute",
          left: 730,
          top: 420,
          width: 270,
          height: 225,
          opacity: (1 - ramp(frame, 58, 12)) * 0.65,
        }}
        viewBox="0 0 270 225"
        fill="none"
      >
        <path
          d="M30 205C-3 110 125 70 250 20"
          stroke={C.accent}
          strokeWidth="2"
          strokeDasharray="360"
          strokeDashoffset={360 * (1 - ramp(frame, 5, 24))}
        />
        <path
          d="M53 215C30 132 146 94 255 40"
          stroke={C.light}
          strokeWidth="1"
          opacity=".5"
          strokeDasharray="340"
          strokeDashoffset={340 * (1 - ramp(frame, 9, 24))}
        />
      </svg>
      {frame < 63 && (
        <div
          style={{
            position: "absolute",
            left: mix(775, 887, transfer),
            top: mix(664, 264, transfer),
            width: mix(225, 916, transfer),
            height: mix(126.5, 505, transfer),
            overflow: "hidden",
            borderRadius: mix(8, 8, transfer),
            opacity: ramp(frame, 9, 10) * flying,
            boxShadow: `0 ${mix(12, 32, transfer)}px 65px #0007`,
          }}
        >
          <Picture
            src="photos/waterfall.jpg"
            w={916}
            h={505}
            style={{ width: "100%", height: "100%" }}
          />
        </div>
      )}
      <div
        style={{
          position: "absolute",
          left: 104,
          bottom: 117,
          fontSize: 36,
          color: C.muted,
          opacity: ramp(frame, 12, 18),
        }}
      >
        同一局域网 · 兼容 DLNA 的电视
      </div>
      <Chapter number="06" text="CAST TO TV" />
      <div
        style={{
          position: "absolute",
          right: 104,
          bottom: 55,
          fontSize: 36,
          color: C.muted,
        }}
      >
        投屏效果示意
      </div>
    </Stage>
  );
};
export const Cast = Interactive.withSchema({
  Component: CastInner,
  componentName: "<Cast>",
  schema: sceneSchema,
  wrapInSequence: true,
});
