import {
  AbsoluteFill,
  Easing,
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
import { measureText } from "@remotion/layout-utils";
import { SceneBackground } from "../SceneBackground";

const { fontFamily } = loadFont("normal", { weights: ["600"] });
// Italic thin face for the emphasised "your" in the final phrase.
loadFont("italic", { weights: ["100"] });
const FONT_WEIGHT = 600;

// The four quadrants of the Xero mark, ordered clockwise starting top-left.
// Each path has a single rounded outer corner; together they form a 420x420 grid.
const QUADRANTS = [
  {
    // top-left
    d: "M182.98 182.984L0.000640869 182.984L0.000629244 50.0041C0.00062683 22.3898 22.3864 0.00413391 50.0006 0.0041315L182.98 0.00411987L182.98 182.984Z",
    fill: "#D4A574",
  },
  {
    // top-right
    d: "M237.02 0L370 0C397.614 0 420 22.3858 420 50V182.98H237.02V0Z",
    fill: "#4E4337",
  },
  {
    // bottom-right
    d: "M237.02 237.023H419.999V370.004C419.999 397.618 397.614 420.004 369.999 420.004H237.02V237.023Z",
    fill: "#D4A574",
  },
  {
    // bottom-left
    d: "M0 237.023H182.98V420.004H50C22.3857 420.004 0 397.618 0 370.004L0 237.023Z",
    fill: "#4E4337",
  },
] as const;

// Reveal timing (frames @ 30fps) — snappy outline-then-fill, staggered clockwise.
const START_DELAY = 2;
const STAGGER = 3;
const DRAW_DURATION = 11;
const FILL_DURATION = 7;
const FILL_OVERLAP = 4;

// The wordmark drive-in + logo shove kick off here.
const LOCKUP_START = 30;
// The logo recoils a few frames after the wordmark starts pushing in.
const LOGO_SHOVE_DELAY = 4;
// The gleam sweeps once the shove has settled (~frame 47), after a short beat.
const SHINE_START = 55;
const SHINE_DURATION = 14;
const SHINE_HALF_WIDTH = 0.38; // half-width of the bright band (gradient units)
const SHINE_PEAK = 0.6; // peak white opacity of the gleam

// Second beat: the wordmark shoves the logo off-screen and centers itself,
// then rapidly cycles through the taglines.
const SECOND_SHOVE_START = 88;
const SWITCH_START = 114;
const PHRASE_STRIDE = 22; // frames each tagline holds before a hard cut
const GLITCH_IN = 5; // glitch frames as a tagline appears
const GLITCH_OUT = 3; // glitch frames just before a tagline cuts away
const LAST_GLITCH_OUT = 194; // the final tagline glitches out to bare bg here
const PHRASES = [
  "Create Agents",
  "Build workflows",
  "Take the reins",
  "This is your harness",
] as const;
const LAST_PHRASE = PHRASES[PHRASES.length - 1];

const clamp01 = (n: number) => Math.max(0, Math.min(1, n));

// Render a tagline, italicising the thin "your" in the final phrase.
const renderPhrase = (phrase: string) => {
  if (phrase !== LAST_PHRASE) {
    return phrase;
  }
  return (
    <>
      This is <span style={{ fontStyle: "italic", fontWeight: 100 }}>your</span>{" "}
      harness
    </>
  );
};

// Renders text with a digital-glitch treatment whose strength is `intensity`
// (0 = clean white). The effect layers an RGB channel split with sporadic
// horizontal slice displacement and a subtle flicker. All randomness is seeded
// off the frame so renders are deterministic.
const GlitchText: React.FC<{
  children: React.ReactNode;
  fontSize: number;
  intensity: number;
  seed: number;
}> = ({ children, fontSize, intensity, seed }) => {
  const base: React.CSSProperties = {
    fontFamily,
    fontWeight: FONT_WEIGHT,
    fontSize,
    lineHeight: 1,
    whiteSpace: "nowrap",
  };

  if (intensity <= 0.001) {
    return <span style={{ ...base, color: "#ffffff" }}>{children}</span>;
  }

  const rnd = (k: string) => random(`${seed}-${k}`);
  const signed = (k: string) => rnd(k) * 2 - 1;

  const shift = (0.04 + 0.05 * rnd("rgb")) * fontSize * intensity;
  const ry = signed("ry") * 0.028 * fontSize * intensity;
  const baseJitter = signed("bx") * 0.045 * fontSize * intensity;
  const flicker = 1 - rnd("flk") * 0.35 * intensity;

  const layer = (
    color: string,
    dx: number,
    dy: number,
    extra?: React.CSSProperties,
  ): React.CSSProperties => ({
    ...base,
    position: "absolute",
    top: 0,
    left: 0,
    color,
    mixBlendMode: "screen",
    transform: `translate(${dx}px, ${dy}px)`,
    ...extra,
  });

  // Several torn horizontal slices, firing on most frames during the burst.
  const slices = [0, 1, 2, 3, 4]
    .filter((k) => rnd(`slice-on-${k}`) < 0.72)
    .map((k) => {
      const top = Math.round(rnd(`slice-top-${k}`) * 76);
      const h = 4 + Math.round(rnd(`slice-h-${k}`) * 18);
      const dx = signed(`slice-dx-${k}`) * 0.24 * fontSize * intensity;
      const dy = signed(`slice-dy-${k}`) * 0.02 * fontSize * intensity;
      return { k, top, bottom: Math.max(0, 100 - top - h), dx, dy };
    });

  return (
    <span
      style={{
        ...base,
        position: "relative",
        display: "inline-block",
        isolation: "isolate",
        opacity: flicker,
        transform: `translateX(${baseJitter}px)`,
      }}
    >
      {/* green core stays put (keeps text legible); red/blue split outward */}
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
      {slices.map((s) => (
        <span
          key={s.k}
          style={layer("#ffffff", s.dx, s.dy, {
            clipPath: `inset(${s.top}% 0 ${s.bottom}% 0)`,
          })}
        >
          {children}
        </span>
      ))}
    </span>
  );
};

// The taglines hard-cut from one to the next — no fade, no slide. A glitch
// burst peaks at each cut. Exactly one tagline is shown at a time.
const PhraseSwitcher: React.FC<{ size: number; maxWidth: number }> = ({
  size,
  maxWidth,
}) => {
  const frame = useCurrentFrame();
  if (frame < SWITCH_START) {
    return null;
  }

  const index = Math.min(
    PHRASES.length - 1,
    Math.floor((frame - SWITCH_START) / PHRASE_STRIDE),
  );
  const phrase = PHRASES[index];

  // Auto-fit: shrink only if the phrase would overflow (the long final one).
  // Measured as semibold, a safe over-estimate for the italic part.
  const measured = measureText({
    text: phrase,
    fontFamily,
    fontSize: size,
    fontWeight: FONT_WEIGHT,
    letterSpacing: "0px",
  }).width;
  const fontSize = size * Math.min(1, maxWidth / measured);

  // Glitch peaks as this tagline appears and again just before it cuts away.
  const start = SWITCH_START + index * PHRASE_STRIDE;
  const inGlitch = interpolate(frame - start, [0, GLITCH_IN], [1, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const isLast = index === PHRASES.length - 1;
  const untilCut = SWITCH_START + (index + 1) * PHRASE_STRIDE - frame;
  const outGlitch = isLast
    ? 0
    : interpolate(untilCut, [0, GLITCH_OUT], [1, 0], {
        extrapolateLeft: "clamp",
        extrapolateRight: "clamp",
      });

  // The final tagline glitches hard, then dissolves to nothing but the bg.
  const finalGlitch = isLast
    ? interpolate(frame, [LAST_GLITCH_OUT, LAST_GLITCH_OUT + 6], [0, 1], {
        extrapolateLeft: "clamp",
        extrapolateRight: "clamp",
      })
    : 0;
  const fadeOut = isLast
    ? interpolate(frame, [LAST_GLITCH_OUT + 2, LAST_GLITCH_OUT + 9], [0, 1], {
        extrapolateLeft: "clamp",
        extrapolateRight: "clamp",
        easing: Easing.in(Easing.cubic),
      })
    : 0;
  const intensity = Math.max(inGlitch, outGlitch, finalGlitch);

  return (
    <AbsoluteFill
      style={{
        justifyContent: "center",
        alignItems: "center",
        opacity: 1 - fadeOut,
      }}
    >
      <GlitchText fontSize={fontSize} intensity={intensity} seed={frame}>
        {renderPhrase(phrase)}
      </GlitchText>
    </AbsoluteFill>
  );
};

const Quadrant: React.FC<{
  d: string;
  fill: string;
  index: number;
}> = ({ d, fill, index }) => {
  const frame = useCurrentFrame();
  const start = START_DELAY + index * STAGGER;

  const drawProgress = interpolate(
    frame,
    [start, start + DRAW_DURATION],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.bezier(0.16, 1, 0.3, 1),
    },
  );

  const fillStart = start + DRAW_DURATION - FILL_OVERLAP;
  const fillOpacity = interpolate(
    frame,
    [fillStart, fillStart + FILL_DURATION],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.bezier(0.4, 0, 0.2, 1),
    },
  );

  return (
    <path
      d={d}
      fill={fill}
      fillOpacity={fillOpacity}
      stroke={fill}
      strokeWidth={10}
      strokeLinejoin="round"
      pathLength={1}
      strokeDasharray={1}
      strokeDashoffset={1 - drawProgress}
    />
  );
};

// A bright band swept along the top-left → bottom-right diagonal, clipped to
// the logo shapes so the gleam only travels across the quadrants.
const Shine: React.FC = () => {
  const frame = useCurrentFrame();

  const pos = interpolate(
    frame,
    [SHINE_START, SHINE_START + SHINE_DURATION],
    [-0.4, 1.4],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.bezier(0.45, 0, 0.2, 1),
    },
  );

  const lead = clamp01(pos - SHINE_HALF_WIDTH);
  const mid = clamp01(pos);
  const trail = clamp01(pos + SHINE_HALF_WIDTH);

  return (
    <g clipPath="url(#logoClip)">
      <defs>
        <linearGradient id="shineGrad" x1="0" y1="0" x2="1" y2="1">
          <stop offset={0} stopColor="#ffffff" stopOpacity={0} />
          <stop offset={lead} stopColor="#ffffff" stopOpacity={0} />
          <stop offset={mid} stopColor="#ffffff" stopOpacity={SHINE_PEAK} />
          <stop offset={trail} stopColor="#ffffff" stopOpacity={0} />
          <stop offset={1} stopColor="#ffffff" stopOpacity={0} />
        </linearGradient>
      </defs>
      <rect x={0} y={0} width={420} height={420} fill="url(#shineGrad)" />
    </g>
  );
};

