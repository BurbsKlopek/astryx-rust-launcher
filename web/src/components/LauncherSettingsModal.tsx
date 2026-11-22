import React, { useEffect, useRef, useState } from 'react';
import { Badge, Button, Divider, Heading, Text } from '@astryxdesign/core';
import {
  Cpu,
  Download,
  Eye,
  HardDrive,
  Home,
  Palette,
  RotateCcw,
  Settings,
  Sliders,
  Sparkles,
  Tv,
  Upload,
  Wifi,
  X,
} from 'lucide-react';
import type { TvSystemStatus } from '../bridge/androidBridge';
import { DEFAULT_LAUNCHER_SETTINGS, type LauncherSettings } from '../theme/themes';
import { focusFirstInScope } from '../navigation/spatialNav';

interface Props {
  settings: LauncherSettings;
  status: TvSystemStatus;
  isDefaultHome: boolean;
  favoritesCount: number;
  onChange: (next: LauncherSettings) => void;
  onSetDefaultHome: () => void;
  onClearFavorites: () => void;
  onExportBackup: () => string;
  onImportBackup: (json: string) => void;
  onOpenTheme: () => void;
  onOpenStudio: () => void;
  onOpenAndroidSettings: (section: string) => void;
  onClose: () => void;
}

export const LauncherSettingsModal: React.FC<Props> = ({
  settings,
  status,
  isDefaultHome,
  favoritesCount,
  onChange,
  onSetDefaultHome,
  onClearFavorites,
  onExportBackup,
  onImportBackup,
  onOpenTheme,
  onOpenStudio,
  onOpenAndroidSettings,
  onClose,
}) => {
  const modalRef = useRef<HTMLDivElement | null>(null);
  const [backupOpen, setBackupOpen] = useState(false);
  const [backupJson, setBackupJson] = useState('');
  const [backupMsg, setBackupMsg] = useState<string | null>(null);

  useEffect(() => {
    if (modalRef.current) {
      focusFirstInScope(modalRef.current);
    }
  }, []);

  const update = (patch: Partial<LauncherSettings>) => onChange({ ...settings, ...patch });

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
            <Sliders size={20} />
            <Heading level={2}>Launcher Settings</Heading>
            <Badge label="Astryx TV" variant="info" />
          </div>
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

        <Divider />

        <div className="tv-modal-body">
          {/* Device RAM & Storage telemetry strip */}
          {(status.ramTotalMb || status.storageTotalGb) && (
            <div className="tv-telemetry-strip">
              {status.ramTotalMb ? (
                <span className="tv-pill tv-pill-weather">
                  <Cpu size={13} />
                  <span>
                    RAM: {status.ramFreeMb ?? 0} MB free / {status.ramTotalMb} MB
                  </span>
                </span>
              ) : null}
              {status.storageTotalGb ? (
                <span className="tv-pill tv-pill-weather">
                  <HardDrive size={13} />
                  <span>
                    Storage: {status.storageFreeGb ?? 0} GB free / {status.storageTotalGb} GB
                  </span>
                </span>
              ) : null}
            </div>
          )}

          <section className="tv-sliders-grid">
            <div className="tv-control-box">
              <div className="tv-control-head">
                <span>App Icon Size</span>
                <strong>{settings.iconSize}</strong>
              </div>
              <div className="tv-step-row">
                {(['compact', 'standard', 'large'] as const).map((sz) => (
                  <button
                    key={sz}
                    type="button"
                    data-tv-focusable="true"
                    className={`tv-step-btn ${settings.iconSize === sz ? 'is-active' : ''}`}
                    onClick={() => update({ iconSize: sz })}
                  >
                    {sz}
                  </button>
                ))}
              </div>
            </div>

            <div className="tv-control-box">
              <div className="tv-control-head">
                <span>App Sort Order</span>
                <strong>{settings.sortBy === 'recent' ? 'Recently Used' : 'Alphabetical'}</strong>
              </div>
              <div className="tv-step-row">
                <button
                  type="button"
                  data-tv-focusable="true"
                  className={`tv-step-btn ${settings.sortBy === 'alpha' ? 'is-active' : ''}`}
                  onClick={() => update({ sortBy: 'alpha' })}
                >
                  A–Z
                </button>
                <button
                  type="button"
                  data-tv-focusable="true"
                  className={`tv-step-btn ${settings.sortBy === 'recent' ? 'is-active' : ''}`}
                  onClick={() => update({ sortBy: 'recent' })}
                >
                  Recent
                </button>
              </div>
            </div>

            <div className="tv-control-box tv-toggles-box">
              <button
                type="button"
                data-tv-focusable="true"
                className={`tv-toggle-row ${settings.showHero ? 'is-on' : ''}`}
                onClick={() => update({ showHero: !settings.showHero })}
              >
                <span>Clock & Weather Header</span>
                <span className="tv-toggle-pill">{settings.showHero ? 'ON' : 'OFF'}</span>
              </button>
              <button
                type="button"
                data-tv-focusable="true"
                className={`tv-toggle-row ${settings.showLabels ? 'is-on' : ''}`}
                onClick={() => update({ showLabels: !settings.showLabels })}
              >
                <span>App Labels Under Icons</span>
                <span className="tv-toggle-pill">{settings.showLabels ? 'ON' : 'OFF'}</span>
              </button>
            </div>

            <div className="tv-control-box tv-toggles-box">
              <button
                type="button"
                data-tv-focusable="true"
                className={`tv-toggle-row ${settings.clock24h ? 'is-on' : ''}`}
                onClick={() => update({ clock24h: !settings.clock24h })}
              >
                <span>24-Hour UK Clock</span>
                <span className="tv-toggle-pill">{settings.clock24h ? 'ON' : 'OFF'}</span>
              </button>
              <button
                type="button"
                data-tv-focusable="true"
                className={`tv-toggle-row ${settings.showWeather ? 'is-on' : ''}`}
                onClick={() => update({ showWeather: !settings.showWeather })}
              >
                <span>Live London Weather</span>
                <span className="tv-toggle-pill">{settings.showWeather ? 'ON' : 'OFF'}</span>
              </button>
              <button
                type="button"
                data-tv-focusable="true"
                className={`tv-toggle-row ${settings.showMobileApps ? 'is-on' : ''}`}
                onClick={() => update({ showMobileApps: !settings.showMobileApps })}
              >
                <span>Sideloaded / Mobile Apps</span>
                <span className="tv-toggle-pill">{settings.showMobileApps ? 'ON' : 'OFF'}</span>
              </button>
              <button
                type="button"
                data-tv-focusable="true"
                className={`tv-toggle-row ${settings.showSystemApps ? 'is-on' : ''}`}
                onClick={() => update({ showSystemApps: !settings.showSystemApps })}
              >
                <span>System Apps</span>
                <span className="tv-toggle-pill">{settings.showSystemApps ? 'ON' : 'OFF'}</span>
              </button>
            </div>
          </section>

          <section className="tv-custom-section">
            <Text className="tv-section-label">Launcher Tools & Backup</Text>
            <div className="tv-accent-row">
              <button
                type="button"
                data-tv-focusable="true"
                className={`tv-icon-btn glass-btn ${isDefaultHome ? 'is-active' : ''}`}
                onClick={onSetDefaultHome}
              >
                <Home size={15} />
                <span>
                  {isDefaultHome ? 'Default Home Launcher (Active)' : 'Set as Default Home Launcher'}
                </span>
              </button>

              <button
                type="button"
                data-tv-focusable="true"
                className="tv-icon-btn glass-btn"
                onClick={onOpenTheme}
              >
                <Palette size={15} />
                <span>Theme & Wallpaper</span>
              </button>

              <button
                type="button"
                data-tv-focusable="true"
                className="tv-icon-btn glass-btn"
                onClick={onOpenStudio}
              >
                <Sparkles size={15} />
                <span>A2UI Studio</span>
              </button>

              <button
                type="button"
                data-tv-focusable="true"
                className="tv-icon-btn glass-btn"
                onClick={() => {
                  setBackupJson(onExportBackup());
                  setBackupOpen((v) => !v);
                  setBackupMsg(null);
                }}
              >
                <Download size={15} />
                <span>Backup / Restore</span>
              </button>

              {settings.hiddenApps.length > 0 && (
                <button
                  type="button"
                  data-tv-focusable="true"
                  className="tv-icon-btn glass-btn"
                  onClick={() => update({ hiddenApps: [] })}
                >
                  <Eye size={15} />
                  <span>Unhide ({settings.hiddenApps.length})</span>
                </button>
              )}

              {favoritesCount > 0 && (
                <button
                  type="button"
                  data-tv-focusable="true"
                  className="tv-icon-btn glass-btn"
                  onClick={onClearFavorites}
                >
                  <RotateCcw size={15} />
                  <span>Clear Favorites ({favoritesCount})</span>
                </button>
              )}
            </div>

            {backupOpen && (
              <div className="tv-control-box">
                <textarea
                  className="tv-json-editor"
                  rows={5}
                  value={backupJson}
                  onChange={(e) => setBackupJson(e.target.value)}
                  spellCheck={false}
                />
                <div className="tv-step-row">
                  <button
                    type="button"
                    data-tv-focusable="true"
                    className="tv-icon-btn glass-btn"
                    onClick={() => {
                      try {
                        onImportBackup(backupJson);
                        setBackupMsg('Backup restored successfully.');
                      } catch (err) {
                        setBackupMsg(
                          err instanceof Error ? err.message : 'Invalid backup JSON',
                        );
                      }
                    }}
                  >
                    <Upload size={14} />
                    <span>Apply Backup JSON</span>
                  </button>
                  {backupMsg && <span className="tv-hint">{backupMsg}</span>}
                </div>
              </div>
            )}
          </section>

          <section className="tv-custom-section">
            <Text className="tv-section-label">Android TV System Settings</Text>
            <div className="tv-accent-row">
              <button
                type="button"
                data-tv-focusable="true"
                className="tv-icon-btn glass-btn"
                onClick={() => onOpenAndroidSettings('general')}
              >
                <Settings size={15} />
                <span>All TV Settings</span>
              </button>
              <button
                type="button"
                data-tv-focusable="true"
                className="tv-icon-btn glass-btn"
                onClick={() => onOpenAndroidSettings('network')}
              >
                <Wifi size={15} />
                <span>Network & Wi-Fi</span>
              </button>
              <button
                type="button"
                data-tv-focusable="true"
                className="tv-icon-btn glass-btn"
                onClick={() => onOpenAndroidSettings('display')}
              >
                <Tv size={15} />
                <span>Display & Sound</span>
              </button>
              <button
                type="button"
                data-tv-focusable="true"
                className="tv-icon-btn glass-btn"
                onClick={() => onOpenAndroidSettings('apps')}
              >
                <Sliders size={15} />
                <span>Manage Apps</span>
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
            onClick={() => onChange(DEFAULT_LAUNCHER_SETTINGS)}
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
