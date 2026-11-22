import React, { useEffect, useMemo, useRef, useState } from 'react';
import { createComponentImplementation } from '@a2ui/react';
import { Badge, Button, Card, Divider, Heading, Text } from '@astryxdesign/core';
import {
  Clock,
  Cloud,
  CloudDrizzle,
  CloudFog,
  CloudLightning,
  CloudRain,
  CloudSnow,
  CloudSun,
  Cpu,
  Droplets,
  EthernetPort,
  Palette,
  Pin,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  Sun,
  Sunrise,
  Thermometer,
  Tv,
  Wifi,
  WifiOff,
  Wind,
  Zap,
} from 'lucide-react';
import {
  BadgeApi,
  ButtonApi,
  CardApi,
  ColumnApi,
  DividerApi,
  RowApi,
  TextApi,
  TvAppTileApi,
  TvClockWeatherApi,
  TvHeroApi,
  TvLayoutApi,
  TvShelfApi,
  TvStatusBarApi,
  TvWidgetCardApi,
} from './schemas';

type ChildRef = string | { id: string; basePath?: string };

function renderChildren(
  children: ReadonlyArray<ChildRef> | undefined,
  buildChild: (id: string, basePath?: string) => React.ReactNode,
): React.ReactNode {
  if (!Array.isArray(children)) return null;
  return children.map((child, idx) => {
    if (typeof child === 'string') {
      return <React.Fragment key={child}>{buildChild(child)}</React.Fragment>;
    }
    return (
      <React.Fragment key={`${child.id}:${child.basePath ?? idx}`}>
        {buildChild(child.id, child.basePath)}
      </React.Fragment>
    );
  });
}

function renderWidgetIcon(name?: string) {
  switch ((name ?? '').toLowerCase()) {
    case 'shield':
    case 'vpn':
      return <ShieldCheck size={22} />;
    case 'clock':
    case 'time':
      return <Clock size={22} />;
    case 'tv':
    case 'display':
      return <Tv size={22} />;
    case 'cpu':
    case 'system':
      return <Cpu size={22} />;
    case 'zap':
    case 'speed':
      return <Zap size={22} />;
    default:
      return <Sparkles size={22} />;
  }
}

export const TvLayoutImpl = createComponentImplementation(
  TvLayoutApi,
  ({ props, buildChild }) => {
    return <div className="tv-layout">{renderChildren(props.children as ChildRef[], buildChild)}</div>;
  },
);

