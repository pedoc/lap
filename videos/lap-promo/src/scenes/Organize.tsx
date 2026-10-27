import React from "react";
import { Easing, Interactive, useCurrentFrame } from "remotion";
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
const OrganizeInner: React.FC<SceneProps> = ({
  headline = "让整理，\n自己发生。",
  subline = "智能相册\n保存规则，持续归集。",
  style,
}) => {
  const frame = useCurrentFrame();
  const land = Easing.bezier(0.34, 1.4, 0.44, 1);
  return (
    <Stage style={style}>
      <Brand />
      <Window
        style={{
          left: mix(1010, 810, ramp(frame, 0, 29)),
          top: 154,
          width: 1040,
          height: 780,
          opacity: ramp(frame, 0, 13),
          transform: `perspective(1800px) rotateY(${mix(-10, -2, ramp(frame, 0, 29))}deg)`,
        }}
      >
        <Picture src="crops/albums-backplate.webp" w={1040} h={780} />
        <div
          style={{
            position: "absolute",
            inset: 0,
            width: 1440,
            height: 1080,
            transform: "scale(0.7222222222)",
            transformOrigin: "0 0",
          }}
        >
          {[337, 628, 919].map((x, i) => {
            const p = ramp(frame, 4 + i * 9, 23, land);
            return (
              <div
                key={x}
                style={{
                  position: "absolute",
                  left: x + mix(230, 0, p),
                  top: 95 + mix(i === 1 ? -390 : 390, 0, p),
                  width: 275,
                  height: 310,
                  rotate: `${mix(i === 1 ? -12 : 10, 0, p)}deg`,
                  opacity: Math.min(1, p),
                  boxShadow: `0 ${mix(50, 0, p)}px ${mix(70, 0, p)}px #0008`,
                }}
              >
                <Picture src={`crops/album-${i}.webp`} w={275} h={310} />
              </div>
            );
          })}
        </div>
      </Window>
      <div style={{ position: "absolute", left: 104, top: 244, width: 670 }}>
        <Kicker>ORGANIZE ONCE. KEEP FINDING.</Kicker>
        <Title text={headline} size={88} style={{ marginTop: 24 }} />
        <Subline style={{ marginTop: 27 }}>{subline}</Subline>
        <div style={{ display: "flex", gap: 12, marginTop: 42 }}>
          {["评分 ≥ 4", "收藏", "地点"].map((text, i) => (
            <div
              key={text}
              style={{
                padding: "12px 17px",
                background: "#635bff20",
                color: C.light,
                border: "1px solid #635bff70",
                borderRadius: 10,
                fontSize: 36,
                opacity: ramp(frame, 17 + i * 4, 14),
                translate: `0 ${mix(28, 0, ramp(frame, 17 + i * 4, 18))}px`,
              }}
            >
              {text}
            </div>
          ))}
        </div>
      </div>
      <Chapter number="04" text="SMART ALBUMS" />
    </Stage>
  );
};
export const Organize = Interactive.withSchema({
  Component: OrganizeInner,
  componentName: "<Organize>",
  schema: sceneSchema,
  wrapInSequence: true,
});
