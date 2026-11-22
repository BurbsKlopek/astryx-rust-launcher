import React, { useEffect, useRef } from 'react';
import { Badge, Button, Divider, Heading, Text } from '@astryxdesign/core';
import { Palette, X } from 'lucide-react';
import {
  ASTRYX_THEMES,
  DEFAULT_TV_THEME,
  WALLPAPER_PRESETS,
  type AstryxBaseThemeId,
  type TvThemeConfig,
  type WallpaperPresetId,
  type WeatherSceneOverride,
} from '../theme/themes';
import { focusFirstInScope } from '../navigation/spatialNav';

interface Props {
  config: TvThemeConfig;
  onChange: (next: TvThemeConfig) => void;
  onClose: () => void;
}

const ACCENT_SWATCHES = [
  { name: 'Violet', primary: '#a855f7', secondary: '#38bdf8' },
  { name: 'Electric Blue', primary: '#3b82f6', secondary: '#22d3ee' },
  { name: 'Neon Rose', primary: '#f43f5e', secondary: '#a78bfa' },
  { name: 'Emerald', primary: '#10b981', secondary: '#2dd4bf' },
  { name: 'Amber Gold', primary: '#f59e0b', secondary: '#fb7185' },
  { name: 'Ice White', primary: '#e2e8f0', secondary: '#60a5fa' },
];

