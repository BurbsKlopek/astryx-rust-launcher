import React, { useEffect, useRef } from 'react';
import { Badge, Divider, Heading, Text } from '@astryxdesign/core';
import { ArrowLeft, ArrowRight, EyeOff, Info, Pin, PinOff, Play, Trash2, X } from 'lucide-react';
import type { TvAppInfo } from '../bridge/androidBridge';
import { focusFirstInScope } from '../navigation/spatialNav';

interface Props {
  app: TvAppInfo;
  isPinned: boolean;
  canMoveLeft: boolean;
  canMoveRight: boolean;
  onLaunch: () => void;
  onTogglePin: () => void;
  onMoveFavorite: (delta: -1 | 1) => void;
  onHideApp: () => void;
  onOpenInfo: () => void;
  onUninstall: () => void;
  onClose: () => void;
}

export const AppContextMenu: React.FC<Props> = ({
  app,
  isPinned,
  canMoveLeft,
  canMoveRight,
  onLaunch,
  onTogglePin,
  onMoveFavorite,
  onHideApp,
  onOpenInfo,
  onUninstall,
  onClose,
}) => {
  const modalRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (modalRef.current) {
      focusFirstInScope(modalRef.current);
    }
  }, []);

  return (
    <div className="tv-modal-backdrop" onClick={onClose}>
      <div
        ref={modalRef}
        className="tv-context-sheet glass-panel-strong"
        data-tv-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tv-context-head">
          <img
            src={app.iconUrl || app.bannerUrl}
            alt={app.label}
            className="tv-context-thumb"
          />
          <div>
            <Heading level={3}>{app.label}</Heading>
            <Text size="sm" color="secondary">
              {app.packageName}
            </Text>
            <div className="tv-context-badges">
              <Badge label={app.category} variant="info" />
              {app.versionName && <Badge label={`v${app.versionName}`} variant="neutral" />}
            </div>
          </div>
        </div>

        <Divider />

        <div className="tv-context-actions">
          <button
            type="button"
            className="tv-context-btn"
            data-tv-focusable="true"
            onClick={onLaunch}
          >
            <Play size={18} />
            <span>Open {app.label}</span>
          </button>

          <button
            type="button"
            className="tv-context-btn"
            data-tv-focusable="true"
            onClick={onTogglePin}
          >
            {isPinned ? <PinOff size={18} /> : <Pin size={18} />}
            <span>{isPinned ? 'Unpin from Favorites' : 'Pin to Favorites'}</span>
          </button>

          {isPinned && (canMoveLeft || canMoveRight) && (
            <div className="tv-reorder-row">
              {canMoveLeft && (
                <button
                  type="button"
                  className="tv-context-btn"
                  data-tv-focusable="true"
                  onClick={() => onMoveFavorite(-1)}
                >
                  <ArrowLeft size={16} />
                  <span>Move Left</span>
                </button>
              )}
              {canMoveRight && (
                <button
                  type="button"
                  className="tv-context-btn"
                  data-tv-focusable="true"
                  onClick={() => onMoveFavorite(1)}
                >
                  <ArrowRight size={16} />
                  <span>Move Right</span>
                </button>
              )}
            </div>
          )}

          <button
            type="button"
            className="tv-context-btn"
            data-tv-focusable="true"
            onClick={onHideApp}
          >
            <EyeOff size={18} />
            <span>Hide from Launcher</span>
          </button>

          <button
            type="button"
            className="tv-context-btn"
            data-tv-focusable="true"
            onClick={onOpenInfo}
          >
            <Info size={18} />
            <span>App Info & Permissions</span>
          </button>

          {!app.isSystemApp && (
            <button
              type="button"
              className="tv-context-btn tv-context-danger"
              data-tv-focusable="true"
              onClick={onUninstall}
            >
              <Trash2 size={18} />
              <span>Uninstall App</span>
            </button>
          )}

          <button
            type="button"
            className="tv-context-btn"
            data-tv-focusable="true"
            onClick={onClose}
          >
            <X size={18} />
            <span>Cancel</span>
          </button>
        </div>
      </div>
    </div>
  );
};
