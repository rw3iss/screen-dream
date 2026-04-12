import { useState, useEffect, useCallback } from "preact/hooks";
import { enumerateSources } from "../lib/ipc";
import type {
  AvailableSources,
  CaptureSource,
  MonitorInfo,
  WindowInfo,
} from "../lib/types";
import { formatError } from "../lib/errors";
import styles from "./SourcePicker.module.scss";

interface SourcePickerProps {
  onSourceSelected: (source: CaptureSource) => void;
  /** Currently selected source, if any. */
  selectedSource: CaptureSource | null;
}

type SourceTab = "screens" | "windows";

export default function SourcePicker({
  onSourceSelected,
  selectedSource,
}: SourcePickerProps) {
  const [sources, setSources] = useState<AvailableSources | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [activeTab, setActiveTab] = useState<SourceTab>("screens");

  const loadSources = useCallback(async () => {
    try {
      setError(null);
      setLoading(true);
      const s = await enumerateSources();
      setSources(s);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSources();
  }, [loadSources]);

  const isScreenSelected = (monitor: MonitorInfo): boolean => {
    if (!selectedSource) return false;
    return (
      selectedSource.type === "Screen" &&
      selectedSource.data.monitor_id === monitor.id
    );
  };

  const isWindowSelected = (win: WindowInfo): boolean => {
    if (!selectedSource) return false;
    return (
      selectedSource.type === "Window" &&
      selectedSource.data.window_id === win.id
    );
  };

  const selectScreen = (monitor: MonitorInfo) => {
    onSourceSelected({
      type: "Screen",
      data: { monitor_id: monitor.id },
    });
  };

  const selectWindow = (win: WindowInfo) => {
    onSourceSelected({
      type: "Window",
      data: { window_id: win.id },
    });
  };

  if (loading) {
    return <div class={styles.sourcePicker}><p>Loading sources...</p></div>;
  }

  if (error) {
    return (
      <div class={styles.sourcePicker}>
        <p class={styles.error}>Failed to load sources: {error}</p>
        <button onClick={loadSources}>Retry</button>
      </div>
    );
  }

  if (!sources) return null;

  return (
    <div class={styles.sourcePicker}>
      <div class={styles.sourceTabs}>
        <button
          class={`${styles.tab} ${activeTab === "screens" ? styles.active : ""}`}
          onClick={() => setActiveTab("screens")}
        >
          Screens ({sources.monitors.length})
        </button>
        <button
          class={`${styles.tab} ${activeTab === "windows" ? styles.active : ""}`}
          onClick={() => setActiveTab("windows")}
          disabled={sources.windows_unavailable}
          title={sources.windows_unavailable_reason || undefined}
        >
          Windows ({sources.windows_unavailable ? "N/A" : sources.windows.length})
        </button>
        <button class={styles.tabRefresh} onClick={loadSources} title="Refresh sources">
          Refresh
        </button>
      </div>

      {activeTab === "screens" && (
        <div class={styles.sourceList}>
          {sources.monitors.map((monitor) => (
            <div
              key={monitor.id}
              class={`${styles.sourceItem} ${isScreenSelected(monitor) ? styles.selected : ""}`}
              onClick={() => selectScreen(monitor)}
            >
              <div class={styles.sourceName}>
                {monitor.friendly_name || monitor.name}
                {monitor.is_primary && <span class={styles.badge}>Primary</span>}
              </div>
              <div class={styles.sourceDetails}>
                {monitor.width}x{monitor.height}
                {monitor.scale_factor !== 1.0 && ` (${monitor.scale_factor}x scale)`}
              </div>
            </div>
          ))}
        </div>
      )}

      {activeTab === "windows" && (
        <div class={styles.sourceList}>
          {sources.windows_unavailable && (
            <p class={styles.notice}>{sources.windows_unavailable_reason}</p>
          )}
          {sources.windows.map((win) => (
            <div
              key={win.id}
              class={`${styles.sourceItem} ${isWindowSelected(win) ? styles.selected : ""}`}
              onClick={() => selectWindow(win)}
            >
              <div class={styles.sourceName}>
                {win.title || win.app_name || `Window ${win.id}`}
              </div>
              <div class={styles.sourceDetails}>
                {win.app_name} -- {win.width}x{win.height}
                {win.is_focused && <span class={styles.badge}>Focused</span>}
              </div>
            </div>
          ))}
          {!sources.windows_unavailable && sources.windows.length === 0 && (
            <p class={styles.notice}>No visible windows found.</p>
          )}
        </div>
      )}
    </div>
  );
}
