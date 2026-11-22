import React, { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Theme } from '@astryxdesign/core/theme';
import { MessageProcessor, type SurfaceModel } from '@a2ui/web_core';
import { A2uiSurface } from '@a2ui/react';
import {
  androidBridge,
  type TvAppInfo,
  type TvSystemStatus,
} from './bridge/androidBridge';
import { astryxTvCatalog } from './catalog';
import { HOME_SURFACE_ID, buildLauncherMessages, formatUkClock } from './a2ui/launcherMessages';
import {
  fetchDetailedLondonWeather,
  getCachedWeather,
  type DetailedWeather,
} from './a2ui/weather';
import {
  ASTRYX_THEMES,
  DEFAULT_LAUNCHER_SETTINGS,
  DEFAULT_TV_THEME,
  applyLauncherSettingsCss,
  applyThemeCssVars,
  type LauncherSettings,
  type TvThemeConfig,
} from './theme/themes';
import { focusFirstInScope, installSpatialNavigation } from './navigation/spatialNav';
import { WeatherWallpaper } from './components/WeatherWallpaper';

const ThemeCustomizer = lazy(() =>
  import('./components/ThemeCustomizer').then((m) => ({ default: m.ThemeCustomizer })),
);
const A2uiStudioModal = lazy(() =>
  import('./components/A2uiStudioModal').then((m) => ({ default: m.A2uiStudioModal })),
);
const AppContextMenu = lazy(() =>
  import('./components/AppContextMenu').then((m) => ({ default: m.AppContextMenu })),
);
const LauncherSettingsModal = lazy(() =>
  import('./components/LauncherSettingsModal').then((m) => ({
    default: m.LauncherSettingsModal,
  })),
);
const SearchModal = lazy(() =>
  import('./components/SearchModal').then((m) => ({ default: m.SearchModal })),
);
const WeatherModal = lazy(() =>
  import('./components/WeatherModal').then((m) => ({ default: m.WeatherModal })),
);

