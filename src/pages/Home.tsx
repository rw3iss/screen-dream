import { useState, useEffect, useCallback } from "preact/hooks";
import { listen } from "@tauri-apps/api/event";
import {
  getPlatformInfo,
  takeScreenshot,
  showRegionSelector,
} from "../lib/ipc";
import { useFfmpegStatus, useSettings } from "../stores/settings";
import type { CaptureSource, PlatformInfo, RegionSource } from "../lib/types";
import SourcePicker from "../components/SourcePicker";
import RecordingControls from "../components/RecordingControls";
import { APP_NAME } from "../lib/constants";
import styles from "./Home.module.scss";

export default function Home() {
  const [platform, setPlatform] = useState<PlatformInfo | null>(null);
  const { status } = useFfmpegStatus();
  const { settings } = useSettings();
  const [selectedSource, setSelectedSource] = useState<CaptureSource | null>(
    null,
  );
  const [screenshotMessage, setScreenshotMessage] = useState<string | null>(
    null,
  );

  useEffect(() => {
    getPlatformInfo().then(setPlatform);
  }, []);

  // Listen for region selection events from the overlay window.
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    listen<RegionSource>("region-selected", (event) => {
      const region = event.payload;
      // Use the first monitor as default for region captures.
      // In a full implementation, detect which monitor the region is on.
      setSelectedSource({
        type: "Region",
        data: {
          monitor_id: region.monitor_id || 0,
          x: region.x,
          y: region.y,
          width: region.width,
          height: region.height,
        },
      });
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, []);

  const handleScreenshot = useCallback(async () => {
    if (!selectedSource) return;
    try {
      setScreenshotMessage(null);
      const outputDir =
        settings?.export?.output_directory || "/tmp";
      const timestamp = new Date()
        .toISOString()
        .replace(/[:.]/g, "-")
        .slice(0, 19);
      const path = `${outputDir}/screenshot_${timestamp}.png`;
      const savedPath = await takeScreenshot(selectedSource, path);
      setScreenshotMessage(`Screenshot saved: ${savedPath}`);
    } catch (err) {
      setScreenshotMessage(`Screenshot failed: ${err}`);
    }
  }, [selectedSource, settings]);

  const handleSelectRegion = useCallback(async () => {
    try {
      await showRegionSelector();
    } catch (err) {
      console.error("Failed to open region selector:", err);
    }
  }, []);

  const outputDirectory =
    settings?.export?.output_directory || "/tmp";

  return (
    <div>
      <h1>{APP_NAME}</h1>

      {platform && (
        <section>
          <h3>Platform</h3>
          <p>
            {platform.os} / {platform.display_server} / {platform.arch}
          </p>
        </section>
      )}

      {status && (
        <section>
          <h3>FFmpeg</h3>
          <p>
            {status.available
              ? `v${status.capabilities?.version} (${status.source})`
              : `Not available: ${status.error}`}
          </p>
        </section>
      )}

      <section>
        <h3>Capture Source</h3>
        <SourcePicker
          onSourceSelected={setSelectedSource}
          selectedSource={selectedSource}
        />
        <button
          class={`${styles.btn} ${styles.btnRegion}`}
          onClick={handleSelectRegion}
        >
          Select Region
        </button>
        {selectedSource && (
          <p class={styles.selectedSource}>
            Selected: {selectedSource.type}
            {selectedSource.type === "Region" &&
              ` (${selectedSource.data.width}x${selectedSource.data.height})`}
          </p>
        )}
      </section>

      <section>
        <h3>Recording</h3>
        <RecordingControls
          selectedSource={selectedSource}
          outputDirectory={outputDirectory}
        />
      </section>

      <section>
        <h3>Screenshot</h3>
        <button
          class={styles.btn}
          onClick={handleScreenshot}
          disabled={!selectedSource}
        >
          Take Screenshot
        </button>
        {screenshotMessage && <p>{screenshotMessage}</p>}
      </section>
    </div>
  );
}