export const LogoReveal: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps, width, height } = useVideoConfig();
  const size = Math.min(width, height) * 0.15;
  const fontSize = size * 1.43;
  const gap = size * 0.42;

  // Measure the wordmark so the final logo + "xero" lockup sits centered.
  const { width: textWidth } = measureText({
    text: "xero",
    fontFamily,
    fontSize,
    fontWeight: FONT_WEIGHT,
    letterSpacing: "0px",
  });

  // The lockup is centered at rest; the logo sits left-of-center by restOffset.
  const restOffset = (gap + textWidth) / 2;

  // The wordmark is the "pusher": it drives in from the right and fades up,
  // arriving with momentum (ease-out).
  const textProgress = interpolate(
    frame,
    [LOCKUP_START, LOCKUP_START + 12],
    [0, 1],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    },
  );
  const textSlideX = (1 - textProgress) * size * 0.45;
  const textOpacity = interpolate(
    frame,
    [LOCKUP_START, LOCKUP_START + 10],
    [0, 1],
    { extrapolateLeft: "clamp", extrapolateRight: "clamp" },
  );

  // The logo is shoved in reaction: it holds dead-center, then recoils left on
  // "impact" a few frames after the wordmark starts driving in. The spring's
  // overshoot/settle is the physics of getting bumped.
  const shove = spring({
    frame: frame - LOCKUP_START - LOGO_SHOVE_DELAY,
    fps,
    config: { mass: 0.5, damping: 12, stiffness: 200 },
  });
  const logoShoveX = restOffset * (1 - shove);
  const logoNudgeY = size * 0.05;

  // Clip the wordmark's left edge to just past the logo's (moving) right edge,
  // so "xero" emerges cleanly from behind the logo instead of passing through
  // it. Goes to 0 once the logo has cleared, revealing the full word.
  const logoRightMargin = size * 0.14;
  const textClipLeft = Math.max(
    0,
    logoShoveX - gap - textSlideX + logoRightMargin,
  );

  // Second shove: the wordmark drives left to screen-center. Aggressive — a
  // stiff, lightly-damped spring snaps the text over fast (with overshoot). The
  // logo gets knocked a short way left and quickly fades out during the shove
  // (rather than flying all the way off the edge).
  const secondShove = spring({
    frame: frame - SECOND_SHOVE_START,
    fps,
    config: { mass: 0.4, damping: 9, stiffness: 320 },
  });
  const textCenterX = -((size + gap) / 2) * secondShove;
  const logoEjectX = interpolate(
    frame,
    [SECOND_SHOVE_START + 2, SECOND_SHOVE_START + 16],
    [0, -size * 2.6],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    },
  );
  const logoOpacity = interpolate(
    frame,
    [SECOND_SHOVE_START + 6, SECOND_SHOVE_START + 15],
    [1, 0],
    { extrapolateLeft: "clamp", extrapolateRight: "clamp" },
  );

  // Match the later phrase-to-phrase cuts: the centered "xero" glitches away
  // for the last few frames before the first tagline glitches in.
  const wordmarkGlitch = interpolate(
    SWITCH_START - frame,
    [0, GLITCH_OUT],
    [1, 0],
    {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    },
  );
  const nudgePx = -0.06 * fontSize;

  return (
    <AbsoluteFill style={{ backgroundColor: "#070707" }}>
      <SceneBackground />
      <AbsoluteFill style={{ justifyContent: "center", alignItems: "center" }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap,
          }}
        >
          <svg
            width={size}
            height={size}
            viewBox="0 0 420 420"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
            style={{
              flexShrink: 0,
              opacity: logoOpacity,
              transform: `translate(${logoShoveX + logoEjectX}px, ${logoNudgeY}px)`,
            }}
          >
            <defs>
              <clipPath id="logoClip">
                {QUADRANTS.map((quadrant) => (
                  <path key={quadrant.d} d={quadrant.d} />
                ))}
              </clipPath>
            </defs>
            {QUADRANTS.map((quadrant, index) => (
              <Quadrant
                key={quadrant.d}
                d={quadrant.d}
                fill={quadrant.fill}
                index={index}
              />
            ))}
            <Shine />
          </svg>
          <span
            style={{
              fontFamily,
              fontWeight: FONT_WEIGHT,
              fontSize,
              lineHeight: 1,
              color: "#ffffff",
              opacity: frame < SWITCH_START ? textOpacity : 0,
              whiteSpace: "nowrap",
              // Drive in from the right, then center on the second shove.
              // nudgePx optically centers the glyphs.
              transform: `translate(${textSlideX + textCenterX}px, ${nudgePx}px)`,
              // Hide whatever would overlap the logo as it recoils.
              clipPath: `inset(0 0 0 ${textClipLeft}px)`,
            }}
          >
            {wordmarkGlitch > 0.001 ? (
              <GlitchText
                fontSize={fontSize}
                intensity={wordmarkGlitch}
                seed={frame}
              >
                xero
              </GlitchText>
            ) : (
              "xero"
            )}
          </span>
        </div>
      </AbsoluteFill>
      <PhraseSwitcher size={size} maxWidth={width * 0.84} />
      {/* pencil scribble while the logo outline is drawn (from the clip's 8s mark) */}
      <Sequence from={START_DELAY} durationInFrames={20} layout="none">
        <Audio
          src={staticFile("scribble.mp3")}
          trimBefore={240}
          volume={0.15}
        />
      </Sequence>
      <Sequence from={SHINE_START} durationInFrames={45} layout="none">
        <Audio src={staticFile("gleam.mp3")} />
      </Sequence>
      {/* glitch sound on each text glitch (first 40% of the clip trimmed off) */}
      {[
        SWITCH_START,
        SWITCH_START + PHRASE_STRIDE,
        SWITCH_START + 2 * PHRASE_STRIDE,
        SWITCH_START + 3 * PHRASE_STRIDE,
      ].map((at) => (
        <Sequence key={at} from={at} durationInFrames={13} layout="none">
          <Audio src={staticFile("glitch2.mp3")} trimBefore={22} volume={0.1} />
        </Sequence>
      ))}
      {/* pop each time the logo is shoved left (first recoil + the eject) */}
      {[LOCKUP_START + LOGO_SHOVE_DELAY, SECOND_SHOVE_START].map((at) => (
        <Sequence key={at} from={at} durationInFrames={27} layout="none">
          <Audio src={staticFile("pop.mp3")} trimBefore={3} volume={0.2} />
        </Sequence>
      ))}
    </AbsoluteFill>
  );
};
