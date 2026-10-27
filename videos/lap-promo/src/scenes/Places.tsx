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
const PlacesInner: React.FC<SceneProps> = ({
  headline = "把回忆，\n放回地图。",
  subline = "照片定位 · 原生 3D / 卫星视图",
  style,
}) => {
  const frame = useCurrentFrame();
  const satellite = ramp(frame, 54, 13);
  return (
    <Stage style={style}>
      <Brand />
      <div style={{ position: "absolute", left: 104, top: 287, width: 620 }}>
        <Kicker>MEMORIES HAVE A PLACE</Kicker>
        <Title text={headline} size={88} style={{ marginTop: 24 }} />
        <Subline style={{ marginTop: 28, width: 570 }}>{subline}</Subline>
        <div
          style={{
            fontSize: 36,
            color: C.muted,
            marginTop: 41,
            opacity: ramp(frame, 12, 18),
          }}
        >
          地图服务需联网
        </div>
      </div>
      <Window
        style={{
          left: mix(870, 760, ramp(frame, 0, 27)),
          top: 133,
          width: 1080,
          height: 810,
          opacity: ramp(frame, 0, 12),
          transform: `perspective(1800px) rotateY(${mix(-8, 0, ramp(frame, 0, 27))}deg)`,
        }}
      >
        <Picture src="screens/map-amap-3d.webp" w={1080} h={810} />
        <div style={{ position: "absolute", inset: 0, opacity: satellite }}>
          <Picture src="screens/map-amap-satellite.webp" w={1080} h={810} />
        </div>
      </Window>
      <div
        style={{
          position: "absolute",
          right: 104,
          bottom: 62,
          fontSize: 36,
          color: C.muted,
        }}
      >
        地图画面来自高德地图
      </div>
      <Chapter number="05" text="PHOTO MAP" />
    </Stage>
  );
};
export const Places = Interactive.withSchema({
  Component: PlacesInner,
  componentName: "<Places>",
  schema: sceneSchema,
  wrapInSequence: true,
});
