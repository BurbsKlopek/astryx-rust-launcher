import type { TvAppInfo, TvSystemStatus } from '../bridge/androidBridge';
import { ASTRYX_TV_CATALOG_ID } from '../catalog';
import type { LauncherSettings, TvThemeConfig } from '../theme/themes';
import type { DetailedWeather } from './weather';

export const HOME_SURFACE_ID = 'tv_home';

export interface LauncherMessageInput {
  apps: TvAppInfo[];
  favorites: string[];
  recents: string[];
  weather: DetailedWeather | null;
  status: TvSystemStatus;
  themeConfig: TvThemeConfig;
  launcherSettings: LauncherSettings;
  now: Date;
}

export function formatUkClock(
  now: Date,
  clock24h = true,
): {
  timeText: string;
  timeMain: string;
  timeSeconds: string;
  timePeriod: string;
  dateText: string;
  fullDate: string;
  tzLabel: string;
  greeting: string;
} {
  const parts = new Intl.DateTimeFormat('en-GB', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: !clock24h,
    timeZone: 'Europe/London',
    timeZoneName: 'short',
  }).formatToParts(now);

  const getPart = (type: string) => parts.find((p) => p.type === type)?.value ?? '';
  const hour = getPart('hour');
  const minute = getPart('minute');
  const second = getPart('second');
  const period = clock24h ? '' : getPart('dayPeriod').toUpperCase();
  const tzName = getPart('timeZoneName') || 'GMT';

  const timeMain = `${hour}:${minute}`;
  const timeText = period ? `${timeMain} ${period}` : timeMain;

  const dateText = new Intl.DateTimeFormat('en-GB', {
    weekday: 'short',
    day: 'numeric',
    month: 'short',
    timeZone: 'Europe/London',
  }).format(now);

  const fullDate = new Intl.DateTimeFormat('en-GB', {
    weekday: 'long',
    day: 'numeric',
    month: 'long',
    year: 'numeric',
    timeZone: 'Europe/London',
  }).format(now);

  const hour24 = Number(
    new Intl.DateTimeFormat('en-GB', {
      hour: '2-digit',
      hour12: false,
      timeZone: 'Europe/London',
    }).format(now),
  );

  const greeting =
    hour24 < 12 ? 'Good morning' : hour24 < 18 ? 'Good afternoon' : 'Good evening';

  return {
    timeText,
    timeMain,
    timeSeconds: second,
    timePeriod: period,
    dateText,
    fullDate,
    tzLabel: `London · ${tzName}`,
    greeting,
  };
}

function toValidId(raw: string): string {
  const cleaned = raw.replace(/[^a-zA-Z0-9_]/g, '_');
  return /^[a-zA-Z_]/.test(cleaned) ? cleaned : `id_${cleaned}`;
}

