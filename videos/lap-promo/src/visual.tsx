import React from "react";
import {
  AbsoluteFill,
  CanvasImage,
  Easing,
  interpolate,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
  type InteractivitySchema,
} from "remotion";

export const C = {
  bg: "#080b10",
  surface: "#15191e",
  ink: "#f5f7fa",
  muted: "#a5adb8",
  accent: "#635bff",
  light: "#aaa4ff",
};
export const FONT = '"Microsoft YaHei", "Noto Sans SC", "Segoe UI", sans-serif';
export const ease = Easing.bezier(0.16, 1, 0.3, 1);
export const smooth = Easing.bezier(0.65, 0, 0.25, 1);
export const ramp = (
  f: number,
  start: number,
  duration: number,
  easing = ease,
) =>
  interpolate(f, [start, start + duration], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing,
  });
export const mix = (a: number, b: number, t: number) => a + (b - a) * t;
export type SceneProps = {
  headline?: string;
  subline?: string;
  style?: React.CSSProperties;
};
export const sceneSchema = {
  headline: { type: "text-content", default: "", description: "主文案" },
  subline: { type: "text-content", default: "", description: "辅助文案" },
} as const satisfies InteractivitySchema;

export const Stage: React.FC<{
  children: React.ReactNode;
  style?: React.CSSProperties;
  glow?: boolean;
}> = ({ children, style, glow = true }) => {
  const { width } = useVideoConfig();
  return (
    <AbsoluteFill style={{ background: C.bg, overflow: "hidden", ...style }}>
      <div
        style={{
          position: "absolute",
          width: 1920,
          height: 1080,
          transform: `scale(${width / 1920})`,
          transformOrigin: "0 0",
          fontFamily: FONT,
          color: C.ink,
          overflow: "hidden",
        }}
      >
        {glow && (
          <div
            style={{
              position: "absolute",
              left: 640,
              top: 0,
              width: 1300,
              height: 1080,
              background:
                "radial-gradient(ellipse at 55% 50%,#635bff17,transparent 65%)",
            }}
          />
        )}
        {children}
      </div>
    </AbsoluteFill>
  );
};

export const Picture: React.FC<{
  src: string;
  w: number;
  h: number;
  style?: React.CSSProperties;
  fit?: "cover" | "contain";
}> = ({ src, w, h, style, fit = "cover" }) => {
  const { fps } = useVideoConfig();
  return (
    <CanvasImage
      src={staticFile(src)}
      width={w * 2}
      height={h * 2}
      fit={fit}
      premountFor={fps}
      style={{ width: w, height: h, display: "block", ...style }}
    />
  );
};

export const Brand: React.FC = () => (
  <div
    style={{
      position: "absolute",
      left: 104,
      top: 62,
      display: "flex",
      alignItems: "center",
      gap: 16,
      fontSize: 36,
      fontWeight: 600,
      zIndex: 8,
    }}
  >
    <Picture src="lap.png" w={56} h={56} />
    <span>Lap</span>
  </div>
);
export const Kicker: React.FC<{
  children: React.ReactNode;
  style?: React.CSSProperties;
}> = ({ children, style }) => (
  <div
    style={{
      fontSize: 24,
      fontWeight: 600,
      letterSpacing: 4,
      color: C.light,
      ...style,
    }}
  >
    {children}
  </div>
);
export const Title: React.FC<{
  text: string;
  size?: number;
  delay?: number;
  style?: React.CSSProperties;
  accent?: string;
}> = ({ text, size = 94, delay = 5, style, accent }) => {
  const frame = useCurrentFrame();
  return (
    <div
      style={{
        fontSize: size,
        fontWeight: 800,
        letterSpacing: -4,
        lineHeight: 1.22,
        ...style,
      }}
    >
      {text.split("\n").map((line, i) => (
        <div key={i} style={{ overflow: "hidden", paddingBottom: 5 }}>
          <div
            style={{
              translate: `0 ${mix(115, 0, ramp(frame, delay + i * 4, 23))}%`,
              color: accent && i > 0 ? accent : undefined,
            }}
          >
            {line}
          </div>
        </div>
      ))}
    </div>
  );
};
export const Subline: React.FC<{
  children: React.ReactNode;
  style?: React.CSSProperties;
}> = ({ children, style }) => {
  const frame = useCurrentFrame();
  return (
    <div
      style={{
        fontSize: 34,
        color: C.muted,
        lineHeight: 1.65,
        whiteSpace: "pre-line",
        opacity: ramp(frame, 15, 18),
        translate: `0 ${mix(16, 0, ramp(frame, 15, 18))}px`,
        ...style,
      }}
    >
      {children}
    </div>
  );
};
export const Window: React.FC<{
  children: React.ReactNode;
  style?: React.CSSProperties;
}> = ({ children, style }) => (
  <div
    style={{
      position: "absolute",
      overflow: "hidden",
      borderRadius: 20,
      border: "1px solid #ffffff25",
      boxShadow: "0 42px 90px #0008, 0 4px 16px #0005",
      background: C.surface,
      ...style,
    }}
  >
    {children}
  </div>
);
export const Chapter: React.FC<{ number: string; text: string }> = ({
  number,
  text,
}) => (
  <div
    style={{
      position: "absolute",
      left: 104,
      bottom: 55,
      display: "flex",
      gap: 20,
      alignItems: "center",
      fontSize: 21,
      letterSpacing: 3,
      color: "#818b99",
    }}
  >
    <span style={{ color: C.light }}>{number}</span>
    <div style={{ width: 44, height: 1, background: "#635bff90" }} />
    {text}
  </div>
);
