export type Pronunciation = { from: string; to: string };
export type AudioMix = { original: number; voice: number; duck: boolean; duckLevel: number };
export type Logo = { enabled: boolean; path: string; sha256: string; size: number; opacity: number; x: number; y: number };
export type SourceMask = { enabled: boolean; mode: "blur" | "solid"; x: number; y: number; width: number; height: number; color: string };
export type RenderSettings = {
  audio: AudioMix; pronunciations: Record<string, Pronunciation[]>; logo: Logo; mask: SourceMask;
  aspect: "original" | "9:16" | "1:1" | "16:9";
  framing: "crop" | "blur";
  focusX: number; focusY: number;
  font: string; size: number; color: string; outlineColor: string;
  outline: number; background: boolean; captionX: number; captionY: number;
};
export const DEFAULT_RENDER: RenderSettings = {
  audio: { original: 0, voice: 100, duck: true, duckLevel: 22 }, pronunciations: {},
  logo: { enabled: false, path: "", sha256: "", size: 18, opacity: 80, x: 95, y: 5 },
  mask: { enabled: false, mode: "blur", x: 5, y: 75, width: 90, height: 18, color: "#000000" },
  aspect: "original", framing: "blur", focusX: 50, focusY: 50,
  font: "Arial", size: 4.2, color: "#ffffff", outlineColor: "#101010",
  outline: 1, background: false, captionX: 50, captionY: 88,
};
export const CAPTION_FONTS = ["Arial", "Tahoma", "Verdana", "Times New Roman"];

export function cleanRender(value: unknown): RenderSettings {
  const s = value && typeof value === "object" ? value as Partial<RenderSettings> : {};
  const number = (key: keyof RenderSettings, min: number, max: number) => {
    const v = s[key];
    return typeof v === "number" && Number.isFinite(v) ? Math.min(max, Math.max(min,v)) : DEFAULT_RENDER[key] as number;
  };
  const color = (v: unknown, fallback: string) => typeof v === "string" && /^#[\da-f]{6}$/i.test(v) ? v : fallback;
  const bounded = (v: unknown, fallback: number, min: number, max: number) => typeof v === "number" && Number.isFinite(v) ? Math.max(min,Math.min(max,v)) : fallback;
  const a=s.audio ?? DEFAULT_RENDER.audio, l=s.logo ?? DEFAULT_RENDER.logo, m=s.mask ?? DEFAULT_RENDER.mask;
  const x=bounded(m.x,5,0,99), y=bounded(m.y,75,0,99);
  const pronunciations: Record<string,Pronunciation[]> = {};
  if(s.pronunciations && typeof s.pronunciations === "object") for(const [lang,list] of Object.entries(s.pronunciations)) {
    if(/^[a-z]{2}$/.test(lang) && Array.isArray(list)) pronunciations[lang]=list.filter(r=>r&&typeof r.from==="string"&&typeof r.to==="string"&&r.from.trim()&&r.to.trim()).slice(0,200).map(r=>({from:r.from.trim().slice(0,70),to:r.to.trim().slice(0,140)}));
  }
  return {
    audio: {original:bounded(a.original,0,0,100),voice:bounded(a.voice,100,0,200),duck:a.duck!==false,duckLevel:bounded(a.duckLevel,22,0,100)},
    logo: {enabled:l.enabled===true && typeof l.path==="string" && /^[a-f0-9]{64}$/i.test(l.sha256??""),path:typeof l.path==="string"?l.path:"",sha256:typeof l.sha256==="string"?l.sha256:"",size:bounded(l.size,18,3,50),opacity:bounded(l.opacity,80,0,100),x:bounded(l.x,95,0,100),y:bounded(l.y,5,0,100)},
    mask: {enabled:m.enabled===true,mode:m.mode==="solid"?"solid":"blur",x,y,width:bounded(m.width,Math.min(90,100-x),1,100-x),height:bounded(m.height,Math.min(18,100-y),1,100-y),color:color(m.color,"#000000")},
    pronunciations,
    aspect: ["9:16", "1:1", "16:9"].includes(s.aspect ?? "") ? s.aspect! : "original",
    framing: s.framing === "crop" ? "crop" : "blur", focusX: number("focusX",0,100), focusY: number("focusY",0,100),
    font: CAPTION_FONTS.includes(s.font ?? "") ? s.font! : DEFAULT_RENDER.font,
    size: number("size",2,8), color: color(s.color,DEFAULT_RENDER.color), outlineColor: color(s.outlineColor,DEFAULT_RENDER.outlineColor),
    outline: number("outline",0,5), background: s.background === true,
    captionX: number("captionX",15,85), captionY: number("captionY",20,92),
  };
}

export type ChannelPreset = {
  id: string; name: string; render: RenderSettings; target: string; voice: string;
  audioOnly: boolean; textOnly: boolean; burnCaptions: boolean; skipTranslate: boolean; useTerms: boolean;
};

export function readRender(): RenderSettings {
  try { return cleanRender(JSON.parse(localStorage.getItem("video-render") ?? "null")); }
  catch { return { ...DEFAULT_RENDER }; }
}

export function readPresets(): ChannelPreset[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem("channel-presets") ?? "[]");
    if (!Array.isArray(value)) return [];
    return value.filter((p) => p && typeof p.id === "string" && typeof p.name === "string" && typeof p.target === "string" && typeof p.voice === "string")
      .map((p) => ({ id: p.id, name: p.name.slice(0,60), target: p.target, voice: p.voice,
        render: cleanRender(p.render), audioOnly: p.audioOnly === true, textOnly: p.audioOnly !== true && p.textOnly === true,
        burnCaptions: p.burnCaptions === true, skipTranslate: p.skipTranslate === true, useTerms: p.useTerms === true }));
  } catch { return []; }
}
