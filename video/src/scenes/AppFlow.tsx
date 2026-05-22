import {
  AbsoluteFill,
  Easing,
  Img,
  interpolate,
  random,
  Sequence,
  spring,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { Audio } from "@remotion/media";
import { loadFont } from "@remotion/google-fonts/Inter";
import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono";
import { measureText } from "@remotion/layout-utils";
import { SceneBackground } from "../SceneBackground";

const { fontFamily } = loadFont("normal", { weights: ["400", "600"] });
const { fontFamily: monoFamily } = loadMono("normal", {
  weights: ["400", "700"],
});

// The app screenshots are 2000x1199; keep that aspect when fitting to frame.
const SCREEN_RATIO = 2000 / 1199;
export const APPFLOW_FRAMES = 1060;

// Head-turn exit at the very end.
const HEAD_TURN_START = 589;
const HEAD_TURN_END = 613;

// Solana workbench: as the zoom finishes it flows straight into a downward pan
// that first opens the Solana Workbench from agent chat, then keeps moving
// through two sidebar clicks. It starts brisk, clicks Personas (2nd tab), holds
// that pace briefly, then accelerates down to the Wallet button near the bottom.
const SOL_OPEN_CLICK = 650; // click Solana Workbench toolbar icon
const SOL_CURSOR_START = 628; // combined frame 832 (after LogoReveal)
const SOL_WORKBENCH_SHOW = SOL_OPEN_CLICK + 4;
const SOL_CAPTION_START = SOL_WORKBENCH_SHOW + 4;
const SOL_CLICK = 688; // click Personas (2nd tab) -> swap to bench_2
const SOL_CLICK2 = 734; // click Wallet button (lower) -> swap to bench_3
const SOL_FINAL_PULLBACK_START = SOL_CLICK2 + 21;
const SOL_FINAL_PULLBACK_END = SOL_FINAL_PULLBACK_START + 30;
const SOL_OPEN_ICON = { fx: 0.946, fy: 0.044 }; // Solana Workbench toolbar icon
const SOL_TAB2 = { fx: 0.739, fy: 0.192 }; // Personas tab, image fraction
const SOL_TAB3 = { fx: 0.743, fy: 0.592 }; // Wallet button, image fraction
const SOL_TAB_NAMES = [
  "Cluster",
  "Personas",
  "Scenarios",
  "Tx",
  "Logs",
  "Indexer",
  "IDL",
  "Deploy",
  "Audit",
  "Token",
  "Wallet",
  "Safety",
  "RPC",
];
const SOL_TAB_NAME_START = SOL_WORKBENCH_SHOW;
const SOL_TAB_NAME_STRIDE = 18;
const SOL_TAB_NAME_TRANSITION = 7;
const CLOSEOUT_START = SOL_FINAL_PULLBACK_START - 8;
const CLOSEOUT_LAYER_TOP = -400;
const FINAL_ZOOM_START = SOL_FINAL_PULLBACK_END + 10;
const FINAL_ZOOM_END = FINAL_ZOOM_START + 36;
const FINAL_SHOVE_START = FINAL_ZOOM_END + 10;
const FINAL_DOMAIN_REVEAL_START = FINAL_SHOVE_START + 28;
const FINAL_BACKGROUND_HOLD_FRAMES = 45;
const FINAL_DOMAIN_GLITCH_OUT_FRAMES = 12;
const FINAL_DOMAIN_CLEAR_START = APPFLOW_FRAMES - FINAL_BACKGROUND_HOLD_FRAMES;
const FINAL_DOMAIN_GLITCH_OUT_START =
  FINAL_DOMAIN_CLEAR_START - FINAL_DOMAIN_GLITCH_OUT_FRAMES;
const XERO_MARK_QUADRANTS = [
  {
    d: "M182.98 182.984L0.000640869 182.984L0.000629244 50.0041C0.00062683 22.3898 22.3864 0.00413391 50.0006 0.0041315L182.98 0.00411987L182.98 182.984Z",
    fill: "#D4A574",
  },
  {
    d: "M237.02 0L370 0C397.614 0 420 22.3858 420 50V182.98H237.02V0Z",
    fill: "#4E4337",
  },
  {
    d: "M237.02 237.023H419.999V370.004C419.999 397.618 397.614 420.004 369.999 420.004H237.02V237.023Z",
    fill: "#D4A574",
  },
  {
    d: "M0 237.023H182.98V420.004H50C22.3857 420.004 0 397.618 0 370.004L0 237.023Z",
    fill: "#4E4337",
  },
] as const;

// Beat timing (frames @ 30fps).
const CLICK1 = 38; // click "Create agent"
const CLICK2 = 68; // click "New agent"
const CANVAS_IN = 72;

// Scene-3 hand-off (no cut): the list slides out while the app flattens and
// zooms into the top-left tabs, clicks "Agent", and the screen swaps.
const T3_START = 150;
const CLICK3 = 194; // click the "Agent" tab
const AGENT_CURSOR_START = CLICK3 - 20;
const AGENT_CURSOR_VISIBLE_START = AGENT_CURSOR_START;
const AGENT_CURSOR_GLITCH_DELAY = 11;
const AGENT_TAB = { x: 15.7, y: 4.4 };
const AGENT_FROM = { x: 36.5, y: 15.2 };

// Typed prompt in the composer (placeholder is covered with its own bg color).
const TYPE_START = 240;
const TYPE_TEXT = "Tell me about this project";
const TYPING_FRAMES = 32; // total typing duration (kept fixed; jitter is relative)

// Per-character reveal times with subtle human jitter (slightly uneven
// keystrokes, a small beat after each space), normalised to TYPING_FRAMES.
const KEY_TIMES = (() => {
  const raw: number[] = [];
  let acc = 0;
  for (let i = 0; i < TYPE_TEXT.length; i++) {
    let d = 1.8 + random(`key-${i}`) * 0.6; // 1.8–2.4 frames between keys
    if (i > 0 && TYPE_TEXT[i - 1] === " ") d += 0.9; // small pause for a new word
    acc += d;
    raw.push(acc);
  }
  return raw.map((v) => (v / acc) * TYPING_FRAMES);
})();
const COMPOSER = {
  coverLeft: 28.8,
  coverTop: 83.1,
  coverWidth: 42,
  coverHeight: 3.7,
  textLeft: 29.2,
  textTop: 85.0,
};

// Cursor waypoints + click targets, as % of the app area.
const INITIAL_CURSOR_START = 12;
const START = { x: 62, y: 72 };
const CREATE_AGENT = { x: 51.5, y: 55.7 };
const NEW_AGENT = { x: 50.4, y: 47.9 };

// "Create agent" modal panel bounds, as % of the app image.
const MODAL = { left: 34.85, top: 31.6, right: 65.95, bottom: 67.5 };

// Per-segment eased keyframe interpolation. A steep S-curve (slow anticipation,
// strong acceleration through the middle, quick settle) gives the camera moves
// punch and depth rather than a flat glide.
const CAM_EASE = Easing.bezier(0.78, 0, 0.2, 1);
const CURSOR_SIZE = 96;
const CURSOR_TIP_X = (3 / 24) * CURSOR_SIZE;
const CURSOR_TIP_Y = (2 / 24) * CURSOR_SIZE;
const CLICK_RIPPLE_SIZE = 64;
const CLICK_RIPPLE_FRAMES = 16;
const CURSOR_GLITCH_DELAY = 6;
const CURSOR_GLITCH_FRAMES = 11;
const CURSOR_PATH =
  "M3 2 L3 20.5 L8.2 15.4 L11.5 22 L14.1 20.8 L10.8 14.4 L17.4 14.4 Z";

const kf = (frame: number, times: number[], values: number[]) => {
  if (frame <= times[0]) return values[0];
  for (let i = 0; i < times.length - 1; i++) {
    if (frame <= times[i + 1]) {
      return interpolate(
        frame,
        [times[i], times[i + 1]],
        [values[i], values[i + 1]],
        {
          extrapolateLeft: "clamp",
          extrapolateRight: "clamp",
          easing: CAM_EASE,
        },
      );
    }
  }
  return values[values.length - 1];
};

type CursorExitClick = number | { at: number; delay?: number };

const cursorGlitchOutState = (progress: number) => {
  if (progress <= 0) {
    return { glitch: 0, opacity: 1 };
  }

  return {
    glitch: interpolate(progress, [0, 0.28, 1], [0.3, 1, 0.75], {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    }),
    opacity: interpolate(progress, [0, 0.45, 1], [1, 0.38, 0], {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    }),
  };
};

const cursorExitAt = (frame: number, clicks: CursorExitClick[]) => {
  const progress = Math.max(
    0,
    ...clicks.map((click) => {
      const at = typeof click === "number" ? click : click.at;
      const delay =
        typeof click === "number"
          ? CURSOR_GLITCH_DELAY
          : (click.delay ?? CURSOR_GLITCH_DELAY);
      const start = at + delay;
      const end = start + CURSOR_GLITCH_FRAMES;
      if (frame < start || frame > end) {
        return 0;
      }
      return interpolate(frame, [start, end], [0, 1], {
        extrapolateLeft: "clamp",
        extrapolateRight: "clamp",
      });
    }),
  );

  return cursorGlitchOutState(progress);
};

const cursorEnterAt = (frame: number, start: number) => {
  if (frame < start) {
    return { glitch: 0, opacity: 0 };
  }

  const progress = interpolate(
    frame,
    [start, start + CURSOR_GLITCH_FRAMES],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    },
  );

  return cursorGlitchOutState(1 - progress);
};

