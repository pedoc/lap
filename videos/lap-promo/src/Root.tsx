import { Composition, Folder } from "remotion";
import { LapPromo } from "./Composition";
import { Opening } from "./scenes/Opening";
import { Search } from "./scenes/Search";
import { Understand } from "./scenes/Understand";
import { Chat } from "./scenes/Chat";
import { Organize } from "./scenes/Organize";
import { Places } from "./scenes/Places";
import { Ending } from "./scenes/Ending";
import { Cast } from "./scenes/Cast";
export const RemotionRoot = () => (
  <>
    <Composition
      id="Lap-Promo-4K"
      component={LapPromo}
      width={3840}
      height={2160}
      fps={30}
      durationInFrames={997}
      defaultProps={{ bgm: true }}
    />
    <Folder name="Scenes">
      <Composition
        id="Opening"
        component={Opening}
        width={1920}
        height={1080}
        fps={30}
        durationInFrames={111}
        defaultProps={{
          headline: "回忆，\n不该难找。",
          subline: "让每一张照片，重新被看见。",
        }}
      />
      <Composition
        id="Search"
        component={Search}
        width={1920}
        height={1080}
        fps={30}
        durationInFrames={111}
        defaultProps={{
          headline: "找照片，\n也能说人话。",
          subline: "本地语义检索\n自由组合筛选",
        }}
      />
      <Composition
        id="Understand"
        component={Understand}
        width={1920}
        height={1080}
        fps={30}
        durationInFrames={166}
        defaultProps={{
          headline: "读出\n文字。",
          subline: "图片里的字，也能搜。",
        }}
      />
      <Composition
        id="Chat"
        component={Chat}
        width={1920}
        height={1080}
        fps={30}
        durationInFrames={111}
        defaultProps={{
          headline: "让照片，\n参与对话。",
          subline: "两图比较 · 构图建议",
        }}
      />
      <Composition
        id="Organize"
        component={Organize}
        width={1920}
        height={1080}
        fps={30}
        durationInFrames={110}
        defaultProps={{
          headline: "让整理，\n自己发生。",
          subline: "智能相册\n保存规则，持续归集。",
        }}
      />
      <Composition
        id="Places"
        component={Places}
        width={1920}
        height={1080}
        fps={30}
        durationInFrames={111}
        defaultProps={{
          headline: "把回忆，\n放回地图。",
          subline: "照片定位 · 原生 3D / 卫星视图",
        }}
      />
      <Composition
        id="Cast"
        component={Cast}
        width={1920}
        height={1080}
        fps={30}
        durationInFrames={111}
        defaultProps={{
          headline: "好照片，\n大屏一起看。",
          subline: "DLNA 电视投屏\n照片与视频，都能分享",
        }}
      />
      <Composition
        id="Ending"
        component={Ending}
        width={1920}
        height={1080}
        fps={30}
        durationInFrames={166}
        defaultProps={{
          headline: "你的照片。你的掌控。",
          subline: "本地优先 · 开源免费",
        }}
      />
    </Folder>
  </>
);