export function buildLauncherMessages(input: LauncherMessageInput): any[] {
  const {
    apps: rawApps,
    favorites,
    recents,
    weather,
    status,
    launcherSettings,
    now,
  } = input;
  const clock = formatUkClock(now, launcherSettings.clock24h);

  const hiddenSet = new Set(launcherSettings.hiddenApps);
  const recentOrder = new Map(recents.map((pkg, idx) => [pkg, idx]));
  const apps = rawApps
    .filter((a) => {
      if (hiddenSet.has(a.packageName)) return false;
      if (!launcherSettings.showMobileApps && !a.isLeanback) return false;
      if (!launcherSettings.showSystemApps && a.isSystemApp && !a.isLeanback) return false;
      return true;
    })
    .sort((a, b) => {
      if (launcherSettings.sortBy === 'recent') {
        const ra = recentOrder.get(a.packageName) ?? 9999;
        const rb = recentOrder.get(b.packageName) ?? 9999;
        if (ra !== rb) return ra - rb;
      }
      return a.label.localeCompare(b.label);
    });

  const favSet = new Set(favorites);
  const pinnedApps = favorites
    .map((pkg) => apps.find((a) => a.packageName === pkg))
    .filter((a): a is TvAppInfo => Boolean(a));

  const recentApps = recents
    .map((pkg) => apps.find((a) => a.packageName === pkg))
    .filter((a): a is TvAppInfo => Boolean(a))
    .slice(0, 8);

  const components: any[] = [];
  const rootChildren: string[] = ['status_bar'];

  // 1. Status Bar (only shows compact clock if main Clock & Weather Header is hidden)
  components.push({
    id: 'status_bar',
    component: 'TvStatusBar',
    brandTitle: 'Astryx TV',
    deviceLabel: { '@path': '/system/deviceModel' },
    ...(!launcherSettings.showHero
      ? {
          timeText: { '@path': '/clock/time' },
          dateText: { '@path': '/clock/date' },
        }
      : {}),
    networkType: { '@path': '/system/networkType' },
    isConnected: { '@path': '/system/isConnected' },
    isVpnActive: { '@path': '/system/isVpnActive' },
    onOpenSearch: { event: { name: 'open_search', context: {} } },
    onOpenNetwork: { event: { name: 'open_settings', context: { section: 'network' } } },
    onOpenVpn: { event: { name: 'open_vpn', context: {} } },
    onOpenTheme: { event: { name: 'open_theme_customizer', context: {} } },
    onOpenStudio: { event: { name: 'open_a2ui_studio', context: {} } },
    onOpenSettings: { event: { name: 'open_launcher_settings', context: {} } },
    onRefresh: { event: { name: 'refresh_apps', context: {} } },
  });

  // 2. Realtime Detailed Clock, Date & Live Weather Band (no heavy box)
  if (launcherSettings.showHero) {
    components.push({
      id: 'clock_weather_band',
      component: 'TvClockWeather',
      timeMain: { '@path': '/clock/timeMain' },
      timeSeconds: { '@path': '/clock/timeSeconds' },
      timePeriod: { '@path': '/clock/timePeriod' },
      clock24h: launcherSettings.clock24h,
      fullDate: { '@path': '/clock/fullDate' },
      locationLabel: { '@path': '/clock/tzLabel' },
      showWeather: launcherSettings.showWeather,
      tempText: { '@path': '/weather/tempText' },
      conditionText: { '@path': '/weather/conditionText' },
      iconKey: { '@path': '/weather/iconKey' },
      feelsLikeText: { '@path': '/weather/feelsLikeText' },
      highLowText: { '@path': '/weather/highLowText' },
      humidityText: { '@path': '/weather/humidityText' },
      windText: { '@path': '/weather/windText' },
      sunText: { '@path': '/weather/sunText' },
      onOpenWeather: { event: { name: 'open_weather', context: {} } },
    });
    rootChildren.push('clock_weather_band');
  }

  // Helper to add an app shelf (UAX #31 compliant IDs)
  const addAppShelf = (
    shelfId: string,
    title: string,
    list: TvAppInfo[],
    layout: 'row' | 'grid' = 'row',
  ) => {
    if (list.length === 0) return;
    const tileIds: string[] = [];
    list.forEach((app, index) => {
      const tileId = toValidId(`${shelfId}__${index}_${app.packageName}`);
      tileIds.push(tileId);
      components.push({
        id: tileId,
        component: 'TvAppTile',
        packageName: app.packageName,
        label: app.label,
        category: app.category,
        bannerUrl: app.bannerUrl,
        iconUrl: app.iconUrl,
        hasBanner: app.hasBanner,
        isPinned: favSet.has(app.packageName),
        showLabel: launcherSettings.showLabels,
        onLaunch: {
          event: { name: 'launch_app', context: { packageName: app.packageName } },
        },
        onContextMenu: {
          event: { name: 'open_app_menu', context: { packageName: app.packageName } },
        },
      });
    });
    components.push({
      id: shelfId,
      component: 'TvShelf',
      title,
      countText: `${list.length}`,
      layout,
      children: tileIds,
    });
    rootChildren.push(shelfId);
  };

  // 3. Pinned Favorites shelf (shown when user pins apps)
  if (pinnedApps.length > 0) {
    addAppShelf('shelf_favorites', 'Pinned Favorites', pinnedApps, 'row');
  }

  // 4. All Apps Grid (no categories)
  addAppShelf('shelf_all_apps', 'All Apps', apps, 'grid');

  // Root layout
  components.push({
    id: 'root',
    component: 'TvLayout',
    children: rootChildren,
  });

  return [
    {
      version: '1.0',
      createSurface: {
        surfaceId: HOME_SURFACE_ID,
        catalogId: ASTRYX_TV_CATALOG_ID,
      },
    },
    {
      version: '1.0',
      updateComponents: {
        surfaceId: HOME_SURFACE_ID,
        components,
      },
    },
    {
      version: '1.0',
      updateDataModel: {
        surfaceId: HOME_SURFACE_ID,
        path: '/',
        value: {
          clock: {
            time: clock.timeText,
            timeMain: clock.timeMain,
            timeSeconds: clock.timeSeconds,
            timePeriod: clock.timePeriod,
            date: clock.dateText,
            fullDate: clock.fullDate,
            tzLabel: clock.tzLabel,
          },
          system: {
            deviceModel: `${status.deviceModel} · ${status.androidVersion}`,
            networkType: status.networkType,
            isConnected: status.isConnected,
            isVpnActive: status.isVpnActive,
          },
          weather: weather
            ? {
                tempText: `${weather.tempC}°C`,
                conditionText: weather.condition,
                iconKey: weather.iconKey,
                feelsLikeText: `Feels ${weather.feelsLikeC}°`,
                highLowText: `H:${weather.highC}° L:${weather.lowC}°`,
                humidityText: `${weather.humidity}%`,
                windText: `${weather.windKmh} km/h`,
                sunText: `${weather.sunrise} / ${weather.sunset}`,
              }
            : {
                tempText: '--°C',
                conditionText: 'Updating weather…',
                iconKey: 'partly',
                feelsLikeText: 'London UK',
                highLowText: '',
                humidityText: '',
                windText: '',
                sunText: '',
              },
        },
      },
    },
  ];
}

