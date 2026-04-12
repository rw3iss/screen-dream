import type { ComponentChildren } from "preact";
import StatusBar from "./StatusBar";

interface LayoutProps {
  children: ComponentChildren;
}

export default function Layout({ children }: LayoutProps) {
  return (
    <div class="app-layout">
      <nav class="app-nav">
        <a href="#/">Home</a>
        <a href="#/settings">Settings</a>
      </nav>
      <main class="app-main">{children}</main>
      <StatusBar />
    </div>
  );
}
