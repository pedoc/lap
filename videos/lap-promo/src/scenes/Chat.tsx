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
const ChatInner: React.FC<SceneProps> = ({
  headline = "让照片，\n参与对话。",
  subline = "两图比较 · 构图建议",
  style,
}) => {
  const frame = useCurrentFrame();
  return (
    <Stage style={style}>
      <Brand />
      <div style={{ position: "absolute", left: 104, top: 241, width: 650 }}>
        <Kicker>ASK YOUR PHOTOS</Kicker>
        <Title text={headline} size={86} style={{ marginTop: 24 }} />
        <Subline style={{ marginTop: 24 }}>{subline}</Subline>
        <div
          style={{
            display: "flex",
            gap: 16,
            marginTop: 37,
            opacity: ramp(frame, 10, 20),
          }}
        >
          <div
            style={{
              borderRadius: 12,
              overflow: "hidden",
              rotate: "-4deg",
              border: "1px solid #ffffff35",
            }}
          >
            <Picture src="photos/mountain.jpg" w={185} h={125} />
          </div>
          <div
            style={{
              borderRadius: 12,
              overflow: "hidden",
              rotate: "4deg",
              border: "2px solid #aaa4ff",
              translate: `0 ${mix(26, 0, ramp(frame, 17, 20))}px`,
            }}
          >
            <Picture src="photos/waterfall.jpg" w={185} h={125} />
          </div>
        </div>
        <div
          style={{
            fontSize: 36,
            color: C.muted,
            marginTop: 40,
            opacity: ramp(frame, 15, 16),
          }}
        >
          需自行配置模型服务
        </div>
      </div>
      <Window
        style={{
          left: mix(920, 800, ramp(frame, 0, 28)),
          top: 225,
          width: 1015,
          height: 664,
          opacity: ramp(frame, 0, 16),
          transform: `perspective(1600px) rotateY(${mix(-8, 0, ramp(frame, 0, 28))}deg)`,
        }}
      >
        <Picture src="crops/chat.webp" w={1015} h={664} />
      </Window>
      <Chapter number="03" text="MEDIA CHAT" />
    </Stage>
  );
};
export const Chat = Interactive.withSchema({
  Component: ChatInner,
  componentName: "<Chat>",
  schema: sceneSchema,
  wrapInSequence: true,
});