const CursorGlyph: React.FC<{ glitch: number; seed: number }> = ({
  glitch,
  seed,
}) => {
  const svg = (
    style?: React.CSSProperties,
    fill = "#ffffff",
    stroke = "#111111",
  ) => (
    <svg
      width={CURSOR_SIZE}
      height={CURSOR_SIZE}
      viewBox="0 0 24 24"
      style={style}
    >
      <path
        d={CURSOR_PATH}
        fill={fill}
        stroke={stroke}
        strokeWidth={1.3}
        strokeLinejoin="round"
      />
    </svg>
  );

  if (glitch <= 0.001) {
    return svg();
  }

  const rnd = (k: string) => random(`cursor-${seed}-${k}`);
  const signed = (k: string) => rnd(k) * 2 - 1;
  const split = (2 + rnd("split") * 5) * glitch;
  const jitterX = signed("jitter-x") * 3.8 * glitch;
  const jitterY = signed("jitter-y") * 2.4 * glitch;
  const flicker = 1 - rnd("flicker") * 0.28 * glitch;
  const overlay = (
    color: string,
    dx: number,
    dy: number,
  ): React.CSSProperties => ({
    position: "absolute",
    top: 0,
    left: 0,
    color,
    mixBlendMode: "screen",
    opacity: 0.85,
    transform: `translate(${dx}px, ${dy}px)`,
  });
  const slices = [0, 1, 2, 3, 4]
    .filter((k) => rnd(`slice-on-${k}`) < 0.7)
    .map((k) => {
      const top = Math.round(rnd(`slice-top-${k}`) * 78);
      const h = 5 + Math.round(rnd(`slice-h-${k}`) * 18);
      const dx = signed(`slice-dx-${k}`) * 16 * glitch;
      const dy = signed(`slice-dy-${k}`) * 2.5 * glitch;
      return { k, top, bottom: Math.max(0, 100 - top - h), dx, dy };
    });

  return (
    <div
      style={{
        position: "relative",
        width: CURSOR_SIZE,
        height: CURSOR_SIZE,
        opacity: flicker,
        transform: `translate(${jitterX}px, ${jitterY}px)`,
      }}
    >
      {svg({ position: "relative", display: "block", opacity: 0.96 })}
      {svg(
        overlay("#ff0000", split, signed("red-y") * 1.5 * glitch),
        "#ff0000",
        "#380000",
      )}
      {svg(
        overlay("#0000ff", -split, signed("blue-y") * 1.5 * glitch),
        "#0000ff",
        "#000038",
      )}
      {slices.map((s) => (
        <div
          key={s.k}
          style={{
            position: "absolute",
            inset: 0,
            clipPath: `inset(${s.top}% 0 ${s.bottom}% 0)`,
            transform: `translate(${s.dx}px, ${s.dy}px)`,
          }}
        >
          {svg()}
        </div>
      ))}
    </div>
  );
};

const Cursor: React.FC<{
  x: number;
  y: number;
  press: number;
  cam: number;
  glitch: number;
  opacity: number;
  seed: number;
}> = ({ x, y, press, cam, glitch, opacity, seed }) => (
  <div
    style={{
      position: "absolute",
      left: `${x}%`,
      top: `${y}%`,
      transform: `scale(${press / cam})`,
      transformOrigin: "top left",
      opacity,
      filter: "drop-shadow(0 2px 3px rgba(0,0,0,0.55))",
    }}
  >
    <CursorGlyph glitch={glitch} seed={seed} />
  </div>
);

const ClickRipple: React.FC<{
  x: number;
  y: number;
  at: number;
  cam: number;
}> = ({ x, y, at, cam }) => {
  const frame = useCurrentFrame();
  if (frame < at || frame > at + CLICK_RIPPLE_FRAMES) {
    return null;
  }
  const p = interpolate(frame, [at, at + CLICK_RIPPLE_FRAMES], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  return (
    <div
      style={{
        position: "absolute",
        left: `${x}%`,
        top: `${y}%`,
        width: CLICK_RIPPLE_SIZE,
        height: CLICK_RIPPLE_SIZE,
        marginLeft: -CLICK_RIPPLE_SIZE / 2,
        marginTop: -CLICK_RIPPLE_SIZE / 2,
        borderRadius: "50%",
        border: "3px solid rgba(212,165,116,0.95)",
        transform: `scale(${interpolate(p, [0, 1], [0.22, 3]) / cam})`,
        opacity: interpolate(p, [0, 1], [0.72, 0]),
      }}
    />
  );
};

const Caption: React.FC = () => {
  const frame = useCurrentFrame();
  const capIn = interpolate(frame, [10, 22], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.out(Easing.cubic),
  });
  const capOut = interpolate(frame, [66, 80], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.in(Easing.cubic),
  });
  const opacity = capIn * (1 - capOut);
  const y = (1 - capIn) * 28 + capOut * -22;

  return (
    <div
      style={{
        position: "absolute",
        left: 88,
        bottom: 84,
        display: "flex",
        alignItems: "center",
        gap: 20,
        opacity,
        transform: `translateY(${y}px)`,
      }}
    >
      <div
        style={{
          width: 5,
          height: 56,
          borderRadius: 3,
          backgroundColor: "#D4A574",
        }}
      />
      <span
        style={{
          fontFamily,
          fontWeight: 600,
          fontSize: 54,
          lineHeight: 1,
          color: "#ffffff",
          whiteSpace: "nowrap",
        }}
      >
        Create Agents
      </span>
    </div>
  );
};

// Top-left caption shown as the view transitions to the Agent tab.
const ChatCaption: React.FC = () => {
  const frame = useCurrentFrame();
  const capIn = interpolate(frame, [163, 173], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.out(Easing.cubic),
  });
  const capOut = interpolate(frame, [228, 240], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.in(Easing.cubic),
  });
  const opacity = capIn * (1 - capOut);
  const y = (1 - capIn) * -24 - capOut * 18;

  return (
    <div
      style={{
        position: "absolute",
        left: 88,
        top: 84,
        display: "flex",
        alignItems: "center",
        gap: 20,
        opacity,
        transform: `translateY(${y}px)`,
      }}
    >
      <div
        style={{
          width: 5,
          height: 56,
          borderRadius: 3,
          backgroundColor: "#D4A574",
        }}
      />
      <span
        style={{
          fontFamily,
          fontWeight: 600,
          fontSize: 54,
          lineHeight: 1,
          color: "#ffffff",
          whiteSpace: "nowrap",
        }}
      >
        Chat with agents in the app
      </span>
    </div>
  );
};

const SolanaTabName: React.FC = () => {
  const frame = useCurrentFrame();
  const elapsed = Math.max(0, frame - SOL_TAB_NAME_START);
  const index =
    Math.floor(elapsed / SOL_TAB_NAME_STRIDE) % SOL_TAB_NAMES.length;
  const previousIndex =
    (index + SOL_TAB_NAMES.length - 1) % SOL_TAB_NAMES.length;
  const label = SOL_TAB_NAMES[index];
  const firstCycle = elapsed < SOL_TAB_NAME_STRIDE;
  const previousLabel = firstCycle ? label : SOL_TAB_NAMES[previousIndex];
  const local = elapsed % SOL_TAB_NAME_STRIDE;
  const swap = interpolate(local, [0, SOL_TAB_NAME_TRANSITION], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.out(Easing.cubic),
  });
  const nameSwap = firstCycle ? 1 : swap;
  const textStyle: React.CSSProperties = {
    position: "absolute",
    inset: 0,
    fontFamily,
    fontStyle: "italic",
    fontWeight: 400,
    fontSize: 32,
    lineHeight: "36px",
    letterSpacing: 0,
    whiteSpace: "nowrap",
  };

  return (
    <span
      style={{
        position: "relative",
        display: "block",
        width: 360,
        height: 36,
        overflow: "hidden",
      }}
    >
      <span
        style={{
          ...textStyle,
          color: "rgba(255,255,255,0.42)",
          opacity: 1 - nameSwap,
          transform: `translateY(${-10 * nameSwap}px)`,
        }}
      >
        {previousLabel}
      </span>
      <span
        style={{
          ...textStyle,
          color: "rgba(255,255,255,0.74)",
          opacity: nameSwap,
          transform: `translateY(${10 * (1 - nameSwap)}px)`,
        }}
      >
        {label}
      </span>
    </span>
  );
};

// Bottom-left caption that animates in as the solana zoom settles.
const SolanaCaption: React.FC = () => {
  const frame = useCurrentFrame();
  const capIn = interpolate(
    frame,
    [SOL_CAPTION_START, SOL_CAPTION_START + 14],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    },
  );
  const capOut = interpolate(
    frame,
    [SOL_FINAL_PULLBACK_START - 5, SOL_FINAL_PULLBACK_START + 2],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.in(Easing.cubic),
    },
  );

  return (
    <div
      style={{
        position: "absolute",
        left: 88,
        bottom: 84,
        display: "flex",
        alignItems: "flex-start",
        gap: 20,
        opacity: capIn * (1 - capOut),
        transform: `translateY(${(1 - capIn) * 28 - capOut * 16}px)`,
      }}
    >
      <div
        style={{
          width: 5,
          height: 92,
          borderRadius: 3,
          backgroundColor: "#D4A574",
        }}
      />
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 9,
        }}
      >
        <span
          style={{
            fontFamily,
            fontWeight: 600,
            fontSize: 54,
            lineHeight: 1,
            color: "#ffffff",
            whiteSpace: "nowrap",
          }}
        >
          Solana workbench
        </span>
        <SolanaTabName />
      </div>
    </div>
  );
};

const CloseoutGlitchText: React.FC<{
  children: React.ReactNode;
  fontSize: number;
  intensity: number;
  seed: number;
  color?: string;
  fontWeight?: number;
  fontStyle?: React.CSSProperties["fontStyle"];
  style?: React.CSSProperties;
}> = ({
  children,
  fontSize,
  intensity,
  seed,
  color = "#ffffff",
  fontWeight = 600,
  fontStyle,
  style,
}) => {
  const base: React.CSSProperties = {
    fontFamily,
    fontWeight,
    fontStyle,
    fontSize,
    lineHeight: 1,
    letterSpacing: 0,
    whiteSpace: "nowrap",
    ...style,
  };

  if (intensity <= 0.001) {
    return <span style={{ ...base, color }}>{children}</span>;
  }

  const rnd = (k: string) => random(`closeout-${seed}-${k}`);
  const signed = (k: string) => rnd(k) * 2 - 1;

  const shift = (0.04 + 0.05 * rnd("rgb")) * fontSize * intensity;
  const ry = signed("ry") * 0.028 * fontSize * intensity;
  const baseJitter = signed("bx") * 0.045 * fontSize * intensity;
  const flicker = 1 - rnd("flk") * 0.35 * intensity;
  const slices = [0, 1, 2, 3, 4]
    .filter((k) => rnd(`slice-on-${k}`) < 0.72)
    .map((k) => {
      const top = Math.round(rnd(`slice-top-${k}`) * 76);
      const h = 4 + Math.round(rnd(`slice-h-${k}`) * 18);
      const dx = signed(`slice-dx-${k}`) * 0.24 * fontSize * intensity;
      const dy = signed(`slice-dy-${k}`) * 0.02 * fontSize * intensity;
      return { k, top, bottom: Math.max(0, 100 - top - h), dx, dy };
    });
  const layer = (
    layerColor: string,
    dx: number,
    dy: number,
    extra?: React.CSSProperties,
  ): React.CSSProperties => ({
    ...base,
    color: layerColor,
    position: "absolute",
    top: 0,
    left: 0,
    mixBlendMode: "screen",
    transform: `translate(${dx}px, ${dy}px)`,
    ...extra,
  });

  return (
    <span
      style={{
        ...base,
        position: "relative",
        display: "inline-block",
        isolation: "isolate",
        color,
        opacity: flicker,
        transform: `translateX(${baseJitter}px)`,
      }}
    >
      <span
        style={{
          ...base,
          color: "#00ff00",
          display: "block",
          position: "relative",
          mixBlendMode: "screen",
        }}
      >
        {children}
      </span>
      <span style={layer("#ff0000", shift, ry)}>{children}</span>
      <span style={layer("#0000ff", -shift, -ry)}>{children}</span>
      {slices.map((slice) => (
        <span
          key={slice.k}
          style={layer("#ffffff", slice.dx, slice.dy, {
            clipPath: `inset(${slice.top}% 0 ${slice.bottom}% 0)`,
          })}
        >
          {children}
        </span>
      ))}
    </span>
  );
};

