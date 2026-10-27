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
const SearchInner: React.FC<SceneProps> = ({
  headline = "找照片，\n也能说人话。",
  subline = "本地语义检索\n自由组合筛选",
  style,
}) => {
  const frame = useCurrentFrame();
  return (
    <Stage style={style}>
      <Brand />
      <div style={{ position: "absolute", left: 104, top: 263, width: 790 }}>
        <Kicker>FIND YOUR MOMENT</Kicker>
        <Title text={headline} size={90} style={{ marginTop: 22 }} />
        <Subline style={{ marginTop: 26 }}>{subline}</Subline>
        <div
          style={{
            marginTop: 39,
            width: 692,
            height: 92,
            display: "flex",
            alignItems: "center",
            gap: 22,
            padding: "0 28px",
            fontSize: 34,
            border: "1px solid #7971ff66",
            background: "linear-gradient(100deg,#635bff20,#15191e)",
            borderRadius: 18,
            opacity: ramp(frame, 18, 18),
            translate: `${mix(-30, 0, ramp(frame, 18, 24))}px 0`,
          }}
        >
          <svg width="32" height="32" viewBox="0 0 32 32" fill="none">
            <circle
              cx="13.5"
              cy="13.5"
              r="9"
              stroke={C.light}
              strokeWidth="2.4"
            />
            <path
              d="M20 20L28 28"
              stroke={C.light}
              strokeWidth="2.4"
              strokeLinecap="round"
            />
          </svg>
          <span>“山间日落的照片”</span>
        </div>
      </div>
      <Window
        style={{
          left: mix(1130, 1050, ramp(frame, 0, 25)),
          top: mix(230, 164, ramp(frame, 0, 25)),
          width: 690,
          height: 759,
          opacity: ramp(frame, 0, 14),
          transform: `perspective(1600px) rotateY(${mix(-11, -2, ramp(frame, 0, 28))}deg)`,
        }}
      >
        <Picture src="crops/filter-dialog.webp" w={690} h={759} />
      </Window>
      <div
        style={{
          position: "absolute",
          left: 905,
          top: 90,
          width: 260,
          height: 164,
          rotate: "-9deg",
          opacity: ramp(frame, 6, 18),
          translate: `0 ${mix(80, 0, ramp(frame, 6, 23))}px`,
        }}
      >
        <Window style={{ inset: 0, borderRadius: 13 }}>
          <Picture src="photos/peak.jpg" w={260} h={164} />
        </Window>
      </div>
      <Chapter number="01" text="SEARCH" />
    </Stage>
  );
};
export const Search = Interactive.withSchema({
  Component: SearchInner,
  componentName: "<Search>",
  schema: sceneSchema,
  wrapInSequence: true,
});
