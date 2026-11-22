export interface TvAppInfo {
  packageName: string;
  label: string;
  category: 'streaming' | 'music' | 'games' | 'tools' | string;
  isLeanback: boolean;
  hasBanner: boolean;
  isSystemApp: boolean;
  versionName: string;
  bannerUrl: string;
  iconUrl: string;
}

export interface TvSystemStatus {
  networkType: 'ethernet' | 'wifi' | 'cellular' | 'none';
  isConnected: boolean;
  isVpnActive: boolean;
  deviceModel: string;
  androidVersion: string;
  isTv: boolean;
  ramFreeMb?: number;
  ramTotalMb?: number;
  storageFreeGb?: number;
  storageTotalGb?: number;
}

interface NativeAndroidTv {
  getInstalledApps(): string;
  launchApp(packageName: string): boolean;
  openAppInfo(packageName: string): void;
  uninstallApp(packageName: string): void;
  isDefaultHomeLauncher(): boolean;
  openHomeLauncherSettings(): boolean;
  openSystemSettings(section: string): void;
  getSystemStatus(): string;
  getPreference(key: string, defaultValue: string): string;
  setPreference(key: string, value: string): void;
}

declare global {
  interface Window {
    AndroidTv?: NativeAndroidTv;
    __tvKeyDown?: (key: string) => void;
    __tvPackagesChanged?: () => void;
    __tvResume?: () => void;
  }
}

export const isNativeAndroid = (): boolean => typeof window !== 'undefined' && Boolean(window.AndroidTv);