const CloseoutMark: React.FC<{ size: number; opacity: number }> = ({
  size,
  opacity,
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 420 420"
    fill="none"
    style={{
      opacity,
      filter: "drop-shadow(0 18px 42px rgba(0,0,0,0.38))",
    }}
  >
    {XERO_MARK_QUADRANTS.map((quadrant) => (
      <path key={quadrant.d} d={quadrant.d} fill={quadrant.fill} />
    ))}
  </svg>
);

const Closeout: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  if (frame < CLOSEOUT_START) {
    return null;
  }

  const markSize = 88;
  const wordFontSize = 102;
  const gap = 32;
  const wordWeight = 600;
  const xeroWidth = measureText({
    text: "xero",
    fontFamily,
    fontSize: wordFontSize,
    fontWeight: wordWeight,
    letterSpacing: "0px",
  }).width;
  const suffixWidth = measureText({
    text: "shell.com",
    fontFamily,
    fontSize: wordFontSize,
    fontWeight: wordWeight,
    letterSpacing: "0px",
  }).width;
  const startWidth = markSize + gap + xeroWidth;
  const finalTextWidth = xeroWidth + suffixWidth;
  const endWidth = startWidth + suffixWidth;
  const finalShove = spring({
    frame: frame - FINAL_SHOVE_START,
    fps,
    config: { mass: 0.4, damping: 9, stiffness: 320 },
  });
  const domainReveal = interpolate(
    frame,
    [FINAL_DOMAIN_REVEAL_START, FINAL_DOMAIN_REVEAL_START + 22],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    },
  );
  const textStartX = -startWidth / 2 + markSize + gap;
  const textCenteredX = -xeroWidth / 2;
  const domainCenteredX = -finalTextWidth / 2;
  const textX =
    textStartX +
    (textCenteredX - textStartX) * finalShove +
    (domainCenteredX - textCenteredX) * domainReveal;
  const logoEjectX = interpolate(
    frame,
    [FINAL_SHOVE_START + 2, FINAL_SHOVE_START + 16],
    [0, -markSize * 2.6],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    },
  );
  const logoOpacity = interpolate(
    frame,
    [FINAL_SHOVE_START + 6, FINAL_SHOVE_START + 15],
    [1, 0],
    { extrapolateLeft: "clamp", extrapolateRight: "clamp" },
  );
  const markX = -startWidth / 2 + logoEjectX;
  const suffixReveal = interpolate(
    frame,
    [FINAL_DOMAIN_REVEAL_START, FINAL_DOMAIN_REVEAL_START + 22],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    },
  );
  const domainRevealGlitch = interpolate(
    frame,
    [
      FINAL_DOMAIN_REVEAL_START,
      FINAL_DOMAIN_REVEAL_START + 8,
      FINAL_DOMAIN_REVEAL_START + 24,
    ],
    [0, 1.1, 0],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    },
  );
  const domainExitGlitch = interpolate(
    frame,
    [FINAL_DOMAIN_GLITCH_OUT_START, FINAL_DOMAIN_GLITCH_OUT_START + 6],
    [0, 1.15],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    },
  );
  const domainExitFade = interpolate(
    frame,
    [FINAL_DOMAIN_GLITCH_OUT_START + 2, FINAL_DOMAIN_CLEAR_START],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.in(Easing.cubic),
    },
  );
  const domainGlitch = Math.max(domainRevealGlitch, domainExitGlitch);
  const xeroGlitch = Math.max(domainRevealGlitch * 0.3, domainExitGlitch);
  const taglineGlitchCycle = (frame - CLOSEOUT_START + 9999) % 16;
  const taglineGlitch = interpolate(
    taglineGlitchCycle,
    [0, 5, 16],
    [0.95, 0.3, 0.3],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    },
  );
  const finalSloganFade = interpolate(
    frame,
    [FINAL_ZOOM_START, FINAL_ZOOM_START + 20],
    [1, 0],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    },
  );
  const finalSloganGlitch = interpolate(
    frame,
    [FINAL_ZOOM_START, FINAL_ZOOM_START + 9, FINAL_ZOOM_START + 20],
    [taglineGlitch, 1.15, 0.55],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    },
  );

  return (
    <div
      style={{
        position: "absolute",
        top: CLOSEOUT_LAYER_TOP,
        left: 0,
        right: 0,
        pointerEvents: "none",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        fontFamily,
      }}
    >
      <div
        style={{
          position: "relative",
          width: endWidth,
          height: wordFontSize,
        }}
      >
        <div
          style={{
            position: "absolute",
            left: `calc(50% + ${markX}px)`,
            top: 7,
          }}
        >
          <CloseoutMark size={markSize} opacity={logoOpacity} />
        </div>
        <div
          style={{
            position: "absolute",
            left: `calc(50% + ${textX}px)`,
            top: -7,
            width: xeroWidth + suffixWidth,
            height: wordFontSize,
            opacity: 1 - domainExitFade,
          }}
        >
          <CloseoutGlitchText
            fontSize={wordFontSize}
            intensity={xeroGlitch}
            seed={frame}
          >
            xero
          </CloseoutGlitchText>
          <div
            style={{
              position: "absolute",
              left: xeroWidth,
              top: 0,
              width: suffixWidth,
              height: wordFontSize,
              overflow: "hidden",
              opacity: suffixReveal,
              transform: `translateX(${(1 - suffixReveal) * 26}px)`,
              clipPath: `inset(0 ${(1 - suffixReveal) * 100}% 0 0)`,
            }}
          >
            <CloseoutGlitchText
              fontSize={wordFontSize}
              intensity={domainGlitch}
              seed={frame + 913}
            >
              shell.com
            </CloseoutGlitchText>
          </div>
        </div>
      </div>
      <div
        style={{
          marginTop: 30,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          opacity: finalSloganFade,
        }}
      >
        <CloseoutGlitchText
          fontSize={62}
          intensity={finalSloganGlitch}
          seed={frame}
          color="rgba(245,245,245,0.92)"
        >
          One harness. Every surface.
        </CloseoutGlitchText>
      </div>
    </div>
  );
};

// Feature list revealed on the left once the canvas slides right.
const PANEL_HEAD = 92;
const PANEL_ITEMS = 100;
const PANEL_STAGGER = 6;
const PANEL_OUT = 150; // list slides out as the scene-3 hand-off begins
const FEATURES = [
  "Attach tools",
  "Custom gates",
  "Configure memory",
  "Attach skills",
  "Define output",
];

const riseIn = (frame: number, start: number) => {
  const t = interpolate(frame, [start, start + 11], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.out(Easing.cubic),
  });
  return { opacity: t, dx: (1 - t) * -26 };
};

const Chevron: React.FC = () => (
  <svg width={22} height={22} viewBox="0 0 24 24" fill="none">
    <path
      d="M9 6l6 6-6 6"
      stroke="#D4A574"
      strokeWidth={2.6}
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const FeaturePanel: React.FC = () => {
  const frame = useCurrentFrame();
  if (frame < PANEL_HEAD) {
    return null;
  }
  const head = riseIn(frame, PANEL_HEAD);
  const out = interpolate(frame, [PANEL_OUT, PANEL_OUT + 14], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.in(Easing.cubic),
  });

  return (
    <div
      style={{
        position: "absolute",
        left: 96,
        top: 0,
        bottom: 0,
        width: 760,
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        fontFamily,
        opacity: 1 - out,
        transform: `translateX(${-out * 60}px)`,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 20,
          marginBottom: 46,
          opacity: head.opacity,
          transform: `translateX(${head.dx}px)`,
        }}
      >
        <div
          style={{
            width: 5,
            height: 58,
            borderRadius: 3,
            backgroundColor: "#D4A574",
          }}
        />
        <span style={{ fontWeight: 600, fontSize: 56, color: "#ffffff" }}>
          Create Agents
        </span>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 26 }}>
        {FEATURES.map((label, i) => {
          const { opacity, dx } = riseIn(
            frame,
            PANEL_ITEMS + i * PANEL_STAGGER,
          );
          return (
            <div
              key={label}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 18,
                opacity,
                transform: `translateX(${dx}px)`,
              }}
            >
              <Chevron />
              <span style={{ fontWeight: 600, fontSize: 34, color: "#dcdcdc" }}>
                {label}
              </span>
            </div>
          );
        })}
        {(() => {
          const { opacity, dx } = riseIn(
            frame,
            PANEL_ITEMS + FEATURES.length * PANEL_STAGGER,
          );
          return (
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 18,
                opacity,
                transform: `translateX(${dx}px)`,
              }}
            >
              <Chevron />
              <span
                style={{
                  fontWeight: 600,
                  fontSize: 34,
                  color: "#b6a892",
                  fontStyle: "italic",
                }}
              >
                and more
              </span>
            </div>
          );
        })()}
      </div>
    </div>
  );
};

