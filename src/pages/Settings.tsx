import { useSettings } from "../stores/settings";

export default function SettingsPage() {
  const { settings, error, loading } = useSettings();

  return (
    <div>
      <h1>Settings</h1>

      {error && <p class="error">{error}</p>}

      {loading
        ? <p>Loading settings...</p>
        : <pre>{JSON.stringify(settings, null, 2)}</pre>
      }

      <p>
        Full settings UI will be designed in the UI design document.
        This page currently displays raw settings JSON for verification.
      </p>
    </div>
  );
}
