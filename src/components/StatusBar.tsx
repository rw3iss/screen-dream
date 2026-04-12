import { useFfmpegStatus } from "../stores/settings";

export default function StatusBar() {
  const { status } = useFfmpegStatus();

  if (!status) return <div class="status-bar"><span>Loading...</span></div>;

  return (
    <div class="status-bar">
      <span class={status.available ? "status-ok" : "status-error"}>
        FFmpeg: {status.available ? status.capabilities?.version : "Not found"}
        {" "}({status.source})
      </span>
    </div>
  );
}
