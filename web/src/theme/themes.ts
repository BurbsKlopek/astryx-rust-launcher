import type { DefinedTheme } from '@astryxdesign/core/theme';
import { neutralTheme } from '@astryxdesign/theme-neutral/built';
import { gothicTheme } from '@astryxdesign/theme-gothic/built';
import { y2kTheme } from '@astryxdesign/theme-y2k/built';
import { matchaTheme } from '@astryxdesign/theme-matcha/built';

export type AstryxBaseThemeId = 'neutral' | 'gothic' | 'y2k' | 'matcha';

export type WallpaperPresetId =
  | 'live-weather'
  | 'nebula-aurora'
  | 'midnight-cobalt'
  | 'cyber-orchid'
  | 'emerald-obsidian'
  | 'sunset-velvet';

export type WeatherSceneOverride =
  | 'auto'
  | 'clear-day'
  | 'golden'
  | 'clear-night'
  | 'cloudy'
  | 'fog'
  | 'rain'
  | 'storm'
  | 'snow';

export interface TvThemeConfig {
  astryxTheme: AstryxBaseThemeId;
  wallpaper: WallpaperPresetId;
  weatherScene: WeatherSceneOverride;
  customWallpaperUrl: string;
  wallpaperDim: number;
  accentColor: string;
  accentSecondary: string;
  glassBlurPx: number;
  glassOpacity: number;
  menuTransparency: boolean;
  menuOpacity: number;
  shadowStrength: 'soft' | 'deep' | 'cinematic';
  boldTypography: boolean;
  tileRadiusPx: number;
  showDynamicWidgets: boolean;
}

export interface LauncherSettings {
  showHero: boolean;
  showRecents: boolean;
  showLabels: boolean;
  showWeather: boolean;
  iconSize: 'compact' | 'standard' | 'large';
  layoutMode: 'categories' | 'grid';
  sortBy: 'alpha' | 'recent';
  clock24h: boolean;
  showMobileApps: boolean;
  showSystemApps: boolean;
  hiddenApps: string[];
}

export const DEFAULT_LAUNCHER_SETTINGS: LauncherSettings = {
  showHero: true,
  showRecents: true,
  showLabels: true,
  showWeather: true,
  iconSize: 'standard',
  layoutMode: 'categories',
  sortBy: 'alpha',
  clock24h: true,
  showMobileApps: true,
  showSystemApps: false,
  hiddenApps: [],
};

export function applyLauncherSettingsCss(settings: LauncherSettings): void {
  const root = document.documentElement;
  const sizeMap = {
    compact: { circle: '60px', slot: '80px' },
    standard: { circle: '72px', slot: '92px' },
    large: { circle: '86px', slot: '108px' },
  };
  const chosen = sizeMap[settings.iconSize] ?? sizeMap.standard;
  root.style.setProperty('--tv-icon-size', chosen.circle);
  root.style.setProperty('--tv-icon-slot', chosen.slot);
}

export const DEFAULT_TV_THEME: TvThemeConfig = {
  astryxTheme: 'gothic',
  wallpaper: 'live-weather',
  weatherScene: 'auto',
  customWallpaperUrl: '',
  wallpaperDim: 0.45,
  accentColor: '#38bdf8',
  accentSecondary: '#a855f7',
  glassBlurPx: 10,
  glassOpacity: 0.14,
  menuTransparency: true,
  menuOpacity: 0.68,
  shadowStrength: 'deep',
  boldTypography: true,
  tileRadiusPx: 14,
  showDynamicWidgets: false,
};

export const ASTRYX_THEMES: Record<
  AstryxBaseThemeId,
  { label: string; description: string; theme: DefinedTheme }
> = {
  gothic: {
    label: 'Astryx Gothic',
    description: 'High-contrast obsidian surfaces & dramatic display type',
    theme: gothicTheme,
  },
  neutral: {
    label: 'Astryx Neutral',
    description: 'Clean studio precision with balanced modern proportions',
    theme: neutralTheme,
  },
  y2k: {
    label: 'Astryx Y2K',
    description: 'Chrome iridescence, crisp bevels & bold futuristic energy',
    theme: y2kTheme,
  },
  matcha: {
    label: 'Astryx Matcha',
    description: 'Organic warmth with lush tinted contrast',
    theme: matchaTheme,
  },
};

export const WALLPAPER_PRESETS: Record<
  WallpaperPresetId,
  { label: string; background: string; accent: string; accent2: string }
