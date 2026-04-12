import { useRecording } from "../stores/recording";
import type { CaptureSource } from "../lib/types";
import styles from "./RecordingControls.module.scss";

interface RecordingControlsProps {
  selectedSource: CaptureSource | null;
  outputDirectory: string;
}

function formatTime(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) {
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${m}:${String(s).padStart(2, "0")}`;
}

export default function RecordingControls({
  selectedSource,
  outputDirectory,
}: RecordingControlsProps) {
  const {
    state,
    elapsed,
    framesCapt,
    error,
    warning,
    outputPath,
    start,
    stop,
    pause,
    resume,
  } = useRecording();

  const isIdle = state === "idle" || state === "completed";
  const isRecording = state === "recording";
  const isPaused = state === "paused";
  const isBusy = state === "starting" || state === "stopping";

  const handleStart = async () => {
    if (!selectedSource) return;
    await start(selectedSource, outputDirectory);
  };

  return (
    <div class={styles.recordingControls}>
      {/* Timer display */}
      {(isRecording || isPaused) && (
        <div class={styles.recordingTimer}>
          <span class={`${styles.recordingDot} ${isPaused ? styles.paused : styles.active}`} />
          <span class={styles.time}>{formatTime(elapsed)}</span>
          <span class={styles.frames}>{framesCapt} frames</span>
        </div>
      )}

      {/* Control buttons */}
      <div class={styles.recordingButtons}>
        {isIdle && (
          <button
            class={`${styles.btn} ${styles.btnRecord}`}
            onClick={handleStart}
            disabled={!selectedSource || isBusy}
            title={!selectedSource ? "Select a source first" : "Start recording"}
          >
            Record
          </button>
        )}

        {isRecording && (
          <>
            <button class={`${styles.btn} ${styles.btnPause}`} onClick={pause}>
              Pause
            </button>
            <button class={`${styles.btn} ${styles.btnStop}`} onClick={stop}>
              Stop
            </button>
          </>
        )}

        {isPaused && (
          <>
            <button class={`${styles.btn} ${styles.btnResume}`} onClick={resume}>
              Resume
            </button>
            <button class={`${styles.btn} ${styles.btnStop}`} onClick={stop}>
              Stop
            </button>
          </>
        )}

        {isBusy && (
          <button class={styles.btn} disabled>
            {state === "starting" ? "Starting..." : "Stopping..."}
          </button>
        )}
      </div>

      {/* Status messages */}
      {error && <p class={styles.error}>{error}</p>}
      {warning && <p class={styles.warning}>{warning}</p>}
      {outputPath && state === "completed" && (
        <p class={styles.success}>Recording saved: {outputPath}</p>
      )}
    </div>
  );
}
