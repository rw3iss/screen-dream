import { useState, useEffect } from "preact/hooks";
import { getPlatformInfo } from "../lib/ipc";
import { useFfmpegStatus } from "../stores/settings";
import type { PlatformInfo } from "../lib/types";

export default function Home() {
  const [platform, setPlatform] = useState<PlatformInfo | null>(null);
  const { status } = useFfmpegStatus();

  useEffect(() => {
    getPlatformInfo().then(setPlatform);
  }, []);

  return (
    <div>
      <h1>Screen Dream</h1>

      {platform && (
        <section>
          <h3>Platform</h3>
          <p>{platform.os} / {platform.display_server} / {platform.arch}</p>
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
          {status.capabilities && (
            <ul>
              <li>Video: {status.capabilities.video_encoders.join(", ") || "none"}</li>
              <li>Audio: {status.capabilities.audio_encoders.join(", ") || "none"}</li>
            </ul>
          )}
        </section>
      )}

      <section>
        <p>
          Recording and screenshot controls will be added in Plan 2.
          See the UI design document for detailed layout specifications.
        </p>
      </section>
    </div>
  );
}
