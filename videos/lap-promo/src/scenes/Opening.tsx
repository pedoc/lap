import React from "react";
import { Interactive, useCurrentFrame } from "remotion";
import {
  Brand,
  C,
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
const OpeningInner: React.FC<SceneProps> = ({
  headline = "回忆，\n不该难找。",
  subline = "让每一张照片，重新被看见。",
  style,
}) => {
  const frame = useCurrentFrame();
  const settle = ramp(frame, 65, 32);
  return (
    <Stage style={style}>
      <div
        style={{
          position: "absolute",
          left: 850,
          top: 0,
          width: 1070,
          height: 1080,
          opacity: 1 - settle,
          overflow: "hidden",
          maskImage: "linear-gradient(90deg,transparent,#000 25%)",
        }}
      >
        <Picture
          src="photos/waterfall.jpg"
          w={1070}
          h={1080}
          style={{
            scale: mix(1.08, 1, ramp(frame, 0, 105)),
            transformOrigin: "70% 55%",
          }}
        />
      </div>
      <Window
        style={{
          left: mix(940, 820, settle),
          top: mix(230, 148, settle),
          width: 1030,
          height: 772.5,
          opacity: settle,
          transform: `perspective(1600px) rotateY(${mix(-12, -5, settle)}deg)`,
          transformOrigin: "50% 50%",
        }}
      >
        <Picture src="screens/home.webp" w={1030} h={772.5} />
      </Window>
      <Brand />
      <div style={{ position: "absolute", left: 104, top: 326, width: 730 }}>
        <div
          style={{
            width: 64 * ramp(frame, 0, 18),
            height: 5,
            background: C.accent,
            marginBottom: 36,
          }}
        />
        <Kicker>PRIVATE. LOCAL. YOURS.</Kicker>
        <Title text={headline} size={106} style={{ marginTop: 23 }} />
        <Subline style={{ marginTop: 28 }}>{subline}</Subline>
      </div>
      <div
        style={{
          position: "absolute",
          left: 104,
          bottom: 58,
          fontSize: 22,
          letterSpacing: 4,
          color: "#8993a1",
        }}
      >
        PRIVATE LOCAL PHOTO MANAGER
      </div>
    </Stage>
  );
};
export const Opening = Interactive.withSchema({
  Component: OpeningInner,
  componentName: "<Opening>",
  schema: sceneSchema,
  wrapInSequence: true,
});