// Types the prompt into the composer. The static placeholder is hidden under a
// rect filled with the input's own background colour.
const ComposerType: React.FC = () => {
  const frame = useCurrentFrame();
  if (frame < TYPE_START) {
    return null;
  }
  const elapsed = frame - TYPE_START;
  let chars = 0;
  for (let i = 0; i < KEY_TIMES.length; i++) {
    if (elapsed >= KEY_TIMES[i]) chars = i + 1;
    else break;
  }
  const shown = TYPE_TEXT.slice(0, chars);
  const typing = chars < TYPE_TEXT.length;
  const caretOn = typing || Math.floor(frame / 8) % 2 === 0;

  return (
    <>
      <div
        style={{
          position: "absolute",
          left: `${COMPOSER.coverLeft}%`,
          top: `${COMPOSER.coverTop}%`,
          width: `${COMPOSER.coverWidth}%`,
          height: `${COMPOSER.coverHeight}%`,
          backgroundColor: "#191919",
        }}
      />
      <div
        style={{
          position: "absolute",
          left: `${COMPOSER.textLeft}%`,
          top: `${COMPOSER.textTop}%`,
          transform: "translateY(-50%)",
          display: "flex",
          alignItems: "center",
          fontFamily,
          fontWeight: 400,
          fontSize: 17,
          color: "#ededed",
          whiteSpace: "nowrap",
        }}
      >
        <span>{shown}</span>
        <span
          style={{
            width: 1.5,
            height: 20,
            marginLeft: 2,
            backgroundColor: "#D4A574",
            opacity: caretOn ? 1 : 0,
          }}
        />
      </div>
    </>
  );
};

// After the prompt is "sent": pan up to the chat area, mask the static empty
// state, and stream an agent conversation into it.
const REVEAL_START = 274;
const CONV_START = 298;

const appear = (frame: number, start: number, dur = 6): React.CSSProperties => {
  const t = interpolate(frame, [start, start + dur], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.out(Easing.cubic),
  });
  // Each element lifts off the surface as it streams: rises, scales up, and
  // unfolds (rotateX) into place — clear 3D elevation rather than a flat fade.
  return {
    opacity: t,
    transform: `perspective(1000px) translateY(${(1 - t) * 30}px) scale(${0.9 + 0.1 * t}) rotateX(${(1 - t) * -24}deg)`,
    transformOrigin: "center top",
  };
};

// Sizes mirror the real app (@xero/ui transcript), converted from CSS px to
// this scene's content space (the screenshots are 2x retina): content ≈ css * 1.27.
const MUTED = "#8b8b8b";

const Check: React.FC<{ size?: number }> = ({ size = 20 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
    <circle cx="12" cy="12" r="9.5" stroke="#4ade80" strokeWidth="1.7" />
    <path
      d="M8.5 12.4l2.4 2.4 4.6-5.2"
      stroke="#4ade80"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const BrainIcon: React.FC<{ size?: number }> = ({ size = 18 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
    <path
      d="M10 4.5a3 3 0 0 0-3 3 3 3 0 0 0-1.2 5.4A3 3 0 0 0 9 18.5h1z"
      stroke="rgba(212,165,116,0.7)"
      strokeWidth="1.6"
      strokeLinejoin="round"
    />
    <path
      d="M14 4.5a3 3 0 0 1 3 3 3 3 0 0 1 1.2 5.4A3 3 0 0 1 15 18.5h-1z"
      stroke="rgba(212,165,116,0.7)"
      strokeWidth="1.6"
      strokeLinejoin="round"
    />
  </svg>
);

const ToolChevron: React.FC<{ size?: number; up?: boolean }> = ({
  size = 18,
  up,
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    style={{ transform: up ? "rotate(180deg)" : undefined, flexShrink: 0 }}
  >
    <path
      d="M6 9l6 6 6-6"
      stroke="rgba(140,140,140,0.5)"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const TOOLS: [string, string][] = [
  ["list tree", "Listed tree for `.` — 64 files, 18 dirs."],
  ["read README.md", "Read 96 lines from `README.md`."],
  ["read package.json", "Read 41 lines from `package.json`."],
  ["grep", "Found 9 matches for `agent`."],
];

const Conversation: React.FC = () => {
  const frame = useCurrentFrame();
  const f = frame - CONV_START;
  if (f < 0) {
    return null;
  }
  const scrollY = interpolate(f, [28, 100], [0, -210], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.inOut(Easing.cubic),
  });

  return (
    <div
      style={{
        position: "absolute",
        left: "52%",
        top: "16%",
        width: "52.4%",
        fontFamily,
        color: "#e6e6e6",
        // Soft drop-shadow on every line so the whole conversation reads as
        // lifted off the surface, not just the user bubble.
        textShadow: "0 9px 22px rgba(0,0,0,0.7)",
        transform: `translate(-50%, ${scrollY}px)`,
      }}
    >
      {/* user message */}
      <div
        style={{
          display: "flex",
          justifyContent: "flex-end",
          alignItems: "flex-start",
          gap: 13,
          ...appear(frame, CONV_START),
        }}
      >
        <div
          style={{
            borderRadius: 20,
            padding: "10px 18px",
            background: "rgba(212,165,116,0.10)",
            boxShadow:
              "inset 0 0 0 1px rgba(212,165,116,0.4), 0 16px 34px rgba(0,0,0,0.5)",
            fontSize: 18,
            lineHeight: 1.5,
            color: "#f2f2f2",
            maxWidth: "80%",
          }}
        >
          Tell me about this project
        </div>
        <Img
          src={staticFile("avatar.jpg")}
          style={{
            width: 30,
            height: 30,
            borderRadius: "50%",
            objectFit: "cover",
            boxShadow:
              "0 0 0 1px rgba(212,165,116,0.45), 0 14px 30px rgba(0,0,0,0.5)",
            marginTop: 3,
            flexShrink: 0,
          }}
        />
      </div>

      {/* thoughts */}
      <div
        style={{
          marginTop: 30,
          display: "flex",
          alignItems: "center",
          gap: 8,
          ...appear(frame, CONV_START + 8),
        }}
      >
        <BrainIcon />
        <span
          style={{
            fontSize: 14.5,
            letterSpacing: "0.07em",
            color: "rgba(150,150,150,0.9)",
            fontWeight: 600,
            textTransform: "uppercase",
          }}
        >
          Thoughts
        </span>
      </div>
      <div
        style={{
          marginTop: 10,
          fontSize: 18,
          fontStyle: "italic",
          fontWeight: 700,
          color: "#e6e6e6",
          ...appear(frame, CONV_START + 12),
        }}
      >
        Getting my bearings
      </div>
      <div
        style={{
          marginTop: 7,
          fontSize: 17.5,
          lineHeight: 1.6,
          fontStyle: "italic",
          color: MUTED,
        }}
      >
        <div style={appear(frame, CONV_START + 16)}>
          Let me ground this in the actual workspace before answering.
        </div>
        <div style={appear(frame, CONV_START + 20)}>
          I&apos;ll skim the README and manifest, then list the tree to see
        </div>
        <div style={appear(frame, CONV_START + 24)}>
          what actually ships here.
        </div>
      </div>

      {/* tool calls */}
      <div
        style={{
          marginTop: 28,
          display: "flex",
          alignItems: "center",
          gap: 10,
          ...appear(frame, CONV_START + 30),
        }}
      >
        <Check size={20} />
        <span
          style={{
            fontSize: 16.5,
            fontWeight: 500,
            color: "#ededed",
            letterSpacing: "-0.005em",
          }}
        >
          4 tool calls
        </span>
        <span
          style={{ flex: 1, fontSize: 15, color: "rgba(150,150,150,0.75)" }}
        >
          4 succeeded · latest read package.json
        </span>
        <ToolChevron up />
      </div>
      <div
        style={{
          marginTop: 12,
          marginLeft: 9,
          borderLeft: "1px solid rgba(140,140,140,0.3)",
          paddingLeft: 22,
          display: "flex",
          flexDirection: "column",
          gap: 15,
          ...appear(frame, CONV_START + 34),
        }}
      >
        {TOOLS.map(([name, desc], i) => (
          <div
            key={name}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 10,
              ...appear(frame, CONV_START + 36 + i * 4),
            }}
          >
            <Check size={18} />
            <span
              style={{
                fontSize: 16,
                fontWeight: 500,
                color: "#dcdcdc",
                letterSpacing: "-0.005em",
              }}
            >
              {name}
            </span>
            <span
              style={{ flex: 1, fontSize: 15, color: "rgba(150,150,150,0.75)" }}
            >
              {desc}
            </span>
            <ToolChevron />
          </div>
        ))}
      </div>

      {/* response */}
      <div style={{ marginTop: 30, fontSize: 18, lineHeight: 1.6 }}>
        <div style={appear(frame, CONV_START + 56)}>
          <b style={{ color: "#fff" }}>Xero</b> is a desktop harness for
          building, running, and observing AI agents.
        </div>
        <div style={{ marginTop: 14, ...appear(frame, CONV_START + 64) }}>
          You compose agents on a canvas — prompts, tools, skills, and gates —
          then drive them from one place.
        </div>
        <div style={{ marginTop: 14, ...appear(frame, CONV_START + 72) }}>
          Every run is fully observable: live tool calls, reasoning, token
          usage, and cost — all in one view.
        </div>

        <div
          style={{
            marginTop: 16,
            display: "flex",
            flexDirection: "column",
            gap: 10,
          }}
        >
          {[
            ["Reusable agents", "chain them into larger workflows"],
            ["Bring your own models", "run them locally or in the cloud"],
            ["Built-in guardrails", "approvals, gates, and persistent memory"],
          ].map(([lead, rest], i) => (
            <div
              key={lead}
              style={{
                display: "flex",
                gap: 12,
                ...appear(frame, CONV_START + 80 + i * 5),
              }}
            >
              <span style={{ color: "rgba(212,165,116,0.85)" }}>•</span>
              <span>
                <b style={{ color: "#fff" }}>{lead}</b> — {rest}
              </span>
            </div>
          ))}
        </div>

        <div style={{ marginTop: 16, ...appear(frame, CONV_START + 98) }}>
          Go from a single prompt to a production-ready agent in minutes — no
          glue code, no guesswork.
        </div>
      </div>
    </div>
  );
};

const pressDip = (frame: number, at: number) =>
  interpolate(frame, [at - 1, at + 1, at + 4], [0, 1, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

// Left-side panel for the cloud/mobile beat (mirrors the FeaturePanel style).
const CLOUD_HEAD = 434;
const CLOUD_ITEMS = 442;
const CLOUD_OUT = 504; // panel slides out as the combo moves left
const CLOUD_FEATURES = [
  "Open sessions from your phone",
  "Watch runs live, anywhere",
  "Approve & steer on the go",
  "Stays in sync with your Desktop",
];

const CloudPanel: React.FC<{ exitX: number }> = ({ exitX }) => {
  const frame = useCurrentFrame();
  if (frame < CLOUD_HEAD) {
    return null;
  }
  const head = riseIn(frame, CLOUD_HEAD);
  const andMore = riseIn(frame, CLOUD_ITEMS + CLOUD_FEATURES.length * 6);

  return (
    <div
      style={{
        position: "absolute",
        left: 96,
        top: 0,
        bottom: 0,
        width: 560,
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        fontFamily,
        // Slide off-screen left, locked to the camera pan, so the panel stays
        // "attached" to the combo as it sweeps left (no fade, no collision).
        transform: `translateX(${exitX}px)`,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 20,
          marginBottom: 46,
          opacity: head.opacity,
          transform: `translateX(${head.dx}px)`,
        }}
      >
        <div
          style={{
            width: 5,
            height: 58,
            borderRadius: 3,
            backgroundColor: "#D4A574",
          }}
        />
        <span style={{ fontWeight: 600, fontSize: 56, color: "#ffffff" }}>
          Take it on the go
        </span>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 26 }}>
        {CLOUD_FEATURES.map((label, i) => {
          const { opacity, dx } = riseIn(frame, CLOUD_ITEMS + i * 6);
          return (
            <div
              key={label}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 18,
                opacity,
                transform: `translateX(${dx}px)`,
              }}
            >
              <Chevron />
              <span style={{ fontWeight: 600, fontSize: 31, color: "#dcdcdc" }}>
                {label}
              </span>
            </div>
          );
        })}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 18,
            opacity: andMore.opacity,
            transform: `translateX(${andMore.dx}px)`,
          }}
        >
          <Chevron />
          <span
            style={{
              fontWeight: 600,
              fontSize: 31,
              color: "#b6a892",
              fontStyle: "italic",
            }}
          >
            and more
          </span>
        </div>
      </div>
    </div>
  );
};

