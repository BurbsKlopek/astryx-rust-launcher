import React, { useEffect, useRef, useState } from 'react';
import { Badge, Divider, Heading, Text } from '@astryxdesign/core';
import {
  Cloud,
  CloudDrizzle,
  CloudFog,
  CloudLightning,
  CloudRain,
  CloudSnow,
  CloudSun,
  Droplets,
  RefreshCw,
  Sun,
  Sunrise,
  Thermometer,
  Umbrella,
  Wind,
  X,
} from 'lucide-react';
import type { DetailedWeather, WeatherIconKey } from '../a2ui/weather';
import { focusFirstInScope } from '../navigation/spatialNav';

interface Props {
  weather: DetailedWeather | null;
  onRefresh: () => Promise<void>;
  onClose: () => void;
}

function WeatherGlyph({ iconKey, size = 20 }: { iconKey: WeatherIconKey; size?: number }) {
  switch (iconKey) {
    case 'sun':
      return <Sun size={size} />;
    case 'partly':
      return <CloudSun size={size} />;
    case 'cloud':
      return <Cloud size={size} />;
    case 'fog':
      return <CloudFog size={size} />;
    case 'drizzle':
      return <CloudDrizzle size={size} />;
    case 'rain':
      return <CloudRain size={size} />;
    case 'snow':
      return <CloudSnow size={size} />;
    case 'storm':
      return <CloudLightning size={size} />;
  }
}

export const WeatherModal: React.FC<Props> = ({ weather, onRefresh, onClose }) => {
  const modalRef = useRef<HTMLDivElement | null>(null);
  const [unit, setUnit] = useState<'C' | 'F'>('C');
  const [refreshing, setRefreshing] = useState(false);

  useEffect(() => {
    if (modalRef.current) {
      focusFirstInScope(modalRef.current);
    }
  }, []);

  const fmt = (c: number) => (unit === 'F' ? `${Math.round((c * 9) / 5 + 32)}°F` : `${c}°C`);
  const fmtShort = (c: number) => (unit === 'F' ? `${Math.round((c * 9) / 5 + 32)}°` : `${c}°`);

  const handleRefresh = async () => {
    setRefreshing(true);
    try {
      await onRefresh();
    } finally {
      setRefreshing(false);
    }
  };

  return (
    <div className="tv-modal-backdrop" onClick={onClose}>
      <div
        ref={modalRef}
        className="tv-modal tv-weather-modal glass-panel-strong"
        data-tv-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tv-modal-header">
          <div className="tv-modal-title-row">
            {weather && <WeatherGlyph iconKey={weather.iconKey} size={22} />}
            <Heading level={2}>London Weather</Heading>
            {weather && <Badge label={`Updated ${weather.updatedTime} UK`} variant="info" />}
          </div>
          <div className="tv-step-row">
            <button
              type="button"
              className="tv-icon-btn glass-btn"
              data-tv-focusable="true"
              onClick={() => setUnit((u) => (u === 'C' ? 'F' : 'C'))}
            >
              <span>°{unit}</span>
            </button>
            <button
              type="button"
              className="tv-icon-btn glass-btn"
              data-tv-focusable="true"
              onClick={() => void handleRefresh()}
            >
              <RefreshCw size={14} />
              <span>{refreshing ? 'Updating…' : 'Refresh'}</span>
            </button>
            <button
              type="button"
              className="tv-icon-btn glass-btn"
              data-tv-focusable="true"
              onClick={onClose}
            >
              <X size={16} />
              <span>Close</span>
            </button>
          </div>
        </div>

        <Divider />

        <div className="tv-modal-body tv-weather-body">
          {!weather ? (
            <Text color="secondary">Fetching live London weather from Open-Meteo…</Text>
          ) : (
            <>
              {/* Current conditions summary */}
              <div className="tv-weather-hero-row">
                <div className="tv-weather-hero-main">
                  <span className="tv-cw-weather-icon">
                    <WeatherGlyph iconKey={weather.iconKey} size={36} />
                  </span>
                  <div>
                    <div className="tv-weather-hero-temp">{fmt(weather.tempC)}</div>
                    <div className="tv-weather-hero-cond">{weather.condition}</div>
                  </div>
                </div>
                <div className="tv-cw-metrics">
                  <span className="tv-cw-metric">
                    <Thermometer size={12} />
                    <span>Feels {fmtShort(weather.feelsLikeC)}</span>
                  </span>
                  <span className="tv-cw-metric">
                    <span>
                      H:{fmtShort(weather.highC)} L:{fmtShort(weather.lowC)}
                    </span>
                  </span>
                  <span className="tv-cw-metric">
                    <Umbrella size={12} />
                    <span>{weather.precipProb}% rain</span>
                  </span>
                  <span className="tv-cw-metric">
                    <Droplets size={12} />
                    <span>{weather.humidity}%</span>
                  </span>
                  <span className="tv-cw-metric">
                    <Wind size={12} />
                    <span>{weather.windKmh} km/h</span>
                  </span>
                  <span className="tv-cw-metric">
                    <Sunrise size={12} />
                    <span>
                      {weather.sunrise} / {weather.sunset}
                    </span>
                  </span>
                </div>
              </div>

              {/* Next 8 Hours */}
              {weather.hourly?.length > 0 && (
                <section className="tv-custom-section">
                  <Text className="tv-section-label">Next 8 Hours</Text>
                  <div className="tv-hourly-grid">
                    {weather.hourly.map((h) => (
                      <div key={h.hourLabel} className="tv-hourly-card" tabIndex={0} data-tv-focusable="true">
                        <span className="tv-hourly-time">{h.hourLabel}</span>
                        <span className="tv-cw-weather-icon">
                          <WeatherGlyph iconKey={h.iconKey} size={18} />
                        </span>
                        <strong className="tv-hourly-temp">{fmtShort(h.tempC)}</strong>
                        <span className="tv-hourly-rain">{h.precipProb}%</span>
                      </div>
                    ))}
                  </div>
                </section>
              )}

              {/* 5-Day Forecast (5-column grid — all 5 days visible at once, zero cutoff) */}
              {weather.daily?.length > 0 && (
                <section className="tv-custom-section">
                  <Text className="tv-section-label">5-Day Forecast</Text>
                  <div className="tv-daily-grid">
                    {weather.daily.map((d) => (
                      <div key={d.dayLabel} className="tv-daily-card" tabIndex={0} data-tv-focusable="true">
                        <strong className="tv-daily-day">{d.dayLabel}</strong>
                        <span className="tv-cw-weather-icon">
                          <WeatherGlyph iconKey={d.iconKey} size={20} />
                        </span>
                        <span className="tv-daily-cond-text">{d.condition}</span>
                        <div className="tv-daily-bottom">
                          <span className="tv-daily-rain">{d.precipProb}%</span>
                          <span className="tv-daily-temps">
                            {fmtShort(d.lowC)} / <strong>{fmtShort(d.highC)}</strong>
                          </span>
                        </div>
                      </div>
                    ))}
                  </div>
                </section>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
};