function makeSvgBanner(title: string, c1: string, c2: string, glyph: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180" viewBox="0 0 320 180">
    <defs>
      <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0%" stop-color="${c1}"/>
        <stop offset="100%" stop-color="${c2}"/>
      </linearGradient>
    </defs>
    <rect width="320" height="180" fill="url(#g)"/>
    <circle cx="270" cy="36" r="68" fill="rgba(255,255,255,0.09)"/>
    <text x="26" y="76" font-family="system-ui,sans-serif" font-size="36" font-weight="800" fill="#fff">${glyph}</text>
    <text x="26" y="142" font-family="system-ui,sans-serif" font-size="24" font-weight="800" fill="#fff" letter-spacing="-0.5">${title}</text>
  </svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

const MOCK_APPS: TvAppInfo[] = [
  {
    packageName: 'com.google.android.youtube.tv',
    label: 'YouTube',
    category: 'streaming',
    isLeanback: true,
    hasBanner: true,
    isSystemApp: false,
    versionName: '4.40.303',
    bannerUrl: makeSvgBanner('YouTube', '#dc2626', '#7f1d1d', '▶'),
    iconUrl: makeSvgBanner('YT', '#dc2626', '#7f1d1d', '▶'),
  },
  {
    packageName: 'com.netflix.ninja',
    label: 'Netflix',
    category: 'streaming',
    isLeanback: true,
    hasBanner: true,
    isSystemApp: false,
    versionName: '11.2.1',
    bannerUrl: makeSvgBanner('Netflix', '#b91c1c', '#18181b', 'N'),
    iconUrl: makeSvgBanner('N', '#b91c1c', '#18181b', 'N'),
  },
  {
    packageName: 'com.bbc.iplayer.tv',
    label: 'BBC iPlayer',
    category: 'streaming',
    isLeanback: true,
    hasBanner: true,
    isSystemApp: false,
    versionName: '5.14.0',
    bannerUrl: makeSvgBanner('BBC iPlayer', '#db2777', '#4c0519', 'iP'),
    iconUrl: makeSvgBanner('iP', '#db2777', '#4c0519', 'iP'),
  },
  {
    packageName: 'com.disney.disneyplus',
    label: 'Disney+',
    category: 'streaming',
    isLeanback: true,
    hasBanner: true,
    isSystemApp: false,
    versionName: '3.9.2',
    bannerUrl: makeSvgBanner('Disney+', '#1d4ed8', '#0f172a', 'D+'),
    iconUrl: makeSvgBanner('D+', '#1d4ed8', '#0f172a', 'D+'),
  },
  {
    packageName: 'com.amazon.amazonvideo.livingroom',
    label: 'Prime Video',
    category: 'streaming',
    isLeanback: true,
    hasBanner: true,
    isSystemApp: false,
    versionName: '6.2.1',
    bannerUrl: makeSvgBanner('Prime Video', '#0284c7', '#0c4a6e', 'PV'),
    iconUrl: makeSvgBanner('PV', '#0284c7', '#0c4a6e', 'PV'),
  },
  {
    packageName: 'com.plexapp.android',
    label: 'Plex',
    category: 'streaming',
    isLeanback: true,
    hasBanner: true,
    isSystemApp: false,
    versionName: '10.22.0',
    bannerUrl: makeSvgBanner('Plex', '#d97706', '#1c1917', '❯'),
    iconUrl: makeSvgBanner('P', '#d97706', '#1c1917', '❯'),
  },
  {
    packageName: 'com.spotify.tv.android',
    label: 'Spotify',
    category: 'music',
    isLeanback: true,
    hasBanner: true,
    isSystemApp: false,
    versionName: '1.88.0',
    bannerUrl: makeSvgBanner('Spotify', '#16a34a', '#052e16', '♫'),
    iconUrl: makeSvgBanner('S', '#16a34a', '#052e16', '♫'),
  },
  {
    packageName: 'com.nordvpn.android',
    label: 'NordVPN',
    category: 'tools',
    isLeanback: true,
    hasBanner: true,
    isSystemApp: false,
    versionName: '8.4.0',
    bannerUrl: makeSvgBanner('NordVPN', '#2563eb', '#1e1b4b', '⛨'),
    iconUrl: makeSvgBanner('NV', '#2563eb', '#1e1b4b', '⛨'),
  },
  {
    packageName: 'com.valvesoftware.steamlink',
    label: 'Steam Link',
    category: 'games',
    isLeanback: true,
    hasBanner: true,
    isSystemApp: false,
    versionName: '1.3.14',
    bannerUrl: makeSvgBanner('Steam Link', '#3b82f6', '#0f172a', '◈'),
    iconUrl: makeSvgBanner('SL', '#3b82f6', '#0f172a', '◈'),
  },
  {
    packageName: 'org.videolan.vlc',
    label: 'VLC Media',
    category: 'streaming',
    isLeanback: true,
    hasBanner: true,
    isSystemApp: false,
    versionName: '3.6.0',
    bannerUrl: makeSvgBanner('VLC', '#ea580c', '#431407', '▲'),
    iconUrl: makeSvgBanner('VLC', '#ea580c', '#431407', '▲'),
  },
];

export const androidBridge = {
  getInstalledApps(): TvAppInfo[] {
    if (window.AndroidTv) {
      try {
        return JSON.parse(window.AndroidTv.getInstalledApps()) as TvAppInfo[];
      } catch {
        return [];
      }
    }
    return MOCK_APPS;
  },

  launchApp(packageName: string): boolean {
    if (window.AndroidTv) {
      return window.AndroidTv.launchApp(packageName);
    }
    console.info('[AstryxTV Mock] Launch app:', packageName);
    return true;
  },

  openAppInfo(packageName: string): void {
    if (window.AndroidTv) {
      window.AndroidTv.openAppInfo(packageName);
    } else {
      console.info('[AstryxTV Mock] Open App Info:', packageName);
    }
  },

  uninstallApp(packageName: string): void {
    if (window.AndroidTv) {
      window.AndroidTv.uninstallApp(packageName);
    } else {
      console.info('[AstryxTV Mock] Uninstall:', packageName);
    }
  },

  isDefaultHomeLauncher(): boolean {
    if (window.AndroidTv?.isDefaultHomeLauncher) {
      try {
        return window.AndroidTv.isDefaultHomeLauncher();
      } catch {
        return false;
      }
    }
    return false;
  },

  openHomeLauncherSettings(): boolean {
    if (window.AndroidTv?.openHomeLauncherSettings) {
      return window.AndroidTv.openHomeLauncherSettings();
    }
    console.info('[AstryxTV Mock] Open Default Home Launcher settings');
    return true;
  },

  openSystemSettings(section = 'general'): void {
    if (window.AndroidTv) {
      window.AndroidTv.openSystemSettings(section);
    } else {
      console.info('[AstryxTV Mock] Open Settings:', section);
    }
  },

  getSystemStatus(): TvSystemStatus {
    if (window.AndroidTv) {
      try {
        return JSON.parse(window.AndroidTv.getSystemStatus()) as TvSystemStatus;
      } catch {
        // fallback below
      }
    }
    return {
      networkType: 'wifi',
      isConnected: true,
      isVpnActive: true,
      deviceModel: 'TCL Smart TV Pro (Preview)',
      androidVersion: 'Android 12',
      isTv: true,
      ramFreeMb: 1420,
      ramTotalMb: 3072,
      storageFreeGb: 14.6,
      storageTotalGb: 32.0,
    };
  },

  getPreference<T>(key: string, fallback: T): T {
    try {
      const raw = window.AndroidTv
        ? window.AndroidTv.getPreference(key, '')
        : window.localStorage.getItem(`astryxtv:${key}`) ?? '';
      if (!raw) return fallback;
      return JSON.parse(raw) as T;
    } catch {
      return fallback;
    }
  },

  setPreference<T>(key: string, value: T): void {
    const raw = JSON.stringify(value);
    try {
      if (window.AndroidTv) {
        window.AndroidTv.setPreference(key, raw);
      } else {
        window.localStorage.setItem(`astryxtv:${key}`, raw);
      }
    } catch {
      // ignore quota errors
    }
  },
};
