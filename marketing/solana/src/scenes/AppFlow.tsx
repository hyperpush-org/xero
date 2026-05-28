import { Audio } from "@remotion/media";
import { loadFont } from "@remotion/google-fonts/Inter";
import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono";
import type { CSSProperties, ReactNode } from "react";
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
import { SceneBackground } from "../SceneBackground";

const { fontFamily } = loadFont("normal", { weights: ["400", "600", "700"] });
const { fontFamily: monoFamily } = loadMono("normal", {
  weights: ["400", "700"],
});

export const APPFLOW_FRAMES = 780;

const IMAGE_W = 3032;
const IMAGE_H = 1814;
const IMAGE_RATIO = IMAGE_W / IMAGE_H;
const ACCENT = "#D4A574";
const DARK_ACCENT = "#4E4337";
const MUTED = "rgba(222,222,222,0.66)";
const OPEN_CLICK = 58;
const WORKBENCH_IN = 72;
const SCAN_START = 546;
const CLOSEOUT_START = 636;
const FINAL_CLEAR_START = APPFLOW_FRAMES - 42;

const XERO_MARK_QUADRANTS = [
  {
    d: "M182.98 182.984L0.000640869 182.984L0.000629244 50.0041C0.00062683 22.3898 22.3864 0.00413391 50.0006 0.0041315L182.98 0.00411987L182.98 182.984Z",
    fill: ACCENT,
  },
  {
    d: "M237.02 0L370 0C397.614 0 420 22.3858 420 50V182.98H237.02V0Z",
    fill: DARK_ACCENT,
  },
  {
    d: "M237.02 237.023H419.999V370.004C419.999 397.618 397.614 420.004 369.999 420.004H237.02V237.023Z",
    fill: ACCENT,
  },
  {
    d: "M0 237.023H182.98V420.004H50C22.3857 420.004 0 397.618 0 370.004L0 237.023Z",
    fill: DARK_ACCENT,
  },
] as const;

type Focus = {
  fx: number;
  fy: number;
  scale: number;
};

type Beat = {
  id: string;
  asset: string;
  start: number;
  end: number;
  focus: Focus;
  tabPoint: { fx: number; fy: number };
  kicker: string;
  title: string;
  body: string;
  chips: string[];
};

const INTRO_FOCUS: Focus = { fx: 0.5, fy: 0.5, scale: 0.92 };

const BEATS: Beat[] = [
  {
    id: "cluster",
    asset: "solana_bench_1.png",
    start: WORKBENCH_IN,
    end: 156,
    focus: { fx: 0.86, fy: 0.39, scale: 1.84 },
    tabPoint: { fx: 0.739, fy: 0.137 },
    kicker: "Cluster",
    title: "Localnet on demand",
    body: "Start a validator, switch to a fork, and keep RPC routing beside the agent.",
    chips: ["localnet", "forked mainnet", "RPC ready"],
  },
  {
    id: "personas",
    asset: "solana_bench_2.png",
    start: 138,
    end: 232,
    focus: { fx: 0.86, fy: 0.37, scale: 1.86 },
    tabPoint: { fx: 0.739, fy: 0.188 },
    kicker: "Personas",
    title: "Fund the actors",
    body: "Seed named wallets for realistic flows without leaving the project window.",
    chips: ["whales", "users", "treasury"],
  },
  {
    id: "scenarios",
    asset: "scenarios.png",
    start: 214,
    end: 312,
    focus: { fx: 0.855, fy: 0.35, scale: 1.9 },
    tabPoint: { fx: 0.739, fy: 0.238 },
    kicker: "Scenarios",
    title: "Run the loop before users do",
    body: "Reusable Solana runbooks live next to the agent canvas, ready to replay.",
    chips: ["mint + list", "transfer hook", "fixtures"],
  },
  {
    id: "tx",
    asset: "tx.png",
    start: 292,
    end: 390,
    focus: { fx: 0.858, fy: 0.37, scale: 1.9 },
    tabPoint: { fx: 0.739, fy: 0.286 },
    kicker: "Transactions",
    title: "Simulate, decode, price",
    body: "Inspect base64 txs, explain signatures, and estimate priority fees in one rail.",
    chips: ["simulate", "explain", "priority fee"],
  },
  {
    id: "deploy",
    asset: "deploy.png",
    start: 370,
    end: 470,
    focus: { fx: 0.858, fy: 0.48, scale: 1.86 },
    tabPoint: { fx: 0.739, fy: 0.466 },
    kicker: "Deploy",
    title: "Ship with guardrails",
    body: "Builds, upgrade checks, authority modes, and rollback sit in the same workflow.",
    chips: ["build", "upgrade safety", "Squads vault"],
  },
  {
    id: "audit",
    asset: "audit.png",
    start: 448,
    end: 560,
    focus: { fx: 0.858, fy: 0.35, scale: 1.9 },
    tabPoint: { fx: 0.739, fy: 0.514 },
    kicker: "Audit",
    title: "Checks before launch",
    body: "Linting, analyzers, fuzzing, coverage, and exploit replay stay close to the code.",
    chips: ["lints", "fuzz", "replay"],
  },
];