export const TvStatusBarImpl = createComponentImplementation(
  TvStatusBarApi,
  ({ props }) => {
    const isConnected = props.isConnected ?? true;
    const isVpn = props.isVpnActive ?? false;
    const netType = props.networkType ?? 'wifi';

    return (
      <header className="tv-status-bar">
        <div className="tv-status-actions tv-status-dock glass-panel">
          <div className="tv-status-pills">
            <button
              type="button"
              data-tv-focusable="true"
              className={`tv-top-icon-btn tv-pill-interactive ${isConnected ? 'tv-pill-ok' : 'tv-pill-warn'}`}
              onClick={() => void props.onOpenNetwork?.()}
              title={isConnected ? (netType === 'ethernet' ? 'Ethernet Connected' : 'Wi-Fi Connected') : 'Offline'}
              aria-label="Network & Wi-Fi Settings"
            >
              {!isConnected ? (
                <WifiOff size={16} />
              ) : netType === 'ethernet' ? (
                <EthernetPort size={16} />
              ) : (
                <Wifi size={16} />
              )}
            </button>

            <button
              type="button"
              data-tv-focusable="true"
              className={`tv-top-icon-btn tv-pill-interactive ${isVpn ? 'tv-pill-vpn' : 'tv-pill-vpn-off'}`}
              onClick={() => void props.onOpenVpn?.()}
              title={isVpn ? 'VPN Active' : 'VPN Off — Open VPN'}
              aria-label="VPN"
            >
              <ShieldCheck size={16} />
            </button>
          </div>

          <div className="tv-quick-btns">
            {props.onOpenSearch && (
              <button
                type="button"
                className="tv-top-icon-btn glass-btn"
                data-tv-focusable="true"
                onClick={() => void props.onOpenSearch?.()}
                title="Search Apps"
                aria-label="Search Apps"
              >
                <Search size={16} />
              </button>
            )}
            {props.onOpenTheme && (
              <button
                type="button"
                className="tv-top-icon-btn glass-btn"
                data-tv-focusable="true"
                onClick={() => void props.onOpenTheme?.()}
                title="Theme & Glass Customiser"
                aria-label="Theme & Glass Customiser"
              >
                <Palette size={16} />
              </button>
            )}
            {props.onOpenStudio && (
              <button
                type="button"
                className="tv-top-icon-btn glass-btn"
                data-tv-focusable="true"
                onClick={() => void props.onOpenStudio?.()}
                title="A2UI Studio"
                aria-label="A2UI Studio"
              >
                <Sparkles size={16} />
              </button>
            )}
            {props.onRefresh && (
              <button
                type="button"
                className="tv-top-icon-btn glass-btn"
                data-tv-focusable="true"
                onClick={() => void props.onRefresh?.()}
                title="Refresh Apps"
                aria-label="Refresh Apps"
              >
                <RefreshCw size={16} />
              </button>
            )}
            {props.onOpenSettings && (
              <button
                type="button"
                className="tv-top-icon-btn glass-btn"
                data-tv-focusable="true"
                onClick={() => void props.onOpenSettings?.()}
                title="Launcher Settings"
                aria-label="Launcher Settings"
              >
                <Settings size={16} />
              </button>
            )}
          </div>

          {props.timeText && (
            <div className="tv-clock-block">
              <div className="tv-clock-time">{props.timeText}</div>
              {props.dateText && <div className="tv-clock-date">{props.dateText}</div>}
            </div>
          )}
        </div>
      </header>
    );
  },
);

function renderWeatherIcon(key?: string) {
  switch (key) {
    case 'sun':
      return <Sun size={30} />;
    case 'partly':
      return <CloudSun size={30} />;
    case 'cloud':
      return <Cloud size={30} />;
    case 'fog':
      return <CloudFog size={30} />;
    case 'drizzle':
      return <CloudDrizzle size={30} />;
    case 'rain':
      return <CloudRain size={30} />;
    case 'snow':
      return <CloudSnow size={30} />;
    case 'storm':
      return <CloudLightning size={30} />;
    default:
      return <CloudSun size={30} />;
  }
}

export const TvClockWeatherImpl = createComponentImplementation(
  TvClockWeatherApi,
  ({ props }) => {
    const is24h = props.clock24h !== false;
    const [liveClock, setLiveClock] = useState(() => ({
      main: props.timeMain,
      sec: props.timeSeconds,
      period: props.timePeriod ?? '',
    }));

    const formatter = useMemo(
      () =>
        new Intl.DateTimeFormat('en-GB', {
          hour: '2-digit',
          minute: '2-digit',
          second: '2-digit',
          hour12: !is24h,
          timeZone: 'Europe/London',
        }),
      [is24h],
    );

    useEffect(() => {
      const updateNow = () => {
        if (document.hidden || document.querySelector('[data-tv-modal="true"]')) return;
        const parts = formatter.formatToParts(new Date());
        const get = (t: string) => parts.find((p) => p.type === t)?.value ?? '';
        setLiveClock({
          main: `${get('hour')}:${get('minute')}`,
          sec: get('second'),
          period: is24h ? '' : get('dayPeriod').toUpperCase(),
        });
      };
      updateNow();
      const id = window.setInterval(updateNow, 1000);
      return () => window.clearInterval(id);
    }, [formatter, is24h]);

    return (
      <section className="tv-cw-band">
        <div className="tv-cw-clock-col">
          <div className="tv-cw-time-row">
            <span className="tv-cw-time-main">{liveClock.main}</span>
            <span className="tv-cw-time-sec">:{liveClock.sec}</span>
            {liveClock.period && <span className="tv-cw-time-period">{liveClock.period}</span>}
          </div>
          <div className="tv-cw-date-row">
            <span className="tv-cw-full-date">{props.fullDate}</span>
            <span className="tv-cw-dot">•</span>
            <span className="tv-cw-loc">{props.locationLabel}</span>
          </div>
        </div>

        {props.showWeather !== false && (
          <button
            type="button"
            data-tv-focusable="true"
            className="tv-cw-weather-col tv-cw-weather-btn"
            onClick={() => void props.onOpenWeather?.()}
            title="Open 5-Day & Hourly Weather Forecast"
          >
            <div className="tv-cw-weather-top">
              <span className="tv-cw-weather-icon">{renderWeatherIcon(props.iconKey)}</span>
              <div className="tv-cw-weather-primary">
                <span className="tv-cw-temp">{props.tempText}</span>
                <span className="tv-cw-cond">{props.conditionText}</span>
              </div>
            </div>
            <div className="tv-cw-metrics">
              {props.feelsLikeText && (
                <span className="tv-cw-metric">
                  <Thermometer size={12} />
                  <span>{props.feelsLikeText}</span>
                </span>
              )}
              {props.highLowText && (
                <span className="tv-cw-metric">
                  <span>{props.highLowText}</span>
                </span>
              )}
              {props.humidityText && (
                <span className="tv-cw-metric">
                  <Droplets size={12} />
                  <span>{props.humidityText}</span>
                </span>
              )}
              {props.windText && (
                <span className="tv-cw-metric">
                  <Wind size={12} />
                  <span>{props.windText}</span>
                </span>
              )}
              {props.sunText && (
                <span className="tv-cw-metric">
                  <Sunrise size={12} />
                  <span>{props.sunText}</span>
                </span>
              )}
            </div>
          </button>
        )}
      </section>
    );
  },
);