export const ThemeCustomizer: React.FC<Props> = ({ config, onChange, onClose }) => {
  const modalRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (modalRef.current) {
      focusFirstInScope(modalRef.current);
    }
  }, []);

  const update = (patch: Partial<TvThemeConfig>) => onChange({ ...config, ...patch });

  return (
    <div className="tv-modal-backdrop" onClick={onClose}>
      <div
        ref={modalRef}
        className="tv-modal glass-panel-strong"
        data-tv-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tv-modal-header">
          <div className="tv-modal-title-row">
            <Palette size={22} />
            <Heading level={2}>Theme & Glass Customiser</Heading>
            <Badge label="Astryx Design" variant="info" />
          </div>
          <button
            type="button"
            className="tv-icon-btn glass-btn"
            data-tv-focusable="true"
            onClick={onClose}
            aria-label="Close"
          >
            <X size={18} />
            <span>Close</span>
          </button>
        </div>

        <Divider />

        <div className="tv-modal-body">
          <section className="tv-custom-section">
            <Text className="tv-section-label">Astryx Base Theme</Text>
            <div className="tv-choice-grid">
              {(Object.keys(ASTRYX_THEMES) as AstryxBaseThemeId[]).map((id) => {
                const item = ASTRYX_THEMES[id];
                const active = config.astryxTheme === id;
                return (
                  <button
                    key={id}
                    type="button"
                    data-tv-focusable="true"
                    className={`tv-choice-card ${active ? 'is-active' : ''}`}
                    onClick={() => update({ astryxTheme: id })}
                  >
                    <strong>{item.label}</strong>
                    <span>{item.description}</span>
                  </button>
                );
              })}
            </div>
          </section>

          <section className="tv-custom-section">
            <Text className="tv-section-label">Wallpaper Mode (Live Photorealistic Weather or Gradient)</Text>
            <div className="tv-swatch-row">
              {(Object.keys(WALLPAPER_PRESETS) as WallpaperPresetId[]).map((id) => {
                const preset = WALLPAPER_PRESETS[id];
                const active = config.wallpaper === id;
                return (
                  <button
                    key={id}
                    type="button"
                    data-tv-focusable="true"
                    className={`tv-wallpaper-btn ${active ? 'is-active' : ''}`}
                    style={{ background: preset.background }}
                    onClick={() =>
                      update({
                        wallpaper: id,
                        customWallpaperUrl: '',
                        accentColor: preset.accent,
                        accentSecondary: preset.accent2,
                      })
                    }
                  >
                    <span>{preset.label}</span>
                  </button>
                );
              })}
            </div>
            {config.wallpaper === 'live-weather' && (
              <div className="tv-step-row">
                <span className="tv-hint">Scene:</span>
                {(
                  [
                    ['auto', 'Auto (Live)'],
                    ['clear-day', 'Clear Day'],
                    ['golden', 'Golden Hour'],
                    ['clear-night', 'Starry Night'],
                    ['cloudy', 'Cloudy'],
                    ['fog', 'Fog'],
                    ['rain', 'Rain'],
                    ['storm', 'Storm'],
                    ['snow', 'Snow'],
                  ] as [WeatherSceneOverride, string][]
                ).map(([key, label]) => (
                  <button
                    key={key}
                    type="button"
                    data-tv-focusable="true"
                    className={`tv-step-btn ${(config.weatherScene ?? 'auto') === key ? 'is-active' : ''}`}
                    onClick={() => update({ weatherScene: key })}
                  >
                    {label}
                  </button>
                ))}
              </div>
            )}
          </section>

          <section className="tv-custom-section">
            <Text className="tv-section-label">Custom 4K Wallpaper URL (Optional)</Text>
            <div className="tv-endpoint-row">
              <input
                type="url"
                data-tv-focusable="true"
                className="tv-text-input"
                placeholder="https://images.unsplash.com/… (leave blank for gradient)"
                value={config.customWallpaperUrl ?? ''}
                onChange={(e) => update({ customWallpaperUrl: e.target.value })}
              />
              {config.customWallpaperUrl && (
                <button
                  type="button"
                  data-tv-focusable="true"
                  className="tv-icon-btn glass-btn"
                  onClick={() => update({ customWallpaperUrl: '' })}
                >
                  Clear
                </button>
              )}
            </div>
            {config.customWallpaperUrl && (
              <div className="tv-step-row">
                <span className="tv-hint">Dimming:</span>
                {[0.25, 0.45, 0.65, 0.8].map((d) => (
                  <button
                    key={d}
                    type="button"
                    data-tv-focusable="true"
                    className={`tv-step-btn ${Math.abs((config.wallpaperDim ?? 0.45) - d) < 0.05 ? 'is-active' : ''}`}
                    onClick={() => update({ wallpaperDim: d })}
                  >
                    {Math.round(d * 100)}%
                  </button>
                ))}
              </div>
            )}
          </section>

          <section className="tv-custom-section">
            <Text className="tv-section-label">Focus Glow & Accent</Text>
            <div className="tv-accent-row">
              {ACCENT_SWATCHES.map((sw) => {
                const active = config.accentColor.toLowerCase() === sw.primary.toLowerCase();
                return (
                  <button
                    key={sw.name}
                    type="button"
                    data-tv-focusable="true"
                    className={`tv-accent-pill ${active ? 'is-active' : ''}`}
                    onClick={() =>
                      update({ accentColor: sw.primary, accentSecondary: sw.secondary })
                    }
                  >
                    <span
                      className="tv-accent-dot"
                      style={{
                        background: `linear-gradient(135deg, ${sw.primary}, ${sw.secondary})`,
                      }}
                    />
                    <span>{sw.name}</span>
                  </button>
                );
              })}
            </div>
          </section>

          <section className="tv-custom-section tv-sliders-grid">
            <div className="tv-control-box">
              <div className="tv-control-head">
                <span>Background Blur</span>
                <strong>{config.glassBlurPx === 0 ? 'Off' : `${config.glassBlurPx}px`}</strong>
              </div>
              <div className="tv-step-row">
                <button
                  type="button"
                  data-tv-focusable="true"
                  className="tv-step-btn"
                  onClick={() => update({ glassBlurPx: Math.max(0, config.glassBlurPx - 4) })}
                >
                  −
                </button>
                {[0, 6, 12, 20, 30].map((px) => (
                  <button
                    key={px}
                    type="button"
                    data-tv-focusable="true"
                    className={`tv-step-btn ${config.glassBlurPx === px ? 'is-active' : ''}`}
                    onClick={() => update({ glassBlurPx: px })}
                  >
                    {px === 0 ? 'Off' : `${px}px`}
                  </button>
                ))}
                <button
                  type="button"
                  data-tv-focusable="true"
                  className="tv-step-btn"
                  onClick={() => update({ glassBlurPx: Math.min(40, config.glassBlurPx + 4) })}
                >
                  +
                </button>
              </div>
            </div>

            <div className="tv-control-box">
              <div className="tv-control-head">
                <span>Menu Transparency</span>
                <strong>
                  {config.menuTransparency === false
                    ? 'Off (Solid)'
                    : `${Math.round((1 - (config.menuOpacity ?? 0.68)) * 100)}%`}
                </strong>
              </div>
              <div className="tv-step-row">
                <button
                  type="button"
                  data-tv-focusable="true"
                  className={`tv-step-btn ${config.menuTransparency !== false ? 'is-active' : ''}`}
                  onClick={() => update({ menuTransparency: config.menuTransparency === false })}
                >
                  {config.menuTransparency === false ? 'OFF' : 'ON'}
                </button>
                <button
                  type="button"
                  data-tv-focusable="true"
                  className="tv-step-btn"
                  onClick={() =>
                    update({
                      menuTransparency: true,
                      menuOpacity: Math.min(0.92, Number(((config.menuOpacity ?? 0.68) + 0.08).toFixed(2))),
                    })
                  }
                >
                  −
                </button>
                {[
                  { label: 'Low', opacity: 0.84 },
                  { label: 'Med', opacity: 0.68 },
                  { label: 'High', opacity: 0.52 },
                  { label: 'Max', opacity: 0.36 },
                ].map((opt) => (
                  <button
                    key={opt.label}
                    type="button"
                    data-tv-focusable="true"
                    className={`tv-step-btn ${
                      config.menuTransparency !== false &&
                      Math.abs((config.menuOpacity ?? 0.68) - opt.opacity) < 0.05
                        ? 'is-active'
                        : ''
                    }`}
                    onClick={() => update({ menuTransparency: true, menuOpacity: opt.opacity })}
                  >
                    {opt.label}
                  </button>
                ))}
                <button
                  type="button"
                  data-tv-focusable="true"
                  className="tv-step-btn"
                  onClick={() =>
                    update({
                      menuTransparency: true,
                      menuOpacity: Math.max(0.32, Number(((config.menuOpacity ?? 0.68) - 0.08).toFixed(2))),
                    })
                  }
                >
                  +
                </button>
              </div>
            </div>

            <div className="tv-control-box">
              <div className="tv-control-head">
                <span>Shadow Depth</span>
                <strong>{config.shadowStrength}</strong>
              </div>
              <div className="tv-step-row">
                {(['soft', 'deep', 'cinematic'] as const).map((level) => (
                  <button
                    key={level}
                    type="button"
                    data-tv-focusable="true"
                    className={`tv-step-btn ${config.shadowStrength === level ? 'is-active' : ''}`}
                    onClick={() => update({ shadowStrength: level })}
                  >
                    {level}
                  </button>
                ))}
              </div>
            </div>

            <div className="tv-control-box tv-toggles-box">
              <button
                type="button"
                data-tv-focusable="true"
                className={`tv-toggle-row ${config.boldTypography ? 'is-on' : ''}`}
                onClick={() => update({ boldTypography: !config.boldTypography })}
              >
                <span>Bold Display Typography</span>
                <span className="tv-toggle-pill">{config.boldTypography ? 'ON' : 'OFF'}</span>
              </button>
            </div>
          </section>
        </div>

        <div className="tv-modal-footer">
          <Button
            label="Reset Defaults"
            variant="secondary"
            size="lg"
            data-tv-focusable="true"
            className="tv-astryx-btn tv-astryx-btn-secondary"
            onClick={() => onChange(DEFAULT_TV_THEME)}
          />
          <Button
            label="Done"
            variant="primary"
            size="lg"
            data-tv-focusable="true"
            className="tv-astryx-btn tv-astryx-btn-primary"
            onClick={onClose}
          />
        </div>
      </div>
    </div>
  );
};
