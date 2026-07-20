# ClipFlow

Modern lightweight Windows clipboard history tool. Tauri v2 + Vanilla TS/CSS, Raycast-style floating panel.

See `CONTEXT.md` for the domain glossary and behavior spec.

## Build

Frontend assets are **embedded into the exe at compile time** from `dist/` (Tauri `frontendDist`). `dist/` must be built **before** compiling Rust, otherwise the exe embeds stale assets. Use the single command:

```bash
npm install
npm run build:app        # = npm run build (tsc + vite → dist/) + cargo build --release
```

Output: `src-tauri/target/release/clipflow.exe`

## Portable deployment

Copy only `clipflow.exe` to the target folder. No external `dist/` needed — assets are embedded. `clipflow.config.json` is auto-created next to the exe on first run.

```
D:\Tools\ClipFlow\
├── clipflow.exe
└── clipflow.config.json   (auto-generated)
```

Run hidden (tray only): `clipflow.exe --hidden`

## Dev

```bash
npm run tauri dev
```

## Usage

- `Ctrl+Shift+V` — toggle the history panel (configurable in Settings)
- `Esc` / click outside / pick a clip — dismiss the panel
- Tray icon (right-click): Pause Monitoring, Settings, About, Quit
