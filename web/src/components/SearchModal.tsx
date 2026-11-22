import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Badge, Divider, Heading } from '@astryxdesign/core';
import { Search, X } from 'lucide-react';
import type { TvAppInfo } from '../bridge/androidBridge';
import { focusFirstInScope } from '../navigation/spatialNav';

interface Props {
  apps: TvAppInfo[];
  onLaunch: (app: TvAppInfo) => void;
  onClose: () => void;
}

const CATEGORIES = ['all', 'streaming', 'music', 'games', 'tools'] as const;

export const SearchModal: React.FC<Props> = ({ apps, onLaunch, onClose }) => {
  const modalRef = useRef<HTMLDivElement | null>(null);
  const [query, setQuery] = useState('');
  const [cat, setCat] = useState<(typeof CATEGORIES)[number]>('all');

  useEffect(() => {
    if (modalRef.current) {
      focusFirstInScope(modalRef.current);
    }
  }, []);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return apps.filter((a) => {
      if (cat !== 'all' && a.category !== cat) return false;
      if (!q) return true;
      return a.label.toLowerCase().includes(q) || a.packageName.toLowerCase().includes(q);
    });
  }, [apps, query, cat]);

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
            <Search size={18} />
            <Heading level={2}>Search Apps</Heading>
            <Badge label={`${filtered.length}`} variant="info" />
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

        <div className="tv-endpoint-row">
          <input
            type="text"
            data-tv-focusable="true"
            className="tv-text-input"
            placeholder="Type app name (e.g. YouTube, Plex, NordVPN)…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {query && (
            <button
              type="button"
              className="tv-icon-btn glass-btn"
              data-tv-focusable="true"
              onClick={() => setQuery('')}
            >
              Clear
            </button>
          )}
        </div>

        <div className="tv-step-row">
          {CATEGORIES.map((c) => (
            <button
              key={c}
              type="button"
              data-tv-focusable="true"
              className={`tv-step-btn ${cat === c ? 'is-active' : ''}`}
              onClick={() => setCat(c)}
            >
              {c}
            </button>
          ))}
        </div>

        <Divider />

        <div className="tv-modal-body">
          <div className="tv-shelf-grid">
            {filtered.map((app) => (
              <button
                key={app.packageName}
                type="button"
                className="tv-app-tile"
                data-tv-focusable="true"
                onClick={() => onLaunch(app)}
              >
                <div className="tv-app-circle">
                  <img
                    src={app.iconUrl || app.bannerUrl}
                    alt={app.label}
                    className="tv-app-circle-img"
                  />
                  <div className="tv-app-circle-sheen" aria-hidden="true" />
                </div>
                <span className="tv-app-tile-label">{app.label}</span>
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
};