const TOOL_CHIPS = [
  "solana_cluster_start",
  "solana_persona_create",
  "solana_scenario_run",
  "solana_tx_simulate",
  "solana_tx_explain",
  "solana_priority_fee",
  "solana_program_build",
  "solana_deploy_guard",
  "solana_audit_static",
  "solana_token_mint",
  "solana_indexer_query",
  "solana_rpc_health",
] as const;

const ease = (
  frame: number,
  start: number,
  end: number,
  easing: (input: number) => number = Easing.out(Easing.cubic),
) =>
  interpolate(frame, [start, end], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing,
  });

const layerOpacity = (frame: number, start: number, end: number, fade = 18) =>
  ease(frame, start, start + fade) * (1 - ease(frame, end - fade, end));

const containRect = (width: number, height: number) => {
  const frameRatio = width / height;
  if (frameRatio > IMAGE_RATIO) {
    const displayHeight = height;
    const displayWidth = height * IMAGE_RATIO;
    return {
      left: (width - displayWidth) / 2,
      top: 0,
      width: displayWidth,
      height: displayHeight,
    };
  }
  const displayWidth = width;
  const displayHeight = width / IMAGE_RATIO;
  return {
    left: 0,
    top: (height - displayHeight) / 2,
    width: displayWidth,
    height: displayHeight,
  };
};

const cameraFor = ({
  frame,
  start,
  end,
  focus,
  width,
  height,
}: {
  frame: number;
  start: number;
  end: number;
  focus: Focus;
  width: number;
  height: number;
}) => {
  const rect = containRect(width, height);
  const drift = interpolate(frame, [start, end], [0, 0.055], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.inOut(Easing.cubic),
  });
  const scale = focus.scale * (1 + drift);
  const sourceX = rect.left + rect.width * focus.fx;
  const sourceY = rect.top + rect.height * focus.fy;
  return {
    rect,
    scale,
    x: width / 2 - sourceX * scale,
    y: height / 2 - sourceY * scale,
  };
};

const pointFor = ({
  point,
  focus,
  frame,
  start,
  end,
  width,
  height,
}: {
  point: { fx: number; fy: number };
  focus: Focus;
  frame: number;
  start: number;
  end: number;
  width: number;
  height: number;
}) => {
  const camera = cameraFor({ frame, start, end, focus, width, height });
  return {
    x:
      camera.x +
      (camera.rect.left + camera.rect.width * point.fx) * camera.scale,
    y:
      camera.y +
      (camera.rect.top + camera.rect.height * point.fy) * camera.scale,
  };
};

