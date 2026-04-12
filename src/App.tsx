import { useState, useEffect } from "preact/hooks";
import Layout from "./components/Layout";
import Home from "./pages/Home";
import SettingsPage from "./pages/Settings";

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
  return (
    <Layout>
      {route === "#/settings" ? <SettingsPage /> : <Home />}
    </Layout>
  );
}