export const TvHeroImpl = createComponentImplementation(TvHeroApi, ({ props }) => {
  return (
    <section className="tv-hero glass-panel">
      <div className="tv-hero-glow" aria-hidden="true" />
      <div className="tv-hero-content">
        <div className="tv-hero-meta">
          {props.eyebrow && <span className="tv-hero-eyebrow">{props.eyebrow}</span>}
          {props.badgeText && <Badge label={props.badgeText} variant="info" />}
        </div>
        <Heading level={1} className="tv-hero-title">
          {props.title}
        </Heading>
        {props.subtitle && (
          <Text className="tv-hero-subtitle" color="secondary">
            {props.subtitle}
          </Text>
        )}
        <div className="tv-hero-actions">
          {props.primaryLabel && props.onPrimary && (
            <Button
              label={props.primaryLabel}
              variant="primary"
              size="lg"
              data-tv-focusable="true"
              className="tv-astryx-btn tv-astryx-btn-primary"
              onClick={() => void props.onPrimary?.()}
            />
          )}
          {props.secondaryLabel && props.onSecondary && (
            <Button
              label={props.secondaryLabel}
              variant="secondary"
              size="lg"
              data-tv-focusable="true"
              className="tv-astryx-btn tv-astryx-btn-secondary"
              onClick={() => void props.onSecondary?.()}
            />
          )}
        </div>
      </div>
    </section>
  );
});

export const TvShelfImpl = createComponentImplementation(
  TvShelfApi,
  ({ props, buildChild }) => {
    const isGrid = props.layout === 'grid';
    return (
      <section className="tv-shelf">
        <div className="tv-shelf-header">
          <Heading level={2} className="tv-shelf-title">
            {props.title}
          </Heading>
          {props.countText && <Badge label={props.countText} variant="neutral" />}
        </div>
        <div className={isGrid ? 'tv-shelf-grid' : 'tv-shelf-track'}>
          {renderChildren(props.children as ChildRef[], buildChild)}
        </div>
      </section>
    );
  },
);