// ---- Cloud app, framed as an iPhone, attached to the app's tilted plane ----
const PHONE_SHOW = 400; // rendered (off-screen) before the zoom-out reveals it
const PH_SCREEN_W = 800; // cloud.png fills the screen exactly
const PH_SCREEN_H = 1576;
const PH_BEZEL = 18;
const PH_W = PH_SCREEN_W + PH_BEZEL * 2;
const PH_H = PH_SCREEN_H + PH_BEZEL * 2;
// Placed in the app's content space (to the left, overlapping it) so it shares
// the camera zoom + tilt — revealed by the zoom-out as if it was always there.
const PH_CONTENT_SCALE = 0.6;
const PHONE_CX = -190;
const PHONE_CY = 205;

const MOBILE_TOOLS: [string, string][] = [
  ["list tree", "64 files, 18 dirs"],
  ["read README.md", "96 lines"],
  ["read package.json", "41 lines"],
  ["grep", "9 matches"],
];

// The Xero conversation re-rendered at mobile sizes, drawn over cloud.png's
// (covered) chat body. Static — the phone arrives with it already complete.
const MobileConversation: React.FC = () => (
  <div
    style={{
      position: "absolute",
      top: 188,
      left: 58,
      width: 684,
      fontFamily,
      color: "#e6e6e6",
    }}
  >
    <div
      style={{
        display: "flex",
        justifyContent: "flex-end",
        alignItems: "flex-start",
        gap: 18,
      }}
    >
      <div
        style={{
          borderRadius: 36,
          padding: "16px 30px",
          background: "rgba(212,165,116,0.10)",
          boxShadow: "inset 0 0 0 2px rgba(212,165,116,0.4)",
          fontSize: 31,
          lineHeight: 1.45,
          color: "#f2f2f2",
          maxWidth: "76%",
        }}
      >
        Tell me about this project
      </div>
      <Img
        src={staticFile("avatar.jpg")}
        style={{
          width: 56,
          height: 56,
          borderRadius: "50%",
          objectFit: "cover",
          boxShadow: "0 0 0 2px rgba(212,165,116,0.45)",
          marginTop: 4,
          flexShrink: 0,
        }}
      />
    </div>

    <div
      style={{ marginTop: 52, display: "flex", alignItems: "center", gap: 12 }}
    >
      <BrainIcon size={32} />
      <span
        style={{
          fontSize: 25,
          letterSpacing: "0.07em",
          color: "rgba(150,150,150,0.9)",
          fontWeight: 600,
          textTransform: "uppercase",
        }}
      >
        Thoughts
      </span>
    </div>
    <div
      style={{
        marginTop: 14,
        fontSize: 31,
        fontStyle: "italic",
        fontWeight: 700,
        color: "#e6e6e6",
      }}
    >
      Getting my bearings
    </div>
    <div
      style={{
        marginTop: 12,
        fontSize: 29,
        lineHeight: 1.55,
        fontStyle: "italic",
        color: MUTED,
      }}
    >
      Let me ground this in the workspace, then summarize what actually ships
      here.
    </div>

    <div
      style={{ marginTop: 44, display: "flex", alignItems: "center", gap: 16 }}
    >
      <Check size={34} />
      <span style={{ fontSize: 30, fontWeight: 500, color: "#ededed" }}>
        4 tool calls
      </span>
      <span style={{ flex: 1 }} />
      <ToolChevron up size={30} />
    </div>
    <div
      style={{
        marginTop: 20,
        marginLeft: 14,
        borderLeft: "2px solid rgba(140,140,140,0.3)",
        paddingLeft: 30,
        display: "flex",
        flexDirection: "column",
        gap: 24,
      }}
    >
      {MOBILE_TOOLS.map(([name, desc]) => (
        <div
          key={name}
          style={{ display: "flex", alignItems: "center", gap: 14 }}
        >
          <Check size={28} />
          <span style={{ fontSize: 28, fontWeight: 500, color: "#dcdcdc" }}>
            {name}
          </span>
          <span
            style={{
              flex: 1,
              fontSize: 26,
              color: "rgba(150,150,150,0.75)",
              textAlign: "right",
            }}
          >
            {desc}
          </span>
        </div>
      ))}
    </div>

    <div style={{ marginTop: 44, fontSize: 30, lineHeight: 1.55 }}>
      <div>
        <b style={{ color: "#fff" }}>Xero</b> is a desktop harness for building,
        running, and observing AI agents.
      </div>
      <div style={{ marginTop: 18 }}>
        Compose agents on a canvas, then drive them from one place.
      </div>
    </div>
  </div>
);

const StatusBar: React.FC = () => (
  <div
    style={{
      position: "absolute",
      top: 42,
      left: 0,
      width: PH_SCREEN_W,
      height: 40,
      display: "flex",
      alignItems: "center",
      justifyContent: "space-between",
      padding: "0 60px",
    }}
  >
    <span style={{ fontFamily, fontWeight: 600, fontSize: 28, color: "#fff" }}>
      9:41
    </span>
    <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
      <svg width="34" height="24" viewBox="0 0 34 24">
        <g fill="#fff">
          <rect x="0" y="15" width="6" height="9" rx="1.5" />
          <rect x="9" y="11" width="6" height="13" rx="1.5" />
          <rect x="18" y="6" width="6" height="18" rx="1.5" />
          <rect x="27" y="0" width="6" height="24" rx="1.5" />
        </g>
      </svg>
      <svg width="46" height="24" viewBox="0 0 46 24">
        <rect
          x="1"
          y="3"
          width="38"
          height="18"
          rx="5"
          fill="none"
          stroke="#fff"
          strokeWidth="2"
          opacity="0.45"
        />
        <rect x="4" y="6" width="30" height="12" rx="2.5" fill="#fff" />
        <rect
          x="42"
          y="9"
          width="3"
          height="6"
          rx="1.5"
          fill="#fff"
          opacity="0.45"
        />
      </svg>
    </div>
  </div>
);

const MobileHeader: React.FC = () => (
  <div
    style={{
      position: "absolute",
      top: 110,
      left: 0,
      width: PH_SCREEN_W,
      height: 44,
      display: "flex",
      alignItems: "center",
      justifyContent: "space-between",
      padding: "0 54px",
    }}
  >
    <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
      <div
        style={{
          width: 26,
          height: 26,
          display: "grid",
          gridTemplateColumns: "1fr 1fr",
          gridTemplateRows: "1fr 1fr",
          gap: 3,
        }}
      >
        <div style={{ background: "#D4A574", borderRadius: 2 }} />
        <div style={{ background: "#4E4337", borderRadius: 2 }} />
        <div style={{ background: "#4E4337", borderRadius: 2 }} />
        <div style={{ background: "#D4A574", borderRadius: 2 }} />
      </div>
      <span style={{ fontFamily, fontSize: 30, color: "#cfcfcf" }}>
        New chat
      </span>
    </div>
    <svg
      width="34"
      height="34"
      viewBox="0 0 24 24"
      fill="none"
      stroke="#cfcfcf"
      strokeWidth="2"
      strokeLinecap="round"
    >
      <path d="M4 7h16M4 12h16M4 17h16" />
    </svg>
  </div>
);