export default function App() {
  const [apps, setApps] = useState<TvAppInfo[]>(() => androidBridge.getInstalledApps());
  const [status, setStatus] = useState<TvSystemStatus>(() => androidBridge.getSystemStatus());
  const [favorites, setFavorites] = useState<string[]>(() =>
    androidBridge.getPreference<string[]>('favorites', []),
  );
  const [recents, setRecents] = useState<string[]>(() =>
    androidBridge.getPreference<string[]>('recents', []),
  );
  const [themeConfig, setThemeConfig] = useState<TvThemeConfig>(() => ({
    ...DEFAULT_TV_THEME,
    ...androidBridge.getPreference<Partial<TvThemeConfig>>('theme_v2', {}),
  }));
  const [launcherSettings, setLauncherSettings] = useState<LauncherSettings>(() => ({
    ...DEFAULT_LAUNCHER_SETTINGS,
    ...androidBridge.getPreference<Partial<LauncherSettings>>('launcherSettings', {}),
  }));
  const [agentUrl, setAgentUrl] = useState<string>(() =>
    androidBridge.getPreference<string>('agentUrl', ''),
  );
  const [weather, setWeather] = useState<DetailedWeather | null>(() => getCachedWeather());
  const [isDefaultHome, setIsDefaultHome] = useState<boolean>(() =>
    androidBridge.isDefaultHomeLauncher(),
  );

  const [themeModalOpen, setThemeModalOpen] = useState(false);
  const [studioModalOpen, setStudioModalOpen] = useState(false);
  const [settingsModalOpen, setSettingsModalOpen] = useState(false);
  const [searchModalOpen, setSearchModalOpen] = useState(false);
  const [weatherModalOpen, setWeatherModalOpen] = useState(false);
  const [contextApp, setContextApp] = useState<TvAppInfo | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const appsRef = useRef(apps);
  appsRef.current = apps;
  const lastFocusRef = useRef<HTMLElement | null>(null);
  const anyModalOpen =
    themeModalOpen ||
    studioModalOpen ||
    settingsModalOpen ||
    searchModalOpen ||
    weatherModalOpen ||
    Boolean(contextApp);
  const prevModalOpenRef = useRef(false);

  // Restore TV focus to the element that opened a modal once the modal closes
  useEffect(() => {
    if (anyModalOpen && !prevModalOpenRef.current) {
      const active = document.activeElement as HTMLElement | null;
      if (active && active !== document.body && !active.closest('[data-tv-modal="true"]')) {
        lastFocusRef.current = active;
      }
    } else if (!anyModalOpen && prevModalOpenRef.current) {
      const target = lastFocusRef.current;
      window.setTimeout(() => {
        if (target && document.contains(target)) {
          target.focus({ preventScroll: true });
        } else {
          focusFirstInScope();
        }
      }, 30);
    }
    prevModalOpenRef.current = anyModalOpen;
  }, [anyModalOpen]);

  const showToast = useCallback((message: string) => {
    setToast(message);
  }, []);

  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => setToast(null), 2400);
    return () => window.clearTimeout(id);
  }, [toast]);

  // Apply CSS custom properties whenever theme config changes
  useEffect(() => {
    applyThemeCssVars(themeConfig);
    androidBridge.setPreference('theme_v2', themeConfig);
  }, [themeConfig]);

  // Apply & persist launcher settings
  useEffect(() => {
    applyLauncherSettingsCss(launcherSettings);
    androidBridge.setPreference('launcherSettings', launcherSettings);
  }, [launcherSettings]);

  // Persist favorites & recents
  useEffect(() => {
    androidBridge.setPreference('favorites', favorites);
  }, [favorites]);

  useEffect(() => {
    androidBridge.setPreference('recents', recents);
  }, [recents]);

  // Fetch & auto-refresh detailed realtime London weather every 10 minutes
  useEffect(() => {
    if (!launcherSettings.showWeather) {
      setWeather(null);
      return;
    }
    let active = true;
    const syncWeather = (force = false) => {
      void fetchDetailedLondonWeather(force).then((w) => {
        if (active && w) setWeather(w);
      });
    };
    syncWeather(false);
    const id = window.setInterval(() => syncWeather(true), 10 * 60 * 1000);
    return () => {
      active = false;
      window.clearInterval(id);
    };
  }, [launcherSettings.showWeather]);

  const recordAndLaunch = useCallback(
    (pkg: string) => {
      const target = appsRef.current.find((a) => a.packageName === pkg);
      setRecents((prev) => [pkg, ...prev.filter((p) => p !== pkg)].slice(0, 12));
      const ok = androidBridge.launchApp(pkg);
      showToast(ok ? `Launching ${target?.label ?? pkg}…` : `Could not launch ${pkg}`);
    },
    [showToast],
  );

  const refreshSystemAndApps = useCallback(() => {
    setApps(androidBridge.getInstalledApps());
    setStatus(androidBridge.getSystemStatus());
  }, []);

  // Global A2UI Action Handler
  const handleA2uiAction = useCallback(
    (action: any) => {
      const name: string = action?.name ?? action?.event?.name ?? '';
      const ctx: Record<string, any> = action?.context ?? action?.event?.context ?? {};

      switch (name) {
        case 'launch_app': {
          const pkg = String(ctx.packageName ?? '');
          if (pkg) recordAndLaunch(pkg);
          break;
        }
        case 'open_app_menu': {
          const pkg = String(ctx.packageName ?? '');
          const found = appsRef.current.find((a) => a.packageName === pkg);
          if (found) setContextApp(found);
          break;
        }
        case 'open_search':
          setSearchModalOpen(true);
          break;
        case 'open_weather':
          setWeatherModalOpen(true);
          break;
        case 'open_vpn': {
          const vpnApp =
            appsRef.current.find((a) => a.packageName.toLowerCase().includes('nordvpn')) ??
            appsRef.current.find(
              (a) =>
                a.packageName.toLowerCase().includes('vpn') ||
                a.label.toLowerCase().includes('vpn'),
            );
          if (vpnApp) {
            recordAndLaunch(vpnApp.packageName);
          } else {
            androidBridge.openSystemSettings('network');
            showToast('Opening Network & VPN Settings…');
          }
          break;
        }
        case 'open_theme_customizer':
          setThemeModalOpen(true);
          break;
        case 'open_a2ui_studio':
          setStudioModalOpen(true);
          break;
        case 'open_launcher_settings':
          setStatus(androidBridge.getSystemStatus());
          setIsDefaultHome(androidBridge.isDefaultHomeLauncher());
          setSettingsModalOpen(true);
          break;
        case 'open_settings':
          androidBridge.openSystemSettings(String(ctx.section ?? 'general'));
          showToast('Opening Android TV Settings…');
          break;
        case 'refresh_apps':
          refreshSystemAndApps();
          showToast('Refreshed installed apps');
          break;
        default:
          if (name) {
            showToast(`A2UI Action: ${name}`);
          }
      }
    },
    [recordAndLaunch, refreshSystemAndApps, showToast],
  );

  const actionHandlerRef = useRef(handleA2uiAction);
  actionHandlerRef.current = handleA2uiAction;

  // Create single A2UI MessageProcessor with our Astryx TV Catalog
  const processor = useMemo(
    () =>
      new MessageProcessor([astryxTvCatalog], (action: any) => {
        actionHandlerRef.current(action);
      }),
    [],
  );

  const [surfaces, setSurfaces] = useState<SurfaceModel<any>[]>(() =>
    Array.from(processor.model.surfacesMap.values()),
  );

  useEffect(() => {
    const sync = () => setSurfaces(Array.from(processor.model.surfacesMap.values()));
    const createdSub = processor.onSurfaceCreated(sync);
    const deletedSub = processor.onSurfaceDeleted(sync);
    return () => {
      createdSub.unsubscribe();
      deletedSub.unsubscribe();
    };
  }, [processor]);

  const weatherRef = useRef(weather);
  weatherRef.current = weather;
  const statusRef = useRef(status);
  statusRef.current = status;
  const themeRef = useRef(themeConfig);
  themeRef.current = themeConfig;

  // Structural fingerprint of launcherSettings (excludes pure-CSS iconSize so changing icon size never rebuilds A2UI)
  const structuralSettingsKey = useMemo(
    () =>
      JSON.stringify({
        showHero: launcherSettings.showHero,
        showLabels: launcherSettings.showLabels,
        showWeather: launcherSettings.showWeather,
        sortBy: launcherSettings.sortBy,
        clock24h: launcherSettings.clock24h,
        showMobileApps: launcherSettings.showMobileApps,
        showSystemApps: launcherSettings.showSystemApps,
        hiddenApps: launcherSettings.hiddenApps,
      }),
    [launcherSettings],
  );
  const settingsRef = useRef(launcherSettings);
  settingsRef.current = launcherSettings;

  const lastBuiltSigRef = useRef<string>('');

  // Rebuild main launcher A2UI surface ONLY when structural inputs actually changed AND Settings modal is closed
  useEffect(() => {
    if (settingsModalOpen) return;
    const sig = `${apps.length}:${favorites.join(',')}:${recents.join(',')}:${structuralSettingsKey}`;
    if (sig === lastBuiltSigRef.current && processor.model.surfacesMap.has(HOME_SURFACE_ID)) {
      return;
    }
    lastBuiltSigRef.current = sig;
    try {
      const messages = buildLauncherMessages({
        apps,
        favorites,
        recents,
        weather: weatherRef.current,
        status: statusRef.current,
        themeConfig: themeRef.current,
        launcherSettings: settingsRef.current,
        now: new Date(),
      });
      if (processor.model.surfacesMap.has(HOME_SURFACE_ID)) {
        processor.processMessages([
          { version: '1.0', deleteSurface: { surfaceId: HOME_SURFACE_ID } },
        ]);
      }
      processor.processMessages(messages);
      setSurfaces(Array.from(processor.model.surfacesMap.values()));
    } catch (err) {
      console.error('[AstryxTV] Failed to build A2UI surface:', err);
      showToast(err instanceof Error ? err.message : 'A2UI surface error');
    }
  }, [apps, favorites, recents, structuralSettingsKey, settingsModalOpen, processor, showToast]);

  // Fine-grained A2UI /system data-model update (zero home screen flash/reset when opening Settings or resuming)
  useEffect(() => {
    if (!processor.model.surfacesMap.has(HOME_SURFACE_ID)) return;
    try {
      processor.processMessages([
        {
          version: '1.0',
          updateDataModel: {
            surfaceId: HOME_SURFACE_ID,
            path: '/system',
            value: {
              deviceModel: `${status.deviceModel} · ${status.androidVersion}`,
              networkType: status.networkType,
              isConnected: status.isConnected,
              isVpnActive: status.isVpnActive,
            },
          },
        },
      ]);
    } catch {
      // ignore transient updates
    }
  }, [status, processor]);

  // Fine-grained A2UI /weather data-model update (zero surface rebuild when weather refreshes)
  useEffect(() => {
    if (!weather || !processor.model.surfacesMap.has(HOME_SURFACE_ID)) return;
    try {
      processor.processMessages([
        {
          version: '1.0',
          updateDataModel: {
            surfaceId: HOME_SURFACE_ID,
            path: '/weather',
            value: {
              tempText: `${weather.tempC}°C`,
              conditionText: weather.condition,
              iconKey: weather.iconKey,
              feelsLikeText: `Feels ${weather.feelsLikeC}°`,
              highLowText: `H:${weather.highC}° L:${weather.lowC}°`,
              humidityText: `${weather.humidity}%`,
              windText: `${weather.windKmh} km/h`,
              sunText: `${weather.sunrise} / ${weather.sunset}`,
            },
          },
        },
      ]);
    } catch {
      // ignore transient updates
    }
  }, [weather, processor]);

  // Lightweight 15-second UK clock sync via A2UI updateDataModel (seconds tick locally in TvClockWeatherImpl)
  useEffect(() => {
    const tick = () => {
      if (document.hidden || !processor.model.surfacesMap.has(HOME_SURFACE_ID)) return;
      const clock = formatUkClock(new Date(), launcherSettings.clock24h);
      try {
        processor.processMessages([
          {
            version: '1.0',
            updateDataModel: {
              surfaceId: HOME_SURFACE_ID,
              path: '/clock',
              value: {
                time: clock.timeText,
                timeMain: clock.timeMain,
                timeSeconds: clock.timeSeconds,
                timePeriod: clock.timePeriod,
                date: clock.dateText,
                fullDate: clock.fullDate,
                tzLabel: clock.tzLabel,
              },
            },
          },
        ]);
      } catch (err) {
        console.error('[AstryxTV] Clock tick failed:', err);
      }
    };
    const interval = window.setInterval(tick, 15_000);
    return () => window.clearInterval(interval);
  }, [processor, launcherSettings.clock24h]);

  // Native lifecycle hooks from MainActivity.kt
  useEffect(() => {
    window.__tvPackagesChanged = () => refreshSystemAndApps();
    window.__tvResume = () => {
      setStatus(androidBridge.getSystemStatus());
      setIsDefaultHome(androidBridge.isDefaultHomeLauncher());
    };
    return () => {
      delete window.__tvPackagesChanged;
      delete window.__tvResume;
    };
  }, [refreshSystemAndApps]);

  // D-pad & Keyboard Spatial Navigation
  useEffect(() => {
    const cleanup = installSpatialNavigation({
      onBack: () => {
        if (contextApp) {
          setContextApp(null);
        } else if (weatherModalOpen) {
          setWeatherModalOpen(false);
        } else if (searchModalOpen) {
          setSearchModalOpen(false);
        } else if (settingsModalOpen) {
          setSettingsModalOpen(false);
        } else if (themeModalOpen) {
          setThemeModalOpen(false);
        } else if (studioModalOpen) {
          setStudioModalOpen(false);
        } else {
          focusFirstInScope();
        }
      },
      onMenu: () => {
        if (
          !themeModalOpen &&
          !studioModalOpen &&
          !settingsModalOpen &&
          !searchModalOpen &&
          !weatherModalOpen &&
          !contextApp
        ) {
          setSettingsModalOpen(true);
        }
      },
    });
    return cleanup;
  }, [
    contextApp,
    weatherModalOpen,
    searchModalOpen,
    settingsModalOpen,
    themeModalOpen,
    studioModalOpen,
  ]);

  // Focus first tile once surfaces mount
  useEffect(() => {
    const t = window.setTimeout(() => {
      if (!document.activeElement || document.activeElement === document.body) {
        focusFirstInScope();
      }
    }, 120);
    return () => window.clearTimeout(t);
  }, [surfaces.length]);

  const handleApplyExternalMessages = useCallback(
    (messages: any[]) => {
      const toDelete: any[] = [];
      for (const m of messages) {
        const sid = m?.createSurface?.surfaceId;
        if (sid && processor.model.surfacesMap.has(sid)) {
          toDelete.push({ version: '1.0', deleteSurface: { surfaceId: sid } });
        }
      }
      if (toDelete.length > 0) {
        processor.processMessages(toDelete);
      }
      processor.processMessages(messages);
      setSurfaces(Array.from(processor.model.surfacesMap.values()));
    },
    [processor],
  );

  const handleClearCustomSurfaces = useCallback(() => {
    const customIds = Array.from(processor.model.surfacesMap.keys()).filter(
      (id) => id !== HOME_SURFACE_ID,
    );
    if (customIds.length === 0) return;
    processor.processMessages(
      customIds.map((surfaceId) => ({
        version: '1.0',
        deleteSurface: { surfaceId },
      })),
    );
    setSurfaces(Array.from(processor.model.surfacesMap.values()));
  }, [processor]);

  const toggleFavorite = useCallback(
    (pkg: string) => {
      setFavorites((prev) => {
        const exists = prev.includes(pkg);
        const next = exists ? prev.filter((p) => p !== pkg) : [pkg, ...prev];
        showToast(exists ? 'Removed from Favorites' : 'Pinned to Favorites');
        return next;
      });
    },
    [showToast],
  );

  const moveFavorite = useCallback(
    (pkg: string, delta: -1 | 1) => {
      setFavorites((prev) => {
        const idx = prev.indexOf(pkg);
        if (idx < 0) return prev;
        const target = idx + delta;
        if (target < 0 || target >= prev.length) return prev;
        const next = [...prev];
        const [item] = next.splice(idx, 1);
        next.splice(target, 0, item);
        return next;
      });
      showToast(delta < 0 ? 'Moved favorite left' : 'Moved favorite right');
    },
    [showToast],
  );

  const exportBackup = useCallback(() => {
    return JSON.stringify(
      {
        version: 1,
        favorites,
        themeConfig,
        launcherSettings,
        agentUrl,
      },
      null,
      2,
    );
  }, [favorites, themeConfig, launcherSettings, agentUrl]);

  const importBackup = useCallback(
    (raw: string) => {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed.favorites)) setFavorites(parsed.favorites);
      if (parsed.themeConfig && typeof parsed.themeConfig === 'object') {
        setThemeConfig((prev) => ({ ...prev, ...parsed.themeConfig }));
      }
      if (parsed.launcherSettings && typeof parsed.launcherSettings === 'object') {
        setLauncherSettings((prev) => ({ ...prev, ...parsed.launcherSettings }));
      }
      if (typeof parsed.agentUrl === 'string') {
        setAgentUrl(parsed.agentUrl);
        androidBridge.setPreference('agentUrl', parsed.agentUrl);
      }
      showToast('Launcher backup restored');
    },
    [showToast],
  );

  const activeAstryxTheme =
    ASTRYX_THEMES[themeConfig.astryxTheme]?.theme ?? ASTRYX_THEMES.gothic.theme;

  const orderedSurfaces = useMemo(() => {
    return [...surfaces].sort((a, b) => {
      if (a.id === HOME_SURFACE_ID) return -1;
      if (b.id === HOME_SURFACE_ID) return 1;
      return a.id.localeCompare(b.id);
    });
  }, [surfaces]);

  const contextFavIndex = contextApp ? favorites.indexOf(contextApp.packageName) : -1;

  const showLiveWeatherBg =
    themeConfig.wallpaper === 'live-weather' && !(themeConfig.customWallpaperUrl ?? '').trim();

  const visibleSearchApps = useMemo(() => {
    const hiddenSet = new Set(launcherSettings.hiddenApps);
    return apps.filter((a) => {
      if (hiddenSet.has(a.packageName)) return false;
      if (!launcherSettings.showMobileApps && !a.isLeanback) return false;
      if (!launcherSettings.showSystemApps && a.isSystemApp && !a.isLeanback) return false;
      return true;
    });
  }, [apps, launcherSettings]);

  return (
    <Theme theme={activeAstryxTheme} mode="dark">
      <div className="tv-viewport">
        {showLiveWeatherBg ? (
          <WeatherWallpaper
            weather={weather}
            sceneOverride={themeConfig.weatherScene ?? 'auto'}
          />
        ) : (
          <div className="tv-ambient-layer" aria-hidden="true" />
        )}
        <main className="tv-scroll-root">
          {orderedSurfaces.map((surface) => (
            <div key={surface.id} className="tv-surface-wrapper" data-surface-id={surface.id}>
              <A2uiSurface surface={surface} />
            </div>
          ))}
        </main>

        <Suspense fallback={null}>
        {weatherModalOpen && (
          <WeatherModal
            weather={weather}
            onRefresh={async () => {
              const fresh = await fetchDetailedLondonWeather(true);
              if (fresh) {
                setWeather(fresh);
                showToast(`Weather updated (${fresh.tempC}°C ${fresh.condition})`);
              }
            }}
            onClose={() => setWeatherModalOpen(false)}
          />
        )}

        {searchModalOpen && (
          <SearchModal
            apps={visibleSearchApps}
            onLaunch={(app) => {
              setSearchModalOpen(false);
              recordAndLaunch(app.packageName);
            }}
            onClose={() => setSearchModalOpen(false)}
          />
        )}

        {themeModalOpen && (
          <ThemeCustomizer
            config={themeConfig}
            onChange={setThemeConfig}
            onClose={() => setThemeModalOpen(false)}
          />
        )}

        {studioModalOpen && (
          <A2uiStudioModal
            activeSurfaceIds={orderedSurfaces.map((s) => s.id)}
            agentUrl={agentUrl}
            onSaveAgentUrl={(url) => {
              setAgentUrl(url);
              androidBridge.setPreference('agentUrl', url);
            }}
            onApplyMessages={handleApplyExternalMessages}
            onClearCustomSurfaces={handleClearCustomSurfaces}
            onClose={() => setStudioModalOpen(false)}
          />
        )}

        {settingsModalOpen && (
          <LauncherSettingsModal
            settings={launcherSettings}
            status={status}
            isDefaultHome={isDefaultHome}
            favoritesCount={favorites.length}
            onChange={setLauncherSettings}
            onSetDefaultHome={() => {
              const ok = androidBridge.openHomeLauncherSettings();
              showToast(
                ok
                  ? 'Choose Astryx TV as your default Home app'
                  : 'Open TV Settings → Apps → Default Apps',
              );
            }}
            onClearFavorites={() => {
              setFavorites([]);
              showToast('Cleared pinned favorites');
            }}
            onExportBackup={exportBackup}
            onImportBackup={importBackup}
            onOpenTheme={() => {
              setSettingsModalOpen(false);
              setThemeModalOpen(true);
            }}
            onOpenStudio={() => {
              setSettingsModalOpen(false);
              setStudioModalOpen(true);
            }}
            onOpenAndroidSettings={(section) => {
              setSettingsModalOpen(false);
              androidBridge.openSystemSettings(section);
              showToast('Opening Android TV Settings…');
            }}
            onClose={() => setSettingsModalOpen(false)}
          />
        )}

        {contextApp && (
          <AppContextMenu
            app={contextApp}
            isPinned={contextFavIndex >= 0}
            canMoveLeft={contextFavIndex > 0}
            canMoveRight={contextFavIndex >= 0 && contextFavIndex < favorites.length - 1}
            onLaunch={() => {
              const pkg = contextApp.packageName;
              setContextApp(null);
              recordAndLaunch(pkg);
            }}
            onTogglePin={() => {
              toggleFavorite(contextApp.packageName);
              setContextApp(null);
            }}
            onMoveFavorite={(delta) => {
              moveFavorite(contextApp.packageName, delta);
              setContextApp(null);
            }}
            onHideApp={() => {
              const pkg = contextApp.packageName;
              const label = contextApp.label;
              setLauncherSettings((prev) => ({
                ...prev,
                hiddenApps: prev.hiddenApps.includes(pkg)
                  ? prev.hiddenApps
                  : [...prev.hiddenApps, pkg],
              }));
              setContextApp(null);
              showToast(`Hidden ${label} (restore in Settings)`);
            }}
            onOpenInfo={() => {
              const pkg = contextApp.packageName;
              setContextApp(null);
              androidBridge.openAppInfo(pkg);
            }}
            onUninstall={() => {
              const pkg = contextApp.packageName;
              setContextApp(null);
              androidBridge.uninstallApp(pkg);
            }}
            onClose={() => setContextApp(null)}
          />
        )}
        </Suspense>

        {toast && (
          <div className="tv-toast glass-panel-strong" role="status" aria-live="polite">
            {toast}
          </div>
        )}
      </div>
    </Theme>
  );
}
