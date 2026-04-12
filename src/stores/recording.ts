import { useState, useEffect, useCallback, useRef } from "preact/hooks";
import {
  startRecording,
  stopRecording,
  pauseRecording,
  resumeRecording,
  getRecordingStatus,
  onRecordingState,
  onRecordingWarning,
} from "../lib/ipc";
import type {
  CaptureSource,
  RecordingConfig,
  RecordingState,
} from "../lib/types";
import { formatError } from "../lib/errors";

export interface RecordingStore {
  state: RecordingState;
  elapsed: number;
  framesCapt: number;
  error: string | null;
  warning: string | null;
  outputPath: string | null;
  start: (source: CaptureSource, outputDir: string) => Promise<void>;
  stop: () => Promise<void>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
}

export function useRecording(): RecordingStore {
  const [state, setState] = useState<RecordingState>("idle");
  const [elapsed, setElapsed] = useState(0);
  const [framesCapt, setFramesCapt] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const [outputPath, setOutputPath] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Listen for recording state events from the backend.
  useEffect(() => {
    let unlisten1: (() => void) | null = null;
    let unlisten2: (() => void) | null = null;

    onRecordingState((s) => {
      setState(s);
    }).then((fn) => {
      unlisten1 = fn;
    });

    onRecordingWarning((msg) => {
      setWarning(msg);
    }).then((fn) => {
      unlisten2 = fn;
    });

    return () => {
      unlisten1?.();
      unlisten2?.();
    };
  }, []);

  // Poll recording status while recording is active.
  useEffect(() => {
    if (state === "recording" || state === "paused") {
      pollRef.current = setInterval(async () => {
        try {
          const status = await getRecordingStatus();
          setElapsed(status.elapsed_seconds);
          setFramesCapt(status.frames_captured);
        } catch {
          // Ignore polling errors.
        }
      }, 500);
    } else {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    }

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [state]);

  const start = useCallback(
    async (source: CaptureSource, outputDir: string) => {
      setError(null);
      setWarning(null);
      setOutputPath(null);

      const timestamp = new Date()
        .toISOString()
        .replace(/[:.]/g, "-")
        .replace("T", "_")
        .slice(0, 19);
      const outPath = `${outputDir}/recording_${timestamp}.mp4`;

      const config: RecordingConfig = {
        source,
        fps: 30,
        video_codec: "libx264",
        crf: 23,
        preset: "ultrafast",
        output_path: outPath,
        capture_microphone: false,
        microphone_device: null,
      };

      try {
        await startRecording(config);
      } catch (err) {
        setError(formatError(err));
        setState("idle");
      }
    },
    [],
  );

  const stop = useCallback(async () => {
    try {
      const path = await stopRecording();
      setOutputPath(path);
      setElapsed(0);
      setFramesCapt(0);
    } catch (err) {
      setError(formatError(err));
    }
  }, []);

  const pause = useCallback(async () => {
    try {
      await pauseRecording();
    } catch (err) {
      setError(formatError(err));
    }
  }, []);

  const resume = useCallback(async () => {
    try {
      await resumeRecording();
    } catch (err) {
      setError(formatError(err));
    }
  }, []);

  return {
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
  };
}
