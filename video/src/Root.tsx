import "./index.css";
import { Composition } from "remotion";
import { LogoReveal } from "./scenes/LogoReveal";

export const RemotionRoot: React.FC = () => {
  return (
    <>
      <Composition
        id="LogoReveal"
        component={LogoReveal}
        durationInFrames={200}
        fps={30}
        width={1920}
        height={1080}
      />
    </>
  );
};
