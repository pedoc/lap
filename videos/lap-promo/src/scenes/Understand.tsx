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
  mix,
  ramp,
  sceneSchema,
  smooth,
  type SceneProps,
} from "../visual";
// Adapted from word-relay-filmstrip: 18f step; 8f exit before the incoming word.
const SWITCH = 83;
const UnderstandInner: React.FC<SceneProps> = ({
  headline = "读出\n文字。",
  subline = "图片里的字，也能搜。",
  style,
}) => {
  const frame = useCurrentFrame();
  const step = ramp(frame, SWITCH, 18, smooth);
  const cards = [
    { src: "crops/ocr-focus.webp" },
    { src: "crops/objects.webp" },
  ];
  const index = frame < SWITCH + 7 ? 0 : 1;
  const wordIn = index === 0 ? ramp(frame, 6, 22) : ramp(frame, SWITCH + 7, 20);
  const wordOut = index === 0 ? ramp(frame, SWITCH, 8) : 0;
  return (
    <Stage style={style}>
      <Brand />
      <div
        style={{
          position: "absolute",
          left: 72,
          top: 160,
          width: 1056,
          height: 790,
          overflow: "hidden",
          maskImage:
            "linear-gradient(transparent,#000 6%,#000 94%,transparent)",
        }}
      >
        {cards.map((card, i) => {
          const y = 50 + i * 820 - step * 820;
          const dist = Math.min(1, Math.abs(i - step));
          return (
            <div
              key={card.src}
              style={{
                position: "absolute",
                left: 8,
                top: y,
                width: 1020,
                height: 680,
                overflow: "hidden",
                borderRadius: 20,
                border: "1px solid #ffffff29",
                boxShadow: "0 28px 65px #0009",
                scale: 1 - 0.06 * dist,
                filter: `blur(${dist * 3}px)`,
                opacity: 1 - 0.68 * dist,
              }}
            >
              <Picture src={card.src} w={1020} h={680} />
            </div>
          );
        })}
      </div>
      <div
        style={{
          position: "absolute",
          left: 1123,
          top: 549,
          height: 2,
          width:
            66 * (1 - ramp(frame, SWITCH, 6) + ramp(frame, SWITCH + 10, 16)),
          maxWidth: 66,
          background: C.accent,
        }}
      />
      <div style={{ position: "absolute", left: 1210, top: 342, width: 660 }}>
        <Kicker>LOCAL INTELLIGENCE</Kicker>
        <div
          style={{
            position: "absolute",
            top: 90,
            height: 236,
            overflow: "hidden",
            fontSize: 100,
            fontWeight: 800,
            letterSpacing: -4,
            lineHeight: "118px",
          }}
        >
          <div
            style={{
              translate: `0 ${mix(110, 0, wordIn) - wordOut * 110}%`,
              opacity: 1 - wordOut,
            }}
          >
            {(index === 0 ? headline : "认出\n画面。")
              .split("\n")
              .map((line, i) => (
                <div key={i} style={{ color: i === 1 ? C.light : C.ink }}>
                  {line}
                </div>
              ))}
          </div>
        </div>
        <Subline
          style={{
            position: "absolute",
            top: 366,
            opacity: wordIn * (1 - wordOut),
            translate: "0 0",
          }}
        >
          {index === 0 ? "本地 OCR" : "本地物体识别"}
          <br />
          {index === 0 ? subline : "看见内容，不止文件名。"}
        </Subline>
      </div>
      <Chapter number="02" text="UNDERSTAND" />
    </Stage>
  );
};
export const Understand = Interactive.withSchema({
  Component: UnderstandInner,
  componentName: "<Understand>",
  schema: sceneSchema,
  wrapInSequence: true,
});