export const TvAppTileImpl = createComponentImplementation(TvAppTileApi, ({ props }) => {
  const btnRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    const el = btnRef.current;
    if (!el || !props.onContextMenu) return;
    const handler = (e: Event) => {
      e.preventDefault();
      void props.onContextMenu?.();
    };
    el.addEventListener('tv-contextmenu', handler);
    return () => el.removeEventListener('tv-contextmenu', handler);
  }, [props.onContextMenu]);

  const imgSrc = props.iconUrl || props.bannerUrl;

  return (
    <button
      ref={btnRef}
      type="button"
      className="tv-app-tile"
      data-tv-focusable="true"
      data-package={props.packageName}
      onClick={() => void props.onLaunch?.()}
      onContextMenu={(e) => {
        e.preventDefault();
        void props.onContextMenu?.();
      }}
    >
      <div className="tv-app-circle">
        <img
          src={imgSrc}
          alt={props.label}
          className="tv-app-circle-img"
          loading="lazy"
          decoding="async"
        />
        <div className="tv-app-circle-sheen" aria-hidden="true" />
        {props.isPinned && (
          <span className="tv-app-pin" title="Pinned">
            <Pin size={11} />
          </span>
        )}
      </div>
      {props.showLabel !== false && <span className="tv-app-tile-label">{props.label}</span>}
    </button>
  );
});

export const TvWidgetCardImpl = createComponentImplementation(TvWidgetCardApi, ({ props }) => {
  const interactive = Boolean(props.onAction);
  const Tag = interactive ? 'button' : 'div';
  return (
    <Tag
      type={interactive ? 'button' : undefined}
      className={`tv-widget-card glass-panel ${interactive ? 'tv-widget-interactive' : ''}`}
      data-tv-focusable={interactive ? 'true' : undefined}
      onClick={interactive ? () => void props.onAction?.() : undefined}
      style={
        props.accent
          ? ({ '--widget-accent': props.accent } as React.CSSProperties)
          : undefined
      }
    >
      <div className="tv-widget-top">
        <span className="tv-widget-icon">{renderWidgetIcon(props.icon)}</span>
        {props.badge && <Badge label={props.badge} variant="neutral" />}
      </div>
      <div className="tv-widget-title">{props.title}</div>
      <div className="tv-widget-value">{props.value}</div>
      {props.subtitle && <div className="tv-widget-sub">{props.subtitle}</div>}
    </Tag>
  );
});

// Standard A2UI primitives backed by Astryx Core (for external/custom A2UI agent surfaces)
export const RowImpl = createComponentImplementation(RowApi, ({ props, buildChild }) => {
  return (
    <div className="a2ui-row" style={{ gap: props.gap ?? 16 }}>
      {renderChildren(props.children as ChildRef[], buildChild)}
    </div>
  );
});

export const ColumnImpl = createComponentImplementation(ColumnApi, ({ props, buildChild }) => {
  return (
    <div className="a2ui-column" style={{ gap: props.gap ?? 14 }}>
      {renderChildren(props.children as ChildRef[], buildChild)}
    </div>
  );
});

export const CardImpl = createComponentImplementation(CardApi, ({ props, buildChild }) => {
  return (
    <Card className="a2ui-astryx-card glass-panel">
      {props.child && buildChild(props.child)}
      {renderChildren(props.children as ChildRef[] | undefined, buildChild)}
    </Card>
  );
});

export const TextImpl = createComponentImplementation(TextApi, ({ props }) => {
  const hint = props.usageHint ?? 'body';
  if (hint === 'h1' || hint === 'h2' || hint === 'h3') {
    const level = hint === 'h1' ? 1 : hint === 'h2' ? 2 : 3;
    return <Heading level={level}>{props.text}</Heading>;
  }
  return <Text color={hint === 'caption' ? 'secondary' : 'primary'}>{props.text}</Text>;
});

export const ButtonImpl = createComponentImplementation(ButtonApi, ({ props }) => {
  const variant =
    props.variant === 'primary' || props.variant === 'secondary' || props.variant === 'tertiary'
      ? props.variant
      : 'primary';
  return (
    <Button
      label={props.label}
      variant={variant}
      size="lg"
      data-tv-focusable="true"
      className="tv-astryx-btn"
      onClick={() => void props.action?.()}
    />
  );
});

export const BadgeImpl = createComponentImplementation(BadgeApi, ({ props }) => {
  return <Badge label={props.label} variant="info" />;
});

export const DividerImpl = createComponentImplementation(DividerApi, () => {
  return <Divider />;
});
