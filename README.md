# Screen Dream

Cross-platform screen recorder and video editor built with Tauri 2, Preact, and FFmpeg.

## Quick Start

**Prerequisites:** Rust (stable), Node.js 18+, pnpm, FFmpeg

```bash
git clone <repo-url> screen-dream
cd screen-dream
pnpm install
pnpm tauri dev
```

For platform-specific dependencies and detailed setup, see [Development.md](./Development.md).

## Build

```bash
pnpm tauri build
```

## License

[GPLv3](./LICENSE) — Screen Dream is free software.

FFmpeg is used at runtime under the LGPL/GPL. See Development.md for details.