const MobileComposer: React.FC = () => (
  <div
    style={{
      position: "absolute",
      left: 50,
      right: 50,
      bottom: 74,
      borderRadius: 40,
      background: "#1a1a1c",
      boxShadow: "inset 0 0 0 1px #2c2c2f",
      padding: "26px 30px 20px",
      boxSizing: "border-box",
    }}
  >
    <div style={{ fontFamily, fontSize: 30, color: "#737373" }}>
      Ask anything...
    </div>
    <div
      style={{ marginTop: 26, display: "flex", alignItems: "center", gap: 22 }}
    >
      <svg
        width="36"
        height="36"
        viewBox="0 0 24 24"
        fill="none"
        stroke="#9a9a9a"
        strokeWidth="2"
        strokeLinecap="round"
      >
        <path d="M12 5v14M5 12h14" />
      </svg>
      <svg
        width="32"
        height="32"
        viewBox="0 0 24 24"
        fill="none"
        stroke="#9a9a9a"
        strokeWidth="1.8"
      >
        <circle cx="12" cy="12" r="3.2" />
        <path
          d="M12 3.5v2M12 18.5v2M3.5 12h2M18.5 12h2M6 6l1.4 1.4M16.6 16.6l1.4 1.4M18 6l-1.4 1.4M7.4 16.6L6 18"
          strokeLinecap="round"
        />
      </svg>
      <div style={{ flex: 1 }} />
      <div
        style={{
          width: 58,
          height: 58,
          borderRadius: 16,
          background: "rgba(212,165,116,0.16)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <svg width="28" height="28" viewBox="0 0 24 24" fill="#D4A574">
          <path d="M12 2l1.8 5.2L19 9l-5.2 1.8L12 16l-1.8-5.2L5 9l5.2-1.8z" />
        </svg>
      </div>
      <svg
        width="32"
        height="32"
        viewBox="0 0 24 24"
        fill="none"
        stroke="#9a9a9a"
        strokeWidth="1.8"
        strokeLinecap="round"
      >
        <rect x="9" y="3" width="6" height="11" rx="3" />
        <path d="M6 11a6 6 0 0 0 12 0M12 17v3" />
      </svg>
      <div
        style={{
          width: 58,
          height: 58,
          borderRadius: 16,
          background: "#2a2a2d",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <svg
          width="28"
          height="28"
          viewBox="0 0 24 24"
          fill="none"
          stroke="#cfcfcf"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M12 19V5M5 12l7-7 7 7" />
        </svg>
      </div>
    </div>
  </div>
);

const CloudPhone: React.FC = () => (
  <div
    style={{
      width: PH_W,
      height: PH_H,
      borderRadius: 134,
      background:
        "linear-gradient(150deg, #34353b 0%, #18191d 46%, #26272c 100%)",
      boxShadow: "0 50px 120px rgba(0,0,0,0.55)",
      padding: PH_BEZEL,
      boxSizing: "border-box",
    }}
  >
    <div
      style={{
        position: "relative",
        width: PH_SCREEN_W,
        height: PH_SCREEN_H,
        borderRadius: 114,
        overflow: "hidden",
        background: "#121212",
        outline: "3px solid #141417",
      }}
    >
      <Img
        src={staticFile("cloud.png")}
        style={{
          position: "absolute",
          inset: 0,
          width: "100%",
          height: "100%",
        }}
      />
      {/* re-skin the whole screen over cloud.png so it fits the frame cleanly */}
      <div
        style={{
          position: "absolute",
          inset: 0,
          backgroundColor: "#121212",
        }}
      />
      <StatusBar />
      <MobileHeader />
      <MobileConversation />
      <MobileComposer />
      {/* dynamic island */}
      <div
        style={{
          position: "absolute",
          top: 16,
          left: PH_SCREEN_W / 2 - 125,
          width: 250,
          height: 70,
          borderRadius: 38,
          background: "#000000",
        }}
      />
      {/* home indicator */}
      <div
        style={{
          position: "absolute",
          bottom: 22,
          left: PH_SCREEN_W / 2 - 130,
          width: 260,
          height: 10,
          borderRadius: 6,
          background: "rgba(255,255,255,0.55)",
        }}
      />
    </div>
  </div>
);

// Right-side panel for the terminal/TUI beat (mirrors the cloud panel).
const TUI_HEAD = 532;
const TUI_ITEMS = 540;
const TUI_FEATURES = [
  "The full agent in your shell",
  "Same sessions, every surface",
  "Project Snapshots",
  "Headless mode",
];

const TuiPanel: React.FC = () => {
  const frame = useCurrentFrame();
  if (frame < TUI_HEAD) {
    return null;
  }
  const head = riseIn(frame, TUI_HEAD);
  const andMore = riseIn(frame, TUI_ITEMS + TUI_FEATURES.length * 6);

  return (
    <div
      style={{
        position: "absolute",
        left: 1245,
        top: 0,
        bottom: 0,
        width: 640,
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        fontFamily,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 18,
          marginBottom: 36,
          opacity: head.opacity,
          transform: `translateX(${-head.dx}px)`,
        }}
      >
        <div
          style={{
            width: 5,
            height: 52,
            borderRadius: 3,
            backgroundColor: "#D4A574",
          }}
        />
        <span style={{ fontWeight: 600, fontSize: 50, color: "#ffffff" }}>
          Live in the terminal
        </span>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
        {TUI_FEATURES.map((label, i) => {
          const { opacity, dx } = riseIn(frame, TUI_ITEMS + i * 6);
          return (
            <div
              key={label}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 18,
                opacity,
                transform: `translateX(${-dx}px)`,
              }}
            >
              <Chevron />
              <span style={{ fontWeight: 600, fontSize: 29, color: "#dcdcdc" }}>
                {label}
              </span>
            </div>
          );
        })}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 18,
            opacity: andMore.opacity,
            transform: `translateX(${-andMore.dx}px)`,
          }}
        >
          <Chevron />
          <span
            style={{
              fontWeight: 600,
              fontSize: 29,
              color: "#b6a892",
              fontStyle: "italic",
            }}
          >
            and more
          </span>
        </div>
      </div>
    </div>
  );
};

// ---- macOS terminal running the Xero TUI, revealed on the right at the close ----
const TERM_SHOW = 500; // rendered (off-screen right) before the left-pan reveals it
const TERM_W = 1500;
const TERM_H = 1000;
const TERM_CONTENT_SCALE = 0.64;
const TERM_CX = 1560;
const TERM_CY = 545;

// Xero TUI palette + box-drawing logo (verbatim from xero-cli).
const T_FG = "#f8f9fa";
const T_MUTED = "#a8aeb5";
const T_DIM = "#6b6f74";
const T_ACCENT = "#d4a574";
const TUI_LOGO = [
  "██╗  ██╗ ███████╗ ██████╗   ██████╗ ",
  "╚██╗██╔╝ ██╔════╝ ██╔══██╗ ██╔═══██╗",
  " ╚███╔╝  █████╗   ██████╔╝ ██║   ██║",
  " ██╔██╗  ██╔══╝   ██╔══██╗ ██║   ██║",
  "██╔╝ ██╗ ███████╗ ██║  ██║ ╚██████╔╝",
  "╚═╝  ╚═╝ ╚══════╝ ╚═╝  ╚═╝  ╚═════╝ ",
].join("\n");

const TerminalWindow: React.FC = () => (
  <div
    style={{
      width: TERM_W,
      height: TERM_H,
      borderRadius: 18,
      overflow: "hidden",
      background: "#121212",
      boxShadow: "0 50px 120px rgba(0,0,0,0.55)",
      border: "1px solid #2b2b2e",
      display: "flex",
      flexDirection: "column",
      fontFamily: monoFamily,
    }}
  >
    <div
      style={{
        height: 52,
        background: "linear-gradient(#262629, #1d1d20)",
        borderBottom: "1px solid #2b2b2e",
        display: "flex",
        alignItems: "center",
        padding: "0 22px",
        gap: 12,
        position: "relative",
        flexShrink: 0,
      }}
    >
      {["#ff5f57", "#febc2e", "#28c840"].map((c) => (
        <div
          key={c}
          style={{ width: 18, height: 18, borderRadius: "50%", background: c }}
        />
      ))}
      <span
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          textAlign: "center",
          color: "#8b8b8b",
          fontSize: 22,
        }}
      >
        xero — ~/Documents/dev/xero
      </span>
    </div>

    <div
      style={{
        flex: 1,
        background: "#121212",
        padding: "40px 48px 28px",
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
      }}
    >
      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <pre
          style={{
            margin: 0,
            color: T_ACCENT,
            fontSize: 44,
            lineHeight: 1,
            letterSpacing: 0,
            fontWeight: 700,
          }}
        >
          {TUI_LOGO}
        </pre>
        <div style={{ marginTop: 28, color: T_FG, fontSize: 26 }}>
          @xeroshell
        </div>
        <div style={{ marginTop: 6, color: T_DIM, fontSize: 26 }}>v0.1.0</div>
      </div>

      <div
        style={{
          borderLeft: `4px solid ${T_ACCENT}`,
          background: "#1a1a1c",
          padding: "26px 30px",
          display: "flex",
          flexDirection: "column",
          gap: 32,
          fontSize: 26,
        }}
      >
        <span style={{ color: T_DIM }}>Ask anything... "Fix broken tests"</span>
        <span>
          <span style={{ color: T_ACCENT }}>Agent</span>
          <span style={{ color: T_DIM }}> · </span>
          <span style={{ color: T_MUTED }}>gpt-5.5</span>
          <span style={{ color: T_DIM }}> · </span>
          <span style={{ color: T_ACCENT }}>openai_codex</span>
          <span style={{ color: T_DIM }}> · </span>
          <span style={{ color: T_MUTED }}>think:medium</span>
        </span>
      </div>

      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          marginTop: 22,
          fontSize: 24,
        }}
      >
        <span>
          <span style={{ color: T_FG }}>~/Documents/dev/xero</span>
          <span style={{ color: T_ACCENT }}>:main</span>
        </span>
        <span style={{ color: T_DIM }}>tab agents ctrl+p /commands 0.1.0</span>
      </div>
    </div>
  </div>
);

