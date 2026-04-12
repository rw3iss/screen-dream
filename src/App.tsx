import { useState, useEffect } from "preact/hooks";
import Layout from "./components/Layout";
import Home from "./pages/Home";
import SettingsPage from "./pages/Settings";
import RegionSelector from "./components/RegionSelector";

function useRoute() {
  const [route, setRoute] = useState(window.location.hash || "#/");

  useEffect(() => {
    const handler = () => setRoute(window.location.hash || "#/");
    window.addEventListener("hashchange", handler);
    return () => window.removeEventListener("hashchange", handler);
  }, []);

  return route;
}

export default function App() {
  const route = useRoute();

  // The region selector runs in its own transparent window -- no Layout wrapper.
  if (route === "#/region-selector") {
    return <RegionSelector />;
  }

  return (
    <Layout>
      {route === "#/settings" ? <SettingsPage /> : <Home />}
    </Layout>
  );
}