const XeroMark = ({
  size,
  opacity = 1,
}: {
  size: number;
  opacity?: number;
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 420 420"
    fill="none"
    style={{ opacity, overflow: "visible" }}
  >
    {XERO_MARK_QUADRANTS.map((quadrant) => (
      <path key={quadrant.d} d={quadrant.d} fill={quadrant.fill} />
    ))}
  </svg>
);

const SolanaBars = ({
  size = 76,
  opacity = 1,
}: {
  size?: number;
  opacity?: number;
}) => {
  const bar = (y: number, color: string, reverse = false) => (
    <path
      d={
        reverse
          ? `M${size * 0.18} ${y + size * 0.12}L${size * 0.78} ${y + size * 0.12}L${size * 0.88} ${y}L${size * 0.28} ${y}Z`
          : `M${size * 0.12} ${y + size * 0.12}L${size * 0.72} ${y + size * 0.12}L${size * 0.82} ${y}L${size * 0.22} ${y}Z`
      }
      fill={color}
    />
  );
  return (
    <svg
      width={size}
      height={size * 0.74}
      viewBox={`0 0 ${size} ${size * 0.74}`}
      style={{ opacity }}
    >
      {bar(size * 0.06, "#e5b174")}
      {bar(size * 0.27, "#9b8cff", true)}
      {bar(size * 0.48, "#77e3b4")}
    </svg>
  );
};

const GlitchText = ({
  children,
  fontSize,
  intensity,
  seed,
  color = "#ffffff",
  fontWeight = 700,
  style,
}: {
  children: ReactNode;
  fontSize: number;
  intensity: number;
  seed: number;
  color?: string;
  fontWeight?: number;
  style?: CSSProperties;
}) => {
  const base: CSSProperties = {
    fontFamily,
    fontWeight,
    fontSize,
    lineHeight: 1,
    whiteSpace: "nowrap",
    letterSpacing: 0,
    ...style,
  };

  if (intensity <= 0.001) {
    return <span style={{ ...base, color }}>{children}</span>;
  }

  const rnd = (key: string) => random(`${seed}-${key}`);
  const signed = (key: string) => rnd(key) * 2 - 1;
  const shift = (0.04 + 0.045 * rnd("rgb")) * fontSize * intensity;
  const jitter = signed("base") * 0.035 * fontSize * intensity;
  const flicker = 1 - rnd("flicker") * 0.24 * intensity;
  const layer = (layerColor: string, dx: number, dy = 0): CSSProperties => ({
    ...base,
    position: "absolute",
    inset: 0,
    color: layerColor,
    mixBlendMode: "screen",
    transform: `translate(${dx}px, ${dy}px)`,
  });
  const slices = [0, 1, 2, 3]
    .filter((i) => rnd(`slice-on-${i}`) < 0.72)
    .map((i) => {
      const top = Math.round(rnd(`slice-top-${i}`) * 78);
      const h = 5 + Math.round(rnd(`slice-height-${i}`) * 16);
      return {
        key: i,
        top,
        bottom: Math.max(0, 100 - top - h),
        dx: signed(`slice-dx-${i}`) * 0.18 * fontSize * intensity,
      };
    });

  return (
    <span
      style={{
        ...base,
        position: "relative",
        display: "inline-block",
        isolation: "isolate",
        opacity: flicker,
        transform: `translateX(${jitter}px)`,
      }}
    >
      <span style={{ ...base, color, display: "block", position: "relative" }}>
        {children}
      </span>
      <span style={layer("#ff4b4b", shift, -shift * 0.12)}>{children}</span>
      <span style={layer("#46a6ff", -shift, shift * 0.1)}>{children}</span>
      {slices.map((slice) => (
        <span
          key={slice.key}
          style={{
            ...layer("#ffffff", slice.dx),
            clipPath: `inset(${slice.top}% 0 ${slice.bottom}% 0)`,
          }}
        >
          {children}
        </span>
      ))}
    </span>
  );
};

const ScreenshotLayer = ({
  src,
  start,
  end,
  focus,
  opacityMultiplier = 1,
}: {
  src: string;
  start: number;
  end: number;
  focus: Focus;
  opacityMultiplier?: number;
}) => {
  const frame = useCurrentFrame();
  const { width, height } = useVideoConfig();
  const opacity = layerOpacity(frame, start, end, 22) * opacityMultiplier;
  if (opacity <= 0.001) {
    return null;
  }
  const camera = cameraFor({ frame, start, end, focus, width, height });
  const skewIn = interpolate(frame, [start, start + 24], [4, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.out(Easing.cubic),
  });

  return (
    <AbsoluteFill style={{ opacity, perspective: 1800 }}>
      <div
        style={{
          position: "absolute",
          inset: 0,
          transformOrigin: "0 0",
          transform: `translate3d(${camera.x}px, ${camera.y}px, 0) scale(${camera.scale}) rotateY(${skewIn}deg)`,
        }}
      >
        <Img
          src={staticFile(src)}
          style={{
            position: "absolute",
            left: camera.rect.left,
            top: camera.rect.top,
            width: camera.rect.width,
            height: camera.rect.height,
            objectFit: "fill",
            filter: "drop-shadow(0 42px 90px rgba(0,0,0,0.55))",
          }}
        />
      </div>
    </AbsoluteFill>
  );
};

const CursorGlyph = ({ glitch = 0 }: { glitch?: number }) => (
  <svg width="48" height="58" viewBox="0 0 48 58" fill="none">
    <path
      d="M6 5L39 32L24.2 35.2L17.2 52L6 5Z"
      fill="#f6f2ec"
      stroke="#101010"
      strokeWidth="3"
      style={{
        transform: `translate(${glitch * 3}px, ${glitch * -2}px)`,
      }}
    />
    {glitch > 0.05 ? (
      <path
        d="M6 5L39 32L24.2 35.2L17.2 52L6 5Z"
        stroke="#d4a574"
        strokeWidth="2"
        opacity={glitch}
      />
    ) : null}
  </svg>
);

const IntroCursor = () => {
  const frame = useCurrentFrame();
  const { width, height } = useVideoConfig();
  const target = pointFor({
    point: { fx: 0.946, fy: 0.045 },
    focus: INTRO_FOCUS,
    frame,
    start: 0,
    end: WORKBENCH_IN + 12,
    width,
    height,
  });
  const t = ease(frame, 18, OPEN_CLICK - 4);
  const x = interpolate(t, [0, 1], [width * 0.56, target.x]);
  const y = interpolate(t, [0, 1], [height * 0.64, target.y]);
  const fade =
    ease(frame, 12, 24) * (1 - ease(frame, OPEN_CLICK + 10, OPEN_CLICK + 28));
  const press = interpolate(
    frame,
    [OPEN_CLICK - 1, OPEN_CLICK + 2, OPEN_CLICK + 7],
    [1, 0.86, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    },
  );
  return (
    <div
      style={{
        position: "absolute",
        left: x - 6,
        top: y - 5,
        opacity: fade,
        transform: `scale(${press})`,
        filter: "drop-shadow(0 3px 4px rgba(0,0,0,0.5))",
      }}
    >
      <CursorGlyph />
    </div>
  );
};

const ClickRipple = ({ x, y, at }: { x: number; y: number; at: number }) => {
  const frame = useCurrentFrame();
  const t = ease(frame, at, at + 18, Easing.out(Easing.cubic));
  const fade =
    ease(frame, at - 3, at + 2) * (1 - ease(frame, at + 12, at + 20));
  if (fade <= 0.001) {
    return null;
  }
  return (
    <div
      style={{
        position: "absolute",
        left: x,
        top: y,
        width: 20 + t * 58,
        height: 20 + t * 58,
        borderRadius: "50%",
        border: "2px solid rgba(212,165,116,0.95)",
        transform: "translate(-50%, -50%)",
        opacity: fade,
        boxShadow: "0 0 28px rgba(212,165,116,0.25)",
      }}
    />
  );
};

const BeatHotspot = ({ beat }: { beat: Beat }) => {
  const frame = useCurrentFrame();
  const { width, height } = useVideoConfig();
  const point = pointFor({
    point: beat.tabPoint,
    focus: beat.focus,
    frame,
    start: beat.start,
    end: beat.end,
    width,
    height,
  });
  return <ClickRipple x={point.x} y={point.y} at={beat.start + 16} />;
};

const IntroCopy = () => {
  const frame = useCurrentFrame();
  const intro = ease(frame, 8, 34);
  const exit = ease(frame, WORKBENCH_IN - 12, WORKBENCH_IN + 12);
  const glitch = interpolate(frame, [12, 18, 38], [0.9, 0.2, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  return (
    <div
      style={{
        position: "absolute",
        left: 86,
        top: 184,
        width: 780,
        opacity: intro * (1 - exit),
        transform: `translateY(${(1 - intro) * 34 - exit * 16}px)`,
        fontFamily,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 18,
          marginBottom: 32,
        }}
      >
        <SolanaBars size={86} opacity={0.95} />
        <span
          style={{
            color: ACCENT,
            fontFamily: monoFamily,
            fontSize: 21,
            fontWeight: 700,
            letterSpacing: 0,
          }}
        >
          SOLANA WORKBENCH
        </span>
      </div>
      <GlitchText fontSize={78} intensity={glitch} seed={frame}>
        One native command center.
      </GlitchText>
      <div
        style={{
          marginTop: 24,
          width: 650,
          color: MUTED,
          fontSize: 28,
          lineHeight: 1.34,
          fontWeight: 400,
        }}
      >
        Localnet, wallets, transactions, deploy checks, and agent tools in the
        same desktop surface.
      </div>
    </div>
  );
};

const BeatCopy = () => {
  const frame = useCurrentFrame();
  const beat =
    BEATS.find(
      (candidate) => frame >= candidate.start && frame < candidate.end,
    ) ?? BEATS[BEATS.length - 1];
  const inProgress = ease(frame, beat.start + 4, beat.start + 24);
  const outProgress = ease(frame, beat.end - 22, beat.end - 4);
  const scanOut = ease(frame, SCAN_START - 12, SCAN_START + 12);
  const opacity = inProgress * (1 - outProgress) * (1 - scanOut);
  if (opacity <= 0.001) {
    return null;
  }

  return (
    <div
      style={{
        position: "absolute",
        left: 84,
        bottom: 94,
        width: 760,
        display: "flex",
        gap: 22,
        opacity,
        transform: `translateY(${(1 - inProgress) * 30 + outProgress * 12}px)`,
        fontFamily,
      }}
    >
      <div
        style={{
          width: 5,
          height: 162,
          borderRadius: 4,
          backgroundColor: ACCENT,
          boxShadow: "0 0 30px rgba(212,165,116,0.22)",
        }}
      />
      <div>
        <div
          style={{
            color: ACCENT,
            fontFamily: monoFamily,
            fontSize: 19,
            fontWeight: 700,
            letterSpacing: 0,
            textTransform: "uppercase",
          }}
        >
          {beat.kicker}
        </div>
        <div
          style={{
            marginTop: 10,
            color: "#ffffff",
            fontSize: 58,
            lineHeight: 1.02,
            fontWeight: 700,
            letterSpacing: 0,
          }}
        >
          {beat.title}
        </div>
        <div
          style={{
            marginTop: 16,
            color: MUTED,
            fontSize: 25,
            lineHeight: 1.32,
            width: 650,
          }}
        >
          {beat.body}
        </div>
        <div
          style={{ marginTop: 20, display: "flex", gap: 10, flexWrap: "wrap" }}
        >
          {beat.chips.map((chip, index) => (
            <div
              key={chip}
              style={{
                border: "1px solid rgba(212,165,116,0.38)",
                background: "rgba(212,165,116,0.1)",
                color: index === 0 ? "#ffffff" : "rgba(245,245,245,0.76)",
                borderRadius: 999,
                padding: "8px 13px",
                fontFamily: monoFamily,
                fontSize: 15,
                fontWeight: 700,
              }}
            >
              {chip}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};

const CommandChip = ({ label, index }: { label: string; index: number }) => {
  const frame = useCurrentFrame();
  const start = SCAN_START + index * 4;
  const t = ease(frame, start, start + 16);
  const pulse = Math.sin((frame - start) * 0.24) * 0.5 + 0.5;
  return (
    <div
      style={{
        opacity: t,
        transform: `translateY(${(1 - t) * 18}px)`,
        border: "1px solid rgba(212,165,116,0.28)",
        background: `rgba(18,18,18,${0.72 + pulse * 0.08})`,
        color: index % 4 === 0 ? "#ffffff" : "rgba(236,236,236,0.78)",
        borderRadius: 8,
        padding: "11px 14px",
        fontFamily: monoFamily,
        fontSize: 17,
        fontWeight: 700,
        boxShadow: "0 18px 42px rgba(0,0,0,0.22)",
      }}
    >
      {label}
    </div>
  );
};

const SurfaceScan = () => {
  const frame = useCurrentFrame();
  const scanIn = ease(frame, SCAN_START, SCAN_START + 22);
  const scanOut = ease(frame, CLOSEOUT_START - 24, CLOSEOUT_START + 8);
  const opacity = scanIn * (1 - scanOut);
  if (opacity <= 0.001) {
    return null;
  }

  return (
    <div
      style={{
        position: "absolute",
        left: 88,
        top: 126,
        right: 88,
        bottom: 96,
        opacity,
        transform: `translateY(${(1 - scanIn) * 24 - scanOut * 18}px)`,
        fontFamily,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 18 }}>
        <div
          style={{
            width: 5,
            height: 66,
            borderRadius: 4,
            backgroundColor: ACCENT,
          }}
        />
        <div>
          <div
            style={{
              color: ACCENT,
              fontFamily: monoFamily,
              fontSize: 18,
              fontWeight: 700,
            }}
          >
            SCRIPTABLE SURFACE
          </div>
          <div
            style={{
              marginTop: 6,
              color: "#fff",
              fontSize: 58,
              lineHeight: 1,
              fontWeight: 700,
            }}
          >
            The whole Solana loop, mounted.
          </div>
        </div>
      </div>

      <div
        style={{
          marginTop: 46,
          width: 790,
          display: "grid",
          gridTemplateColumns: "repeat(3, 1fr)",
          gap: 12,
        }}
      >
        {TOOL_CHIPS.map((chip, index) => (
          <CommandChip key={chip} label={chip} index={index} />
        ))}
      </div>

      <div
        style={{
          position: "absolute",
          right: 40,
          top: 120,
          width: 440,
          borderRadius: 14,
          border: "1px solid rgba(212,165,116,0.22)",
          background: "rgba(10,10,10,0.72)",
          boxShadow: "0 40px 90px rgba(0,0,0,0.4)",
          padding: 26,
        }}
      >
        <div
          style={{
            color: ACCENT,
            fontFamily: monoFamily,
            fontSize: 16,
            fontWeight: 700,
            marginBottom: 18,
          }}
        >
          WORKBENCH TABS
        </div>
        {[
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
        ].map((label, index) => {
          const itemIn = ease(
            frame,
            SCAN_START + 10 + index * 2,
            SCAN_START + 24 + index * 2,
          );
          return (
            <div
              key={label}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 11,
                opacity: itemIn,
                transform: `translateX(${(1 - itemIn) * 16}px)`,
                marginTop: index === 0 ? 0 : 10,
                color: index < 5 ? "#ffffff" : "rgba(230,230,230,0.68)",
                fontSize: 22,
                fontWeight: 600,
              }}
            >
              <span
                style={{
                  width: 7,
                  height: 7,
                  borderRadius: "50%",
                  backgroundColor: index < 5 ? ACCENT : "rgba(212,165,116,0.4)",
                }}
              />
              {label}
            </div>
          );
        })}
      </div>
    </div>
  );
};

const Closeout = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const intro = ease(frame, CLOSEOUT_START, CLOSEOUT_START + 28);
  const domain = ease(frame, CLOSEOUT_START + 42, CLOSEOUT_START + 68);
  const fade = ease(
    frame,
    FINAL_CLEAR_START,
    APPFLOW_FRAMES - 8,
    Easing.in(Easing.cubic),
  );
  const shove = spring({
    frame: frame - CLOSEOUT_START - 34,
    fps,
    config: { mass: 0.42, damping: 9, stiffness: 260 },
  });
  const glitch = interpolate(
    frame,
    [CLOSEOUT_START + 4, CLOSEOUT_START + 12, CLOSEOUT_START + 30],
    [1, 0.35, 0],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    },
  );
  const domainGlitch = interpolate(
    frame,
    [CLOSEOUT_START + 42, CLOSEOUT_START + 51, CLOSEOUT_START + 72],
    [0, 1.1, 0],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    },
  );
  const markOut = ease(frame, CLOSEOUT_START + 36, CLOSEOUT_START + 52);

  return (
    <AbsoluteFill
      style={{
        alignItems: "center",
        justifyContent: "center",
        opacity: intro * (1 - fade),
        fontFamily,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          transform: `translateX(${-82 * shove}px)`,
        }}
      >
        <div
          style={{
            marginRight: 30,
            opacity: 1 - markOut,
            transform: `translateX(${-64 * markOut}px) scale(${1 - markOut * 0.16})`,
          }}
        >
          <XeroMark size={86} />
        </div>
        <div style={{ display: "flex", alignItems: "baseline" }}>
          <GlitchText
            fontSize={104}
            intensity={Math.max(glitch, domainGlitch * 0.28)}
            seed={frame}
          >
            xero
          </GlitchText>
          <div
            style={{
              width: 570,
              overflow: "hidden",
              opacity: domain,
              clipPath: `inset(0 ${(1 - domain) * 100}% 0 0)`,
              transform: `translateX(${(1 - domain) * 26}px)`,
            }}
          >
            <GlitchText
              fontSize={104}
              intensity={domainGlitch}
              seed={frame + 922}
            >
              shell.com
            </GlitchText>
          </div>
        </div>
      </div>
      <div
        style={{
          marginTop: 32,
          color: "rgba(245,245,245,0.92)",
          fontSize: 48,
          fontWeight: 700,
          opacity: ease(frame, CLOSEOUT_START + 12, CLOSEOUT_START + 36),
        }}
      >
        <GlitchText
          fontSize={48}
          intensity={interpolate(frame % 18, [0, 4, 18], [0.55, 0.16, 0.16], {
            extrapolateLeft: "clamp",
            extrapolateRight: "clamp",
          })}
          seed={frame + 61}
          color="rgba(245,245,245,0.92)"
        >
          One harness. Every Solana surface.
        </GlitchText>
      </div>
    </AbsoluteFill>
  );
};

const AudioBed = () => (
  <>
    <Sequence from={OPEN_CLICK} durationInFrames={12} layout="none">
      <Audio src={staticFile("click.mp3")} trimBefore={1} volume={0.42} />
    </Sequence>
    {BEATS.map((beat) => (
      <Sequence
        key={`${beat.id}-click`}
        from={beat.start + 16}
        durationInFrames={12}
        layout="none"
      >
        <Audio src={staticFile("click.mp3")} trimBefore={1} volume={0.28} />
      </Sequence>
    ))}
    {BEATS.slice(1).map((beat) => (
      <Sequence
        key={`${beat.id}-whoosh`}
        from={beat.start - 10}
        durationInFrames={34}
        layout="none"
      >
        <Audio src={staticFile("whoosh.mp3")} trimBefore={6} volume={0.18} />
      </Sequence>
    ))}
    <Sequence from={SCAN_START} durationInFrames={16} layout="none">
      <Audio src={staticFile("glitch2.mp3")} trimBefore={22} volume={0.12} />
    </Sequence>
    <Sequence from={CLOSEOUT_START} durationInFrames={27} layout="none">
      <Audio src={staticFile("pop.mp3")} trimBefore={3} volume={0.2} />
    </Sequence>
    <Sequence from={CLOSEOUT_START + 42} durationInFrames={13} layout="none">
      <Audio src={staticFile("glitch2.mp3")} trimBefore={22} volume={0.11} />
    </Sequence>
  </>
);

export const AppFlow = () => {
  const frame = useCurrentFrame();
  const { width, height } = useVideoConfig();
  const closeFade = 1 - ease(frame, CLOSEOUT_START - 18, CLOSEOUT_START + 24);
  const introRipplePoint = pointFor({
    point: { fx: 0.946, fy: 0.045 },
    focus: INTRO_FOCUS,
    frame,
    start: 0,
    end: WORKBENCH_IN + 12,
    width,
    height,
  });

  return (
    <AbsoluteFill style={{ backgroundColor: "#070707", overflow: "hidden" }}>
      <SceneBackground />

      <ScreenshotLayer
        src="agent_chat.png"
        start={0}
        end={WORKBENCH_IN + 22}
        focus={INTRO_FOCUS}
        opacityMultiplier={closeFade}
      />
      {BEATS.map((beat) => (
        <ScreenshotLayer
          key={beat.id}
          src={beat.asset}
          start={beat.start - 10}
          end={beat.end + 10}
          focus={beat.focus}
          opacityMultiplier={closeFade}
        />
      ))}

      <AbsoluteFill
        style={{
          background:
            "radial-gradient(92% 76% at 27% 52%, rgba(0,0,0,0.1), rgba(0,0,0,0.58) 100%)",
          opacity: closeFade,
        }}
      />

      <IntroCopy />
      <BeatCopy />
      <SurfaceScan />
      <Closeout />
      <IntroCursor />
      <ClickRipple
        x={introRipplePoint.x}
        y={introRipplePoint.y}
        at={OPEN_CLICK}
      />
      {BEATS.map((beat) => (
        <BeatHotspot key={`${beat.id}-hotspot`} beat={beat} />
      ))}
      <AudioBed />
    </AbsoluteFill>
  );
};