export const AppFlow: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps, width, height } = useVideoConfig();

  // Fill the full frame width (the app is narrower than 16:9, so this crops a
  // few px off the top/bottom rather than leaving black side bars).
  const baseW = width;
  const baseH = width / SCREEN_RATIO;

  const enter = interpolate(frame, [0, 10], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  // "Head turn" exit: the whole scene (combo + text) swings left off-screen
  // while pivoting on Y, as if turning your head to the right.
  const turn = interpolate(frame, [HEAD_TURN_START, HEAD_TURN_END], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: CAM_EASE,
  });
  const turnTx = turn * -2300;
  const turnRy = turn * 58;

  // The solana view is locked to the head turn (mirror of the exiting scene)
  // so they move as one, then ~halfway through it starts a camera push into
  // the Solana Workbench sidebar (top-right). It begins zoomed out.
  const solZoom = interpolate(frame, [607, 641], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: CAM_EASE,
  });
  const solPullback = interpolate(frame, [702, SOL_CLICK2], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.inOut(Easing.cubic),
  });
  const solFinalPullback = interpolate(
    frame,
    [SOL_FINAL_PULLBACK_START, SOL_FINAL_PULLBACK_END],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: CAM_EASE,
    },
  );
  const activeSolS = 0.6 + solZoom * 1.55 - solPullback * 0.5;
  const activeSolFx = 0.5 + solZoom * 0.258;
  // Downward pan: brisk to the Personas tab, a brief hold at that pace, then a
  // clear acceleration down to the Wallet button. Piecewise-linear so velocity
  // only ever increases (no stall) until it settles at the end.
  const solPanDown = interpolate(
    frame,
    [656, 686, 702, 734, 746],
    [0, 0.072, 0.103, 0.345, 0.37],
    { extrapolateLeft: "clamp", extrapolateRight: "clamp" },
  );
  const activeSolFy = 0.5 - solZoom * 0.32 + solPanDown; // center -> top, then pan down
  const finalZoom = interpolate(
    frame,
    [FINAL_ZOOM_START, FINAL_ZOOM_END],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: CAM_EASE,
    },
  );
  const settledSolS = interpolate(solFinalPullback, [0, 1], [activeSolS, 0.88]);
  const settledSolFx = interpolate(
    solFinalPullback,
    [0, 1],
    [activeSolFx, 0.5],
  );
  const settledSolFy = interpolate(
    solFinalPullback,
    [0, 1],
    [activeSolFy, 0.04],
  );
  const solS = interpolate(finalZoom, [0, 1], [settledSolS, 1.72]);
  const solFx = interpolate(finalZoom, [0, 1], [settledSolFx, 0.5]);
  const solFy = interpolate(finalZoom, [0, 1], [settledSolFy, -0.32]);
  const solTx = width / 2 - solFx * width * solS;
  const solTy = height / 2 - solFy * height * solS;

  // The solana view renders objectFit:contain in a 1920x1080 box, so the image
  // is letterboxed. Map image fractions -> on-screen pixels. By the time we
  // interact, the head-turn wrapper is identity, so the only transform is
  // translate(solTx,solTy) scale(solS).
  const solImgScale = Math.min(width / 3032, height / 1812);
  const solImgW = 3032 * solImgScale;
  const solImgH = 1812 * solImgScale;
  const solImgOffX = (width - solImgW) / 2;
  const solImgOffY = (height - solImgH) / 2;
  const solTabPos = (fx: number, fy: number) => ({
    x: solTx + (solImgOffX + fx * solImgW) * solS,
    y: solTy + (solImgOffY + fy * solImgH) * solS,
  });
  const openIcon = solTabPos(SOL_OPEN_ICON.fx, SOL_OPEN_ICON.fy);
  const tab2 = solTabPos(SOL_TAB2.fx, SOL_TAB2.fy);
  const tab3 = solTabPos(SOL_TAB3.fx, SOL_TAB3.fy);
  // Cursor: one continuous pointer opens the Workbench, continues through the
  // sidebar tab sequence, then glitches away only after the final click. All
  // targets track the live camera pan.
  const solCurIn = interpolate(
    frame,
    [SOL_CURSOR_START, SOL_CURSOR_START + 4],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    },
  );
  const approachOpen = interpolate(
    frame,
    [SOL_CURSOR_START, SOL_OPEN_CLICK],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.inOut(Easing.cubic),
    },
  );
  const toTab2 = interpolate(frame, [SOL_OPEN_CLICK + 14, SOL_CLICK], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.inOut(Easing.cubic),
  });
  const toTab3 = interpolate(frame, [SOL_CLICK + 6, SOL_CLICK2], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.inOut(Easing.cubic),
  });
  const solCurX =
    openIcon.x +
    (tab2.x - openIcon.x) * toTab2 +
    (tab3.x - tab2.x) * toTab3 +
    (1 - approachOpen) * 240;
  const solCurY =
    openIcon.y +
    (tab2.y - openIcon.y) * toTab2 +
    (tab3.y - tab2.y) * toTab3 +
    (1 - approachOpen) * 200;
  const solPress =
    1 -
    0.16 *
      Math.max(
        pressDip(frame, SOL_OPEN_CLICK),
        pressDip(frame, SOL_CLICK),
        pressDip(frame, SOL_CLICK2),
      );
  const solCursorExit = cursorExitAt(frame, [SOL_CLICK2]);
  const solCursorVisible =
    frame >= SOL_CURSOR_START &&
    frame <= SOL_CLICK2 + CURSOR_GLITCH_DELAY + CURSOR_GLITCH_FRAMES;

  // Camera: zoom into "Create agent", pan to the modal, then zoom out to reveal
  // the whole canvas.
  // Ends by zooming out past 1.0 and panning right (small focal x) so the agent
  // chat panel slides off the right edge, clearing the left for the feature list.
  // Opens further out (whole app visible) with a brief hold, then zooms in.
  const camS = kf(
    frame,
    [
      0, 12, 30, 44, 58, 74, 108, 150, 176, 208, 236, 274, 298, 400, 438, 504,
      526,
    ],
    [
      0.78, 0.78, 1.55, 1.55, 1.45, 1.45, 0.85, 0.85, 2.6, 2.6, 1.9, 1.9, 1.62,
      1.62, 0.85, 0.85, 0.85,
    ],
  );
  const camFx = kf(
    frame,
    [
      0, 12, 30, 44, 58, 74, 108, 150, 176, 208, 236, 274, 298, 400, 438, 504,
      526,
    ],
    [
      0.5, 0.5, 0.515, 0.515, 0.5, 0.5, 0.034, 0.034, 0.157, 0.157, 0.518,
      0.518, 0.55, 0.55, 0.034, 0.034, 1.17,
    ],
  );
  const camFy = kf(
    frame,
    [
      0, 12, 30, 44, 58, 74, 108, 150, 176, 208, 236, 274, 298, 400, 438, 504,
      526,
    ],
    [
      0.5, 0.5, 0.557, 0.557, 0.5, 0.5, 0.5, 0.5, 0.09, 0.09, 0.715, 0.715, 0.4,
      0.4, 0.5, 0.5, 0.525,
    ],
  );
  const camTx = width / 2 - camFx * baseW * camS;
  const camTy = height / 2 - camFy * baseH * camS;

  // The cloud panel rides the camera's left pan (so it exits "attached" to the
  // combo). camTx at the pre-pan floating hold is the rest position.
  const restCamTx = width / 2 - 0.034 * baseW * 0.85;
  const cloudExitX = frame >= CLOUD_OUT ? camTx - restCamTx : 0;

  // Cursor path: start → Create agent (hold) → New agent.
  const ease = { easing: Easing.inOut(Easing.cubic) } as const;
  const seg = (a: number, b: number, from: number, to: number) =>
    interpolate(frame, [a, b], [from, to], {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      ...ease,
    });
  const cx =
    frame < 46
      ? seg(INITIAL_CURSOR_START, 30, START.x, CREATE_AGENT.x)
      : frame < T3_START
        ? seg(48, 62, CREATE_AGENT.x, NEW_AGENT.x)
        : seg(AGENT_CURSOR_START, CLICK3, AGENT_FROM.x, AGENT_TAB.x);
  const cy =
    frame < 46
      ? seg(INITIAL_CURSOR_START, 30, START.y, CREATE_AGENT.y)
      : frame < T3_START
        ? seg(48, 62, CREATE_AGENT.y, NEW_AGENT.y)
        : seg(AGENT_CURSOR_START, CLICK3, AGENT_FROM.y, AGENT_TAB.y);
  const press =
    1 -
    0.16 *
      Math.max(
        pressDip(frame, CLICK1),
        pressDip(frame, CLICK2),
        pressDip(frame, CLICK3),
      );
  const cursorExit = cursorExitAt(frame, [
    CLICK2,
    { at: CLICK3, delay: AGENT_CURSOR_GLITCH_DELAY },
  ]);
  const cursorEnter = cursorEnterAt(
    frame,
    frame < T3_START ? INITIAL_CURSOR_START : AGENT_CURSOR_VISIBLE_START,
  );
  const cursorVisible =
    (frame >= INITIAL_CURSOR_START &&
      frame < CLICK2 + CURSOR_GLITCH_DELAY + CURSOR_GLITCH_FRAMES) ||
    (frame >= AGENT_CURSOR_VISIBLE_START &&
      frame < CLICK3 + AGENT_CURSOR_GLITCH_DELAY + CURSOR_GLITCH_FRAMES);

  // Modal: springs in on click 1, scales out on click 2.
  const modalIn = spring({
    frame: frame - CLICK1 - 1,
    fps,
    config: { mass: 0.5, damping: 13, stiffness: 220 },
  });
  const modalExit = interpolate(frame, [CLICK2, CLICK2 + 10], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.in(Easing.cubic),
  });
  const modalOpacity =
    interpolate(frame, [CLICK1, CLICK1 + 8], [0, 1], {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    }) *
    (1 - modalExit);
  const modalScale =
    interpolate(modalIn, [0, 1], [0.9, 1]) *
    interpolate(modalExit, [0, 1], [1, 1.05]);
  const dim =
    interpolate(frame, [CLICK1, CLICK1 + 8], [0, 0.5], {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    }) *
    (1 - modalExit);

  // Canvas dissolves in after click 2.
  const canvasIn = interpolate(frame, [CANVAS_IN, CANVAS_IN + 12], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.out(Easing.cubic),
  });

  // The agent chat screen swaps in at the Agent-tab click (match cut on tabs).
  const agentChatIn = interpolate(frame, [CLICK3, CLICK3 + 3], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  // The app eases into a hovering 3D perspective once it settles, then flattens
  // back out for the scene-3 hand-off.
  // Flat through the composer/typing, then a gentle 3D angle while the
  // conversation streams (298-414), growing into the full card tilt at the
  // zoom-out.
  const tiltP = kf(
    frame,
    [CANVAS_IN, 112, T3_START, T3_START + 20, 298, 318, 414, 438],
    [0, 1, 1, 0, 0, 0.78, 0.78, 1],
  );
  // As the combo slides to the left at the close, flip the yaw so the cards
  // face the now-empty right side (with a slightly shallower angle there).
  const tiltYaw = interpolate(frame, [504, 526], [-9, 7], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  // While the conversation is up, the plane gently rocks so its 3D angle is in
  // motion — depth reads far better moving than static at this zoom.
  const convPhase = interpolate(frame, [298, 316, 400, 414], [0, 1, 1, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const tiltRy =
    tiltP * (tiltYaw + 0.8 * Math.sin(frame * 0.045)) +
    convPhase * 4 * Math.sin(frame * 0.028);
  const tiltRx =
    tiltP * (3 + 0.5 * Math.sin(frame * 0.038 + 1.2)) +
    convPhase * 1.8 * Math.sin(frame * 0.02 + 1.3);
  const tiltFloat = tiltP * 7 * Math.sin(frame * 0.04 + 0.7);
  const tiltTransform = `perspective(1700px) translateY(${tiltFloat}px) rotateX(${tiltRx}deg) rotateY(${tiltRy}deg)`;
  const tiltShadow = tiltP * 0.5;

  const mw = MODAL.right - MODAL.left;
  const mh = MODAL.bottom - MODAL.top;
  const fill: React.CSSProperties = {
    position: "absolute",
    inset: 0,
    width: "100%",
    height: "100%",
    objectFit: "cover",
  };

  return (
    <AbsoluteFill style={{ backgroundColor: "#070707" }}>
      <SceneBackground />
      <AbsoluteFill style={{ perspective: 1700 }}>
        <AbsoluteFill
          style={{
            transform: `translateX(${turnTx}px) rotateY(${turnRy}deg)`,
            transformOrigin: "center center",
          }}
        >
          <div
            style={{
              position: "absolute",
              inset: 0,
              overflow: "hidden",
              opacity: enter,
            }}
          >
            <div
              style={{
                position: "absolute",
                width: baseW,
                height: baseH,
                transformOrigin: "0 0",
                transform: `translate(${camTx}px, ${camTy}px) scale(${camS})`,
              }}
            >
              <div
                style={{
                  position: "absolute",
                  width: baseW,
                  height: baseH,
                  transformOrigin: "center center",
                  transform: tiltTransform,
                  filter: `drop-shadow(0 44px 80px rgba(0,0,0,${tiltShadow}))`,
                }}
              >
                <Img src={staticFile("app/main.png")} style={fill} />

                <div
                  style={{
                    position: "absolute",
                    inset: 0,
                    backgroundColor: "#000000",
                    opacity: dim,
                  }}
                />

                {modalOpacity > 0.001 && (
                  <div
                    style={{
                      position: "absolute",
                      left: `${MODAL.left}%`,
                      top: `${MODAL.top}%`,
                      width: `${mw}%`,
                      height: `${mh}%`,
                      overflow: "hidden",
                      borderRadius: 14,
                      opacity: modalOpacity,
                      transform: `scale(${modalScale})`,
                      transformOrigin: "center",
                      boxShadow: "0 30px 80px rgba(0,0,0,0.6)",
                    }}
                  >
                    <Img
                      src={staticFile("app/modal.png")}
                      style={{
                        position: "absolute",
                        width: `${10000 / mw}%`,
                        height: `${10000 / mh}%`,
                        left: `${(-MODAL.left / mw) * 100}%`,
                        top: `${(-MODAL.top / mh) * 100}%`,
                        maxWidth: "none",
                      }}
                    />
                  </div>
                )}

                {canvasIn > 0.001 && (
                  <Img
                    src={staticFile("app/canvas.png")}
                    style={{ ...fill, opacity: canvasIn }}
                  />
                )}

                {agentChatIn > 0.001 && (
                  <Img
                    src={staticFile("agent_chat.png")}
                    style={{ ...fill, opacity: agentChatIn }}
                  />
                )}

                <ComposerType />

                {frame >= REVEAL_START && (
                  <div
                    style={{
                      position: "absolute",
                      left: "5%",
                      top: "10.5%",
                      width: "90%",
                      height: "69%",
                      overflow: "hidden",
                    }}
                  >
                    <div
                      style={{
                        position: "absolute",
                        inset: 0,
                        backgroundColor: "#121212",
                      }}
                    />
                    <Conversation />
                  </div>
                )}

                <ClickRipple
                  x={CREATE_AGENT.x}
                  y={CREATE_AGENT.y}
                  at={CLICK1}
                  cam={camS}
                />
                <ClickRipple
                  x={NEW_AGENT.x}
                  y={NEW_AGENT.y}
                  at={CLICK2}
                  cam={camS}
                />
                <ClickRipple
                  x={AGENT_TAB.x}
                  y={AGENT_TAB.y}
                  at={CLICK3}
                  cam={camS}
                />

                {cursorVisible && (
                  <Cursor
                    x={cx}
                    y={cy}
                    press={press}
                    cam={camS}
                    glitch={Math.max(cursorEnter.glitch, cursorExit.glitch)}
                    opacity={cursorEnter.opacity * cursorExit.opacity}
                    seed={frame}
                  />
                )}

                {/* Cloud iPhone, on the app's own tilted plane — revealed (not slid
                in) by the zoom-out, so it moves attached to the app. */}
                {frame >= PHONE_SHOW && (
                  <div
                    style={{
                      position: "absolute",
                      left: PHONE_CX,
                      top: PHONE_CY,
                      transform: `scale(${PH_CONTENT_SCALE})`,
                      transformOrigin: "top left",
                    }}
                  >
                    <CloudPhone />
                  </div>
                )}

                {/* macOS terminal (Xero TUI), to the right of the app on the same
                plane — revealed as the combo pans left at the close. */}
                {frame >= TERM_SHOW && (
                  <div
                    style={{
                      position: "absolute",
                      left: TERM_CX,
                      top: TERM_CY,
                      transform: `scale(${TERM_CONTENT_SCALE})`,
                      transformOrigin: "top left",
                    }}
                  >
                    <TerminalWindow />
                  </div>
                )}
              </div>
            </div>
          </div>

          <Caption />
          <FeaturePanel />
          <ChatCaption />
          <CloudPanel exitX={cloudExitX} />
          <TuiPanel />
        </AbsoluteFill>
      </AbsoluteFill>

      {/* As the head turn swings the scene off-left, the next view (solana
          benchmark) pans in from the right and settles face-on — same motion. */}
      {turn > 0.001 && (
        <AbsoluteFill style={{ perspective: 1700 }}>
          <AbsoluteFill
            style={{
              transform: `translateX(${(1 - turn) * 2300}px) rotateY(${-(1 - turn) * 58}deg)`,
              transformOrigin: "center center",
            }}
          >
            <div
              style={{
                position: "absolute",
                inset: 0,
                transformOrigin: "0 0",
                transform: `translate(${solTx}px, ${solTy}px) scale(${solS})`,
              }}
            >
              <Closeout />
              <Img
                src={staticFile("agent_chat.png")}
                style={{
                  position: "absolute",
                  inset: 0,
                  width: "100%",
                  height: "100%",
                  objectFit: "contain",
                  opacity: frame < SOL_WORKBENCH_SHOW ? 1 : 0,
                }}
              />
              <Img
                src={staticFile("solana_bench_1.png")}
                style={{
                  position: "absolute",
                  inset: 0,
                  width: "100%",
                  height: "100%",
                  objectFit: "contain",
                  opacity:
                    frame >= SOL_WORKBENCH_SHOW && frame < SOL_CLICK ? 1 : 0,
                }}
              />
              <Img
                src={staticFile("solana_bench_2.png")}
                style={{
                  position: "absolute",
                  inset: 0,
                  width: "100%",
                  height: "100%",
                  objectFit: "contain",
                  opacity: frame >= SOL_CLICK && frame < SOL_CLICK2 ? 1 : 0,
                }}
              />
              <Img
                src={staticFile("solana_bench_3.png")}
                style={{
                  position: "absolute",
                  inset: 0,
                  width: "100%",
                  height: "100%",
                  objectFit: "contain",
                  opacity: frame >= SOL_CLICK2 ? 1 : 0,
                }}
              />
            </div>
          </AbsoluteFill>
        </AbsoluteFill>
      )}

      {/* Cursor opens Solana Workbench, continues through the sidebar, then exits. */}
      {solCursorVisible && (
        <>
          <div
            style={{
              position: "absolute",
              left: solCurX - CURSOR_TIP_X,
              top: solCurY - CURSOR_TIP_Y,
              transform: `scale(${solPress})`,
              transformOrigin: "top left",
              opacity: solCurIn * solCursorExit.opacity,
              filter: "drop-shadow(0 2px 3px rgba(0,0,0,0.55))",
            }}
          >
            <CursorGlyph glitch={solCursorExit.glitch} seed={frame} />
          </div>
          <ClickRipple
            x={(openIcon.x / width) * 100}
            y={(openIcon.y / height) * 100}
            at={SOL_OPEN_CLICK}
            cam={1}
          />
          <ClickRipple
            x={(tab2.x / width) * 100}
            y={(tab2.y / height) * 100}
            at={SOL_CLICK}
            cam={1}
          />
          <ClickRipple
            x={(tab3.x / width) * 100}
            y={(tab3.y / height) * 100}
            at={SOL_CLICK2}
            cam={1}
          />
        </>
      )}

      {frame >= SOL_CAPTION_START && <SolanaCaption />}

      {[CLICK1, CLICK2, CLICK3, SOL_OPEN_CLICK, SOL_CLICK, SOL_CLICK2].map(
        (at) => (
          <Sequence key={at} from={at} durationInFrames={12} layout="none">
            <Audio src={staticFile("click.mp3")} trimBefore={1} volume={0.4} />
          </Sequence>
        ),
      )}

      {/* keyboard sound while the prompt is typed in the composer */}
      <Sequence
        from={TYPE_START}
        durationInFrames={TYPING_FRAMES}
        layout="none"
      >
        <Audio src={staticFile("keyboard.mp3")} volume={1} />
      </Sequence>

      {/* single Enter/Return press as the prompt sends and the empty state is covered */}
      <Sequence from={REVEAL_START - 4} durationInFrames={5} layout="none">
        <Audio src={staticFile("keyboard.mp3")} volume={1} />
      </Sequence>

      <Sequence from={FINAL_SHOVE_START} durationInFrames={27} layout="none">
        <Audio src={staticFile("pop.mp3")} trimBefore={3} volume={0.2} />
      </Sequence>
      <Sequence
        from={FINAL_DOMAIN_REVEAL_START}
        durationInFrames={13}
        layout="none"
      >
        <Audio src={staticFile("glitch2.mp3")} trimBefore={22} volume={0.1} />
      </Sequence>
      <Sequence
        from={FINAL_DOMAIN_GLITCH_OUT_START}
        durationInFrames={13}
        layout="none"
      >
        <Audio src={staticFile("glitch2.mp3")} trimBefore={22} volume={0.1} />
      </Sequence>
    </AbsoluteFill>
  );
};