export const SAMPLE_AGENT_SURFACE_JSON = JSON.stringify(
  [
    {
      version: '1.0',
      createSurface: {
        surfaceId: 'agent_tonight',
        catalogId: ASTRYX_TV_CATALOG_ID,
      },
    },
    {
      version: '1.0',
      updateComponents: {
        surfaceId: 'agent_tonight',
        components: [
          {
            id: 'root',
            component: 'TvShelf',
            title: 'Agent Surface · Tonight on TV',
            countText: 'A2UI',
            children: ['card_weather', 'card_pick', 'card_focus'],
          },
          {
            id: 'card_weather',
            component: 'TvWidgetCard',
            title: 'London Weather',
            value: '11°C · Clear',
            subtitle: 'Light breeze · Sunset 16:04 GMT',
            badge: 'Agent Card',
            icon: 'clock',
            accent: '#38bdf8',
          },
          {
            id: 'card_pick',
            component: 'TvWidgetCard',
            title: '4K Cinema Mode',
            value: '2160p Ready',
            subtitle: 'Tap to open Display & HDR settings',
            badge: 'Actionable',
            icon: 'tv',
            accent: '#a855f7',
            onAction: { event: { name: 'open_settings', context: { section: 'display' } } },
          },
          {
            id: 'card_focus',
            component: 'TvWidgetCard',
            title: 'Ambient Glass',
            value: 'Customise',
            subtitle: 'Switch Astryx themes, blur & gradients',
            badge: 'Theme',
            icon: 'zap',
            accent: '#f43f5e',
            onAction: { event: { name: 'open_theme_customizer', context: {} } },
          },
        ],
      },
    },
  ],
  null,
  2,
);
