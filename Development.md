# Screen Dream — Development Guide

## Prerequisites

### All Platforms

- **Rust** (stable toolchain) — [rustup.rs](https://rustup.rs)
- **Node.js** 18+ — [nodejs.org](https://nodejs.org)
- **pnpm** — `npm install -g pnpm`
- **FFmpeg** — must be available on `$PATH` at runtime

### Linux (Debian/Ubuntu)

```bash
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev \
  librsvg2-dev patchelf libssl-dev ffmpeg
```

For **Wayland** support you also need:

```bash
sudo apt install libxdo-dev
```

### Linux (Fedora)

```bash
sudo dnf install webkit2gtk4.1-devel libappindicator-gtk3-devel \
  librsvg2-devel openssl-devel ffmpeg
```

### macOS

```bash
brew install ffmpeg
xcode-select --install
```

Screen capture requires the **ScreenCaptureKit** entitlement on macOS 12.3+. The Tauri
build handles entitlements automatically when you sign the app.

### Windows

Install FFmpeg and ensure it is on your `PATH`. The MSVC build tools are required
(Visual Studio Build Tools with the "Desktop development with C++" workload).

---

## Project Structure

```
screen-dream/
├── index.html                 # Vite entry HTML
├── src/                       # Frontend (Preact + TypeScript)
│   ├── main.tsx               # App entry point
│   ├── App.tsx                # Root component
│   ├── components/            # Shared UI components
│   │   ├── Layout.tsx
│   │   └── StatusBar.tsx
│   ├── pages/                 # Route-level pages
│   │   ├── Home.tsx
│   │   └── Settings.tsx
│   ├── stores/                # Preact signals state stores
│   │   └── settings.ts
│   ├── lib/                   # Frontend utilities
│   │   ├── ipc.ts             # Typed IPC wrapper (invoke)
│   │   ├── types.ts           # Shared TypeScript types
│   │   └── errors.ts          # Error handling utilities
│   └── styles/                # SCSS framework
│       ├── main.scss           # Entry — imports all partials
│       ├── _reset.scss         # Minimal CSS reset
│       ├── _variables.scss     # Design tokens & SCSS variables
│       ├── _theme-dark.scss    # Dark theme (default)
│       ├── _theme-light.scss   # Light theme
│       ├── _mixins.scss        # Utility mixins
│       └── _utilities.scss     # Utility classes
├── src-tauri/                 # Rust backend (Tauri 2)
│   ├── src/
│   │   ├── main.rs            # Tauri bootstrap
│   │   ├── lib.rs             # Plugin & command registration
│   │   ├── state.rs           # App state (Mutex-wrapped)
│   │   ├── error.rs           # Error types with serde
│   │   └── commands/          # IPC command handlers
│   │       ├── mod.rs
│   │       ├── settings.rs
│   │       ├── ffmpeg.rs
│   │       ├── platform.rs
│   │       └── shortcuts.rs
│   └── crates/
│       ├── domain/            # Pure business logic (no framework deps)
│       │   └── src/
│       │       ├── lib.rs
│       │       ├── app_config.rs
│       │       ├── error.rs
│       │       ├── settings/
│       │       ├── ffmpeg/
│       │       └── platform/
│       └── infrastructure/    # External integrations
│           └── src/
│               ├── lib.rs
│               ├── settings/
│               └── ffmpeg/
├── docs/
│   └── plans/                 # Implementation plans
├── vite.config.ts
├── tsconfig.json
└── package.json
```

---

## Architecture

Screen Dream follows **Clean Architecture** with three layers:

### Domain Layer (`src-tauri/crates/domain`)

Pure Rust business logic with no framework dependencies. Contains:

- Data models and enums (settings, recording profiles, FFmpeg presets)
- Validation rules
- Trait definitions (ports) for infrastructure

### Infrastructure Layer (`src-tauri/crates/infrastructure`)

Implements domain traits with real I/O:

- File-based settings persistence (JSON)
- FFmpeg process management
- Platform-specific screen capture

### Application Layer (`src-tauri/src`)

Tauri command handlers that wire domain + infrastructure together. Manages
application state via `Mutex<AppState>` and exposes IPC commands to the frontend.

### Frontend (`src/`)

Preact with TypeScript. Uses Preact Signals for state management. Communicates
with the Rust backend exclusively through typed IPC calls (`@tauri-apps/api`).

### IPC Command Flow

```
Frontend (TypeScript)
  └─ invoke("command_name", { args })     ← src/lib/ipc.ts
      └─ Tauri IPC bridge
          └─ #[tauri::command] handler    ← src-tauri/src/commands/
              └─ Domain service call      ← crates/domain/
                  └─ Infrastructure I/O   ← crates/infrastructure/
```

---

## Development Commands

```bash
# Start dev server with hot reload (frontend + Rust rebuilds)
pnpm tauri dev

# Build frontend only (type-check + Vite)
pnpm build

# Build release binary
pnpm tauri build

# Type-check without emitting
npx tsc --noEmit

# Run Rust tests
cd src-tauri && cargo test --workspace

# Run Rust linter
cd src-tauri && cargo clippy --workspace -- -D warnings

# Format Rust code
cd src-tauri && cargo fmt --all
```

---

## Adding a New IPC Command

1. **Define the domain type** in `src-tauri/crates/domain/src/` if the command
   needs new data structures.

2. **Create the command handler** in `src-tauri/src/commands/`. Example:

   ```rust
   // src-tauri/src/commands/my_feature.rs
   use tauri::State;
   use crate::state::AppState;

   #[tauri::command]
   pub async fn my_command(
       state: State<'_, AppState>,
   ) -> Result<String, String> {
       let app = state.inner().lock().await;
       Ok("result".to_string())
   }
   ```

3. **Register the command** in `src-tauri/src/commands/mod.rs` — add it to the
   module exports and the command list.

4. **Register in `lib.rs`** — add to the `invoke_handler` macro.

5. **Add the TypeScript binding** in `src/lib/ipc.ts`:

   ```typescript
   export async function myCommand(): Promise<string> {
     return invoke<string>("my_command");
   }
   ```

6. **Add the TypeScript type** in `src/lib/types.ts` if needed.

---

## Adding a New Setting

1. **Add the field** to the settings struct in `src-tauri/crates/domain/src/settings/`.
   Include a `Default` impl and serde attributes.

2. **Update the settings store** in `src-tauri/crates/infrastructure/src/settings/`
   if persistence logic changes.

3. **Expose via IPC** — the `get_settings` / `update_settings` commands should
   pick up the new field automatically if it is part of the serialized struct.

4. **Update the frontend store** in `src/stores/settings.ts` — add the field to
   the signal and any derived state.

5. **Add UI controls** in `src/pages/Settings.tsx`.

---

## SCSS Framework Guide

### Design Tokens

All themeable colors are CSS custom properties (set in `_theme-dark.scss` and
`_theme-light.scss`). Use them in your styles:

```scss
.my-component {
  color: var(--color-text-primary);
  background: var(--color-bg-secondary);
  border: 1px solid var(--color-border);
}
```

Static tokens (spacing, font sizes, radii) are SCSS variables in `_variables.scss`:

```scss
@use '../styles/variables' as *;

.my-component {
  padding: $spacing-md;
  border-radius: $radius-md;
  font-size: $font-size-sm;
}
```

### Using Mixins

```scss
@use '../styles/mixins' as *;

.responsive-layout {
  @include flex-between;

  @include respond-to(md) {
    flex-direction: column;
  }
}

.label {
  @include truncate;
}
```

### Theming

The dark theme is the default (applied to `:root` and `[data-theme="dark"]`).
To switch themes at runtime:

```typescript
document.documentElement.dataset.theme = 'light'; // or 'dark'
```

### Utility Classes

Common layout and spacing utilities are available as classes. See
`src/styles/_utilities.scss` for the full list:

```html
<div class="flex items-center gap-md p-lg">
  <span class="text-sm truncate">Label</span>
</div>
```

---

## Platform-Specific Notes

### Linux — Wayland

Screen capture on Wayland uses the XDG Desktop Portal (`org.freedesktop.portal.ScreenCast`).
PipeWire must be running. On X11, the app falls back to X11-based capture.

### macOS — ScreenCaptureKit

macOS 12.3+ uses ScreenCaptureKit for capture. The app must be signed with the
appropriate entitlement (`com.apple.security.screen-capture`). On older macOS,
the app falls back to CGWindowListCreateImage.

The user will be prompted for screen recording permission on first launch.

### Windows

Windows capture uses the Desktop Duplication API (DXGI). No special permissions
are required, but the app must run in a desktop session (not a service).

---

## License

Screen Dream is licensed under the **GNU General Public License v3.0** (GPLv3).
See the [LICENSE](./LICENSE) file for the full text.

### FFmpeg

Screen Dream invokes FFmpeg as a subprocess at runtime for encoding and muxing.
FFmpeg itself is licensed under LGPL 2.1+ (or GPL depending on build flags).
Screen Dream does **not** link against FFmpeg libraries — it spawns the `ffmpeg`
binary, so LGPL requirements regarding dynamic linking do not apply directly.
Users are responsible for ensuring their FFmpeg installation complies with its
license terms.
