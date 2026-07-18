import { Audio } from "@remotion/media";
import { AbsoluteFill, interpolate, Series, staticFile } from "remotion";
import { LogoReveal } from "./scenes/LogoReveal";
import { AppFlow, APPFLOW_FRAMES } from "./scenes/AppFlow";

export { APPFLOW_FRAMES };

// Per-scene lengths (frames @ 30fps). Shared so the parent and the standalone
// scene compositions can't drift out of sync.
export const LOGO_FRAMES = 204;
export const MAIN_FRAMES = LOGO_FRAMES + APPFLOW_FRAMES;

// The full video: each scene plays in sequence (hard cut between them), with
// background music whose first 5s (150 frames @ 30fps) are trimmed off. The
// music eases in over the logo draw and fades out with the closing card so
// the video doesn't end on a hard audio cut.
export const Main: React.FC = () => {
  return (
    <AbsoluteFill>
      <Audio
        src={staticFile("bgm2.mp3")}
        trimBefore={150}
        volume={(f) =>
          interpolate(
            f,
            [0, 24, MAIN_FRAMES - 70, MAIN_FRAMES - 10],
            [0, 0.1, 0.1, 0],
            { extrapolateLeft: "clamp", extrapolateRight: "clamp" },
          )
        }
      />
      <Series>
        <Series.Sequence durationInFrames={LOGO_FRAMES}>
          <LogoReveal />
        </Series.Sequence>
        <Series.Sequence durationInFrames={APPFLOW_FRAMES}>
          <AppFlow />
        </Series.Sequence>
      </Series>
    </AbsoluteFill>
  );
};
