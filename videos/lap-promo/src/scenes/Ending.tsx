import React from "react";
import { Easing, Interactive, useCurrentFrame } from "remotion";
import { Television } from "./Cast";
import {
  C,
  Picture,
  Stage,
  mix,
  ramp,
  sceneSchema,
  type SceneProps,
} from "../visual";
const cards = [
  {
    src: "screens/combined-filters.webp",
    x: -100,
    y: 50,
    w: 490,
    h: 367,
    r: -8,
    dx: -650,
    dy: -240,
  },
  {
    src: "screens/ocr.webp",
    x: 1465,
    y: 30,
    w: 520,
    h: 390,
    r: 7,
    dx: 650,
    dy: -200,
  },
  {
    src: "screens/objects.webp",
    x: -175,
    y: 646,
    w: 500,
    h: 375,
    r: 7,
    dx: -600,
    dy: 450,
  },
  {
    src: "screens/media-chat.webp",
    x: 1515,
    y: 672,
    w: 500,
    h: 375,
    r: -8,
    dx: 620,
    dy: 400,
  },
  {
    src: "screens/smart-albums.webp",
    x: 270,
    y: 880,
    w: 425,
    h: 319,
    r: 3,
    dx: -180,
    dy: 450,
  },
  {
    src: "screens/map-amap-satellite.webp",
    x: 1200,
    y: -190,
    w: 430,
    h: 323,
    r: -4,
    dx: 220,
    dy: -480,
  },
  {
    src: "cast-preview",
    x: 1150,
    y: 870,
    w: 450,
    h: 253,
    r: -4,
    dx: 150,
    dy: 450,
  },
];
const land = Easing.bezier(0.34, 1.4, 0.44, 1);
const EndingInner: React.FC<SceneProps> = ({
  headline = "你的照片。你的掌控。",
  subline = "本地优先 · 开源免费",
  style,
}) => {
  const frame = useCurrentFrame();
  const stamp = ramp(frame, 38, 18);
  return (
    <Stage style={style}>
      <div
        style={{
          position: "absolute",
          inset: 0,
          background:
            "radial-gradient(ellipse at 50% 38%,#635bff2b,transparent 52%)",
          opacity: ramp(frame, 25, 25),
        }}
      />
      {cards.map((card, i) => {
        const p = ramp(frame, 2 + i * 3, 24, land);
        const dim = ramp(frame, 36, 17);
        return (
          <div
            key={card.src}
            style={{
              position: "absolute",
              left: card.x + card.dx * (1 - p),
              top: card.y + card.dy * (1 - p),
              width: card.w,
              height: card.h,
              rotate: `${mix(card.r * 2, card.r, p)}deg`,
              scale: mix(1.14, 1, p),
              opacity: p * mix(0.72, 0.29, dim),
              overflow: "hidden",
              borderRadius: 18,
              border: "1px solid #ffffff2a",
              boxShadow: "0 28px 65px #000a",
              filter: `blur(${mix(1.4, 0, p)}px)`,
            }}
          >
            {card.src === "cast-preview" ? (
              <Television w={card.w} h={card.h} />
            ) : (
              <Picture src={card.src} w={card.w} h={card.h} />
            )}
          </div>
        );
      })}
      <div
        style={{
          position: "absolute",
          left: 360,
          top: 260,
          width: 1200,
          textAlign: "center",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          opacity: stamp,
          scale: mix(0.91, 1, stamp),
          translate: `0 ${mix(40, 0, stamp)}px`,
        }}
      >
        <Picture src="lap.png" w={123} h={123} />
        <div
          style={{
            fontFamily: '"Segoe UI", sans-serif',
            fontSize: 164,
            fontWeight: 650,
            letterSpacing: -8,
            lineHeight: 1.15,
            marginTop: 7,
          }}
        >
          Lap
        </div>
        <div
          style={{
            fontSize: 62,
            fontWeight: 600,
            letterSpacing: -2,
            marginTop: 28,
            opacity: ramp(frame, 48, 18),
          }}
        >
          {headline}
        </div>
        <div
          style={{
            fontSize: 35,
            color: C.muted,
            marginTop: 28,
            opacity: ramp(frame, 54, 18),
          }}
        >
          {subline}
        </div>
      </div>
    </Stage>
  );
};
export const Ending = Interactive.withSchema({
  Component: EndingInner,
  componentName: "<Ending>",
  schema: sceneSchema,
  wrapInSequence: true,
});
