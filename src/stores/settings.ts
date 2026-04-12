import { useState, useEffect, useCallback } from "preact/hooks";
import { getSettings, saveSettings, getFfmpegStatus } from "../lib/ipc";
import type { AppSettings, FfmpegStatus } from "../lib/types";
import { formatError } from "../lib/errors";

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    try {
      setError(null);
      setLoading(true);
      const s = await getSettings();
      setSettings(s);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setLoading(false);
    }
  }, []);

  const update = useCallback(async (updated: AppSettings) => {
    try {
      setError(null);
      await saveSettings(updated);
      setSettings(updated);
    } catch (err) {
      setError(formatError(err));
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  return { settings, error, loading, update, reload: load };
}

export function useFfmpegStatus() {
  const [status, setStatus] = useState<FfmpegStatus | null>(null);

  const load = useCallback(async () => {
    try {
      setStatus(await getFfmpegStatus());
    } catch (err) {
      setStatus({
        available: false,
        source: "unknown",
        capabilities: null,
        error: formatError(err),
      });
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  return { status, reload: load };
}