> = {
  'live-weather': {
    label: 'Live Weather',
    background:
      'radial-gradient(ellipse 80% 60% at 25% 15%, rgba(56,189,248,0.34), transparent 60%), linear-gradient(160deg, #071222 0%, #0d1d34 50%, #070b14 100%)',
    accent: '#38bdf8',
    accent2: '#a855f7',
  },
  'nebula-aurora': {
    label: 'Nebula Aurora',
    background:
      'radial-gradient(ellipse 85% 65% at 15% 10%, rgba(168,85,247,0.42), transparent 60%), radial-gradient(ellipse 70% 55% at 88% 18%, rgba(56,189,248,0.34), transparent 58%), radial-gradient(ellipse 65% 60% at 50% 95%, rgba(236,72,153,0.24), transparent 62%), linear-gradient(155deg, #070812 0%, #0d1024 48%, #090616 100%)',
    accent: '#a855f7',
    accent2: '#38bdf8',
  },
  'midnight-cobalt': {
    label: 'Midnight Pulse',
    background:
      'radial-gradient(ellipse 80% 60% at 20% 15%, rgba(59,130,246,0.44), transparent 58%), radial-gradient(ellipse 70% 55% at 82% 80%, rgba(6,182,212,0.32), transparent 60%), linear-gradient(150deg, #040914 0%, #0a162e 55%, #050a18 100%)',
    accent: '#3b82f6',
    accent2: '#22d3ee',
  },
  'cyber-orchid': {
    label: 'Cyber Orchid',
    background:
      'radial-gradient(ellipse 75% 60% at 80% 15%, rgba(244,63,94,0.38), transparent 60%), radial-gradient(ellipse 75% 60% at 18% 82%, rgba(139,92,246,0.42), transparent 60%), linear-gradient(145deg, #0c0614 0%, #1d0c2e 52%, #0b0612 100%)',
    accent: '#f43f5e',
    accent2: '#a78bfa',
  },
  'emerald-obsidian': {
    label: 'Emerald Glass',
    background:
      'radial-gradient(ellipse 78% 60% at 20% 15%, rgba(16,185,129,0.36), transparent 60%), radial-gradient(ellipse 70% 55% at 85% 75%, rgba(45,212,191,0.26), transparent 60%), linear-gradient(150deg, #040d0b 0%, #081e1a 52%, #050b0c 100%)',
    accent: '#10b981',
    accent2: '#2dd4bf',
  },
  'sunset-velvet': {
    label: 'Velvet Amber',
    background:
      'radial-gradient(ellipse 80% 60% at 18% 12%, rgba(245,158,11,0.36), transparent 60%), radial-gradient(ellipse 75% 60% at 84% 78%, rgba(239,68,68,0.32), transparent 60%), linear-gradient(150deg, #100806 0%, #1f1014 50%, #0b070c 100%)',
    accent: '#f59e0b',
    accent2: '#fb7185',
  },
};

export function applyThemeCssVars(cfg: TvThemeConfig): void {
  const root = document.documentElement;
  const preset = WALLPAPER_PRESETS[cfg.wallpaper] ?? WALLPAPER_PRESETS['nebula-aurora'];

  const shadowMap = {
    soft: '0 4px 12px rgba(0,0,0,0.34)',
    deep: '0 8px 20px rgba(0,0,0,0.48)',
    cinematic: '0 12px 28px rgba(0,0,0,0.62)',
  };

  const customUrl = (cfg.customWallpaperUrl ?? '').trim();
  const dim = Math.min(0.85, Math.max(0.15, cfg.wallpaperDim ?? 0.45));
  const bgValue = customUrl
    ? `linear-gradient(rgba(6,7,14,${dim}), rgba(6,7,14,${Math.min(0.92, dim + 0.15)})), url("${customUrl.replace(/"/g, '')}") center/cover no-repeat`
    : preset.background;

  root.style.setProperty('--tv-wallpaper', bgValue);
  root.style.setProperty('--tv-accent', cfg.accentColor);
  root.style.setProperty('--tv-accent-2', cfg.accentSecondary);
  const menuAlpha = cfg.menuTransparency === false ? 0.96 : Math.min(0.95, Math.max(0.3, cfg.menuOpacity ?? 0.68));
  const dockAlpha = cfg.menuTransparency === false ? 0.92 : Math.max(0.25, menuAlpha - 0.16);
  root.style.setProperty('--tv-glass-blur', `${cfg.glassBlurPx}px`);
  root.style.setProperty('--tv-glass-bg', `rgba(255, 255, 255, ${cfg.glassOpacity})`);
  root.style.setProperty('--tv-glass-bg-strong', `rgba(14, 18, 30, ${menuAlpha})`);
  root.style.setProperty('--tv-menu-bg', `rgba(14, 18, 30, ${menuAlpha})`);
  root.style.setProperty('--tv-dock-bg', `rgba(14, 18, 30, ${dockAlpha})`);
  root.style.setProperty('--tv-shadow', shadowMap[cfg.shadowStrength]);
  root.style.setProperty('--tv-tile-radius', `${cfg.tileRadiusPx}px`);
  root.style.setProperty('--tv-heading-weight', cfg.boldTypography ? '900' : '700');
  root.style.setProperty('--tv-body-weight', cfg.boldTypography ? '700' : '500');
}
