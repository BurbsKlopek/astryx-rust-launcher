import React, { useEffect, useRef, useState } from 'react';
import { Badge, Button, Divider, Heading, Text } from '@astryxdesign/core';
import { Radio, Sparkles, Trash2, X } from 'lucide-react';
import { SAMPLE_AGENT_SURFACE_JSON } from '../a2ui/launcherMessages';
import { fetchA2uiMessagesFromEndpoint } from '../a2ui/agentClient';
import { focusFirstInScope } from '../navigation/spatialNav';

interface Props {
  activeSurfaceIds: string[];
  agentUrl: string;
  onSaveAgentUrl: (url: string) => void;
  onApplyMessages: (messages: any[]) => void;
  onClearCustomSurfaces: () => void;
  onClose: () => void;
}

export const A2uiStudioModal: React.FC<Props> = ({
  activeSurfaceIds,
  agentUrl,
  onSaveAgentUrl,
  onApplyMessages,
  onClearCustomSurfaces,
  onClose,
}) => {
  const modalRef = useRef<HTMLDivElement | null>(null);
  const [endpoint, setEndpoint] = useState(agentUrl);
  const [jsonInput, setJsonInput] = useState(SAMPLE_AGENT_SURFACE_JSON);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (modalRef.current) {
      focusFirstInScope(modalRef.current);
    }
  }, []);

  const handleApplyJson = () => {
    setErrorMsg(null);
    setStatusMsg(null);
    try {
      const parsed = JSON.parse(jsonInput);
      const list = Array.isArray(parsed) ? parsed : [parsed];
      onApplyMessages(list);
      setStatusMsg(`Processed ${list.length} A2UI v1.0 message(s) live.`);
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : 'Invalid JSON payload');
    }
  };

  const handleFetchEndpoint = async () => {
    const trimmed = endpoint.trim();
    if (!trimmed) {
      setErrorMsg('Enter an A2UI agent HTTP/SSE endpoint URL first.');
      return;
    }
    setLoading(true);
    setErrorMsg(null);
    setStatusMsg(null);
    try {
      onSaveAgentUrl(trimmed);
      const msgs = await fetchA2uiMessagesFromEndpoint(trimmed);
      if (msgs.length === 0) {
        setStatusMsg('Connected, but endpoint returned 0 A2UI messages.');
      } else {
        onApplyMessages(msgs);
        setStatusMsg(`Streamed ${msgs.length} A2UI message(s) from agent.`);
      }
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : 'Failed to connect to agent endpoint');
    } finally {
      setLoading(false);
    }
  };

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
            <Sparkles size={22} />
            <Heading level={2}>A2UI v1.0 Surface Studio</Heading>
            <Badge label={`${activeSurfaceIds.length} active`} variant="info" />
          </div>
          <button
            type="button"
            className="tv-icon-btn glass-btn"
            data-tv-focusable="true"
            onClick={onClose}
          >
            <X size={18} />
            <span>Close</span>
          </button>
        </div>

        <Divider />

        <div className="tv-modal-body">
          <div className="tv-surface-chips">
            {activeSurfaceIds.map((id) => (
              <Badge key={id} label={`surface: ${id}`} variant="neutral" />
            ))}
          </div>

          <section className="tv-custom-section">
            <Text className="tv-section-label">Live A2UI Agent Endpoint (JSON / NDJSON / SSE)</Text>
            <div className="tv-endpoint-row">
              <input
                type="url"
                data-tv-focusable="true"
                className="tv-text-input"
                placeholder="http://192.168.1.50:8080/a2ui/tv-surfaces"
                value={endpoint}
                onChange={(e) => setEndpoint(e.target.value)}
              />
              <Button
                label={loading ? 'Syncing…' : 'Sync Agent'}
                variant="primary"
                size="lg"
                data-tv-focusable="true"
                className="tv-astryx-btn tv-astryx-btn-primary"
                onClick={handleFetchEndpoint}
              />
            </div>
          </section>

          <section className="tv-custom-section">
            <div className="tv-control-head">
              <Text className="tv-section-label">Inject A2UI v1.0 Surface Payload</Text>
              <span className="tv-hint">Catalog: https://astryxtv.local/catalogs/tv-v1.json</span>
            </div>
            <textarea
              className="tv-json-editor"
              rows={9}
              value={jsonInput}
              onChange={(e) => setJsonInput(e.target.value)}
              spellCheck={false}
            />
          </section>

          {statusMsg && (
            <div className="tv-feedback tv-feedback-ok">
              <Radio size={16} />
              <span>{statusMsg}</span>
            </div>
          )}
          {errorMsg && <div className="tv-feedback tv-feedback-err">{errorMsg}</div>}
        </div>

        <div className="tv-modal-footer">
          <button
            type="button"
            className="tv-icon-btn glass-btn"
            data-tv-focusable="true"
            onClick={() => {
              onClearCustomSurfaces();
              setStatusMsg('Cleared custom A2UI surfaces.');
            }}
          >
            <Trash2 size={16} />
            <span>Clear Custom Surfaces</span>
          </button>
          <Button
            label="Inject Sample / JSON Surface"
            variant="primary"
            size="lg"
            data-tv-focusable="true"
            className="tv-astryx-btn tv-astryx-btn-primary"
            onClick={handleApplyJson}
          />
        </div>
      </div>
    </div>
  );
};
