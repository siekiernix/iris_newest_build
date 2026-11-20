# New iris-gui framework: shared MVC core plus native shells

Status: **draft, no code yet** (2026-11-20, branch `iris-newgui`). Mock-ups under
review; decisions D1-D10 are the user's answers of 2026-11-20. Open points are
listed at the end.

Reference: `../rusty-backup` (CONTRIBUTING.md "Architecture policy (MVC)",
`docs/native_frontends_plan.md`, `docs/appkit_shell_plan.md`, `frontends/appkit`,
`frontends/mac-screens`). Same layering, adapted for an emulator with a live
display.

## Decisions

| # | Decision |
|---|---|
| D1 | egui stays and ships. It is rewired onto the new controllers slice by slice, so every controller is validated by a working app before AppKit uses it. |
| D2 | One machine runs at a time. The library lists all of them; the worker (`handle.rs`) and the cwd-per-machine rule are unchanged. Nothing in the controller API may assume a single session forever: session state hangs off a `SessionController`, not the app. |
| D3 | Layout follows VMware Workstation: library on the left, the selected machine on the right (summary when stopped, guest display when running). The display can be detached into its own window. |
| D4 | macOS presenter: a `CAMetalLayer` view fed by the existing `FrameSink` damaged bands (`replaceRegion` per band). Gives vsync, integer/linear scaling and a later path to a shared-device wgpu presenter. |
| D5 | Keep every current indicator; add per-disk activity lights and the others in "Status bar" below. |
| D6 | Shells are Rust over the platform bindings (`objc2` / `objc2-app-kit`), no Swift, no C ABI. |
| D7 | Every shell has every feature. Each lays it out in native widgets; macOS gets a complete menu bar (see "Menu bar"). |
| D8 | Adding an SGI machine (Octane, O2, Onyx...) must not need shell code: the settings screens are built from the core hardware tables plus one per-profile traits table in the shared crate. |
| D9 | The first macOS release is a full implementation (parity with egui). The App Store build waits for more platforms. |
| D10 | All new code lives under `iris-gui/`. Exceptions need the user's OK (see "Outside iris-gui"). |
| D11 | Controls are icons with tooltips, not words (2026-11-20). The controller gives each action an icon *meaning* (an enum) and its tooltip text; the shell picks the glyph. macOS uses SF Symbols (`NSImage(systemSymbolName:)`, macOS 11+). SF Symbols may not ship on other platforms, so GTK4/Win32/egui use one set of our own SVGs under `iris-gui/assets/icons/`, which is also the macOS fallback below 11. The accessibility label is the tooltip. |
| D12 | Light and dark appearance, following the system. Shells use the toolkit's semantic colours (AppKit: `NSColor` system colours, no hard-coded hex); the guest display area stays dark in both. |
| D13 | Build features are shown: a "This build" chip row on the summary page (present and absent, from `iris::build_features`), the executor chip in the status bar, and the full list in Help > Diagnostics. |
| D14 | No pause/resume: the core has none, so it is not offered. |
| D15 | Saved states: the core already takes a name (`Cmd::SaveState(String)` / `RestoreState`), so each machine gets a list of its named states (summary page chips, toolbar "Saved states", Machine menu). No snapshot tree. |
| D16 | The egui shell keeps its current look. It is rewired onto the controllers (D1) but does not take the Workstation layout; phase 3 below is dropped. |
| D17 | macOS floor: 10.13, the same as today's egui bundle (`scripts/dist/macos-bundle.sh`) and what `objc2-app-kit` and Metal allow without workarounds. Features newer than that are runtime-checked (SF Symbols on 11+). Revisit only if a build problem forces it. |
| D18 | Core activity counters (per-SCSI-id read/write, audio active, serial activity) are allowed, on **their own branch off `main`** with their own PR, not on `iris-newgui`. The status bar degrades to "no light" for a counter the core does not report. |

## Layout on disk

```
iris-gui/
  Cargo.toml, src/            the egui shell (as today), now a view
  shared/                     crate iris-gui-shared: no toolkit dependencies
    src/model/                worker handle, machine store, settings, summary, netplan, i18n
    src/controller/           one controller per task; Action / Effect; keys, ids, labels
  frontends/
    appkit/                   crate iris-gui-appkit (objc2), macOS
    gtk4/, win32/             later
  locales/, assets/           shared by every shell
  scripts/check-boundaries.sh shells may import only iris_gui_shared::controller
```

`shared/` is a path dependency of `iris-gui`, so Cargo adds it to the workspace
with no root `Cargo.toml` edit. Shell crates need one line each in the root
`members` (or their own `[workspace]`, which rebuilds the core a second time).

## Layers

- **Model** (`shared/src/model/`): what exists today without egui in it:
  `handle.rs` (`Cmd`/`Evt`/`Status`, the 64 MB worker), `machines.rs`,
  `summary.rs`, `settings.rs`, `netplan.rs`, `netfix.rs`, `safe_stop.rs`,
  `ram.rs`, `single_instance.rs`, `i18n.rs`, `macos_sandbox.rs`,
  `capture_access.rs`, and the frame sink half of `framebuffer.rs`.
- **Controller** (`shared/src/controller/`): rusty-backup's contract.
  `apply(Action) -> Vec<Effect>`, `tick() -> Vec<Effect>`, plain read
  accessors, exhaustive enums matched with no `_` arm, stable widget ids used
  as accessibility identifiers, key map as data (`Primary` = Cmd/Ctrl),
  labels from the controller through Fluent. No toolkit, no raw threads.
- **View**: egui (`iris-gui/src`), AppKit, later GTK4 and Win32. Renders,
  maps input to actions, fulfils effects (file pickers, confirms, alerts,
  clipboard, open URL, elevation prompt).

`Notify::wake` replaces polling: egui `request_repaint`, AppKit
`dispatch_async` to the main queue (always queued, never inline). The frame
path keeps its own wake (`FrameSink`), separate from controller wakes.

### Controllers

| Controller | Owns | Comes from |
|---|---|---|
| `AppController` | library (discover, select, new, rename, delete, duplicate), active machine, dirty/autosave, toasts, quit and close interception (CHD sync), prefs, language | `main.rs`, `machines.rs` |
| `SessionController` | start, stop, halt IRIX, save/restore state, screenshot, run state, every status-bar indicator, CD load/eject/remount, COW commit/reset, safe-stop rules | `main.rs`, `handle.rs`, `scsi_menu.rs`, `safe_stop.rs` |
| `DisplayController` | capture/release rules and chords, fullscreen, detach, scaling mode, the "powered off" overlay state | `input.rs`, `framebuffer.rs`, `main.rs` |
| `SettingsController` | the machine config as a form schema (below), validation, refit on profile change, missing-disk preflight | `config_ui.rs`, `extra_ui.rs`, `summary.rs` |
| `NetworkController` | NAT/PCAP mode, subnet presets and overlap warnings, port forwards (live), "check networking" diagnosis, PCAP elevation | `netplan.rs`, `netfix.rs`, `capture_access.rs` |
| `SerialConsoleController` | console port routing, scrollback, input | `serial_console.rs` |
| `BenchController` | benchmark run, progress, results | `bench_ui.rs` |
| `DebugController` | monitor port, perfetto start/stop, diagnostics text | `config_ui.rs` Debug tab |
| `CameraTestController` | VINO/IndyCam source test | `camera_test.rs` |
| dialogs | new-machine wizard, rename, about, diagnostics | `dialogs/` |

### Settings as data (D8)

`SettingsController` hands every shell a list of sections, each a list of
fields; shells implement the field kinds once.

```rust
pub struct Section { pub id: SectionId, pub title: String, pub fields: Vec<Field> }
pub enum FieldKind { Choice{options, selected}, Toggle(bool), Text{value, validator},
                     Number{value, min, max, unit}, Path{value, pick: PickSpec},
                     BankGrid(..), DiskTable(..), PortForwardTable(..), Note(Severity, String) }
pub struct Field { pub id: FieldId, pub label: String, pub help: Option<String>,
                   pub kind: FieldKind, pub enabled: Enabled /* with the reason */ }
```

Options come from the core tables (`MachineProfile::ALL`, `cpus()`,
`memory_banks()`, `bank_sizes()`, `GraphicsBoard::ALL`), never from a shell.
What the core does not say goes in **one** `ProfileTraits` table in the shared
crate (NVRAM kind and whether reset/seed applies, EEPROM MAC, console port,
storage bus, fixed graphics, which sections apply), replacing the
`match profile` arms scattered through `config_ui.rs` / `extra_ui.rs`
(`clanker/gui/profile-menus-must-use-core-hardware-tables.md`). Adding a
machine is then: core profile, one traits row, one sidebar icon.

## The window (D3)

- **Toolbar**: power on / shut down guest (halt) / power off, pause-less
  restart, snapshot (save/restore state), screenshot, CD, fullscreen, detach
  display, settings.
- **Library** (left, collapsible): machines grouped by family, each with an
  icon, name, profile and a run-state dot; context menu (start, settings,
  rename, duplicate, delete, show in Finder); "+" for New Machine.
- **Content** (right): stopped: summary page (preview thumbnail of the last
  screenshot, Power On, hardware list with one row per device that opens its
  settings section, missing-disk warnings). Running: the guest display.
- **Status bar** under the content.
- Settings open as a sheet over the window (Workstation's hardware list on
  the left, the section's form on the right); live-editable fields stay
  enabled while running, the rest say "takes effect at next power on".

### Status bar (D5)

Left: run state (Stopped / PROM / Booting / Running / Halted / Powered off),
machine, profile, CPU (plus "CPU change pending"), executor badge
(interpreter / jitv2 / lightning), unsaved mark, toast.

Right, one clickable item each: per-disk lights (read/write; COW dirty-sector
badge; CHD sync pending), CD-ROM (disc loaded, activity; click to load/eject),
network (NAT or PCAP, activity, guest IP, fault badge that opens the
diagnosis), serial console (activity; click opens it), audio (active/muted),
camera (in use), input capture (captured + release chord), display mode and
scale, MIPS, kernel Hz.

Per-disk lights, audio and serial activity need counters the worker `Status`
does not have today (see "Outside iris-gui").

### Menu bar (macOS, D7)

Every item runs a controller action and greys out with it; shortcuts live in
the controller key map.

- **IRIS**: About, Check for Updates, Settings (app prefs, language), Quit.
- **File**: New Machine, Open Machine Folder, Import Config, Duplicate,
  Rename, Delete, Show in Finder, Close.
- **Edit**: standard items; Copy Screenshot; Paste Text to Guest.
- **View**: Show/Hide Library, Show/Hide Status Bar, Enter Full Screen, Detach
  Display, Scale (Fit, 1x, 2x, Integer), Filter (Nearest, Linear), Serial
  Console, Log.
- **Machine**: Power On, Shut Down Guest (halt), Power Off, Restart, Save
  State, Restore State, Take Screenshot, Capture Input, Send Key (F11,
  Ctrl+Alt+Del...), Settings.
- **Devices**: one submenu per CD-ROM (Insert Disc, Eject, Remount in IRIX),
  per disk (Apply Changes, Roll Back, Synchronize), Network (mode, Check
  Networking, Port Forwards), Camera, Audio.
- **Tools**: Benchmark, Camera Test, Debug (monitor, perfetto), Diagnostics.
- **Window**, **Help** (guide, debug.md, report an issue).

## Phases

Small commits per slice; egui builds and its tests pass after each.

0. **Scaffold**: `shared/` crate, `Controller` trait, `Effect`, `Notify`,
   `ids`, `keys`, `labels`, boundary script. No behaviour change.
1. **Move the model down**: the toolkit-free modules move to
   `shared/src/model`; egui imports them from there.
2. **Controllers, egui rewired per slice**: App (library) -> Session ->
   Display -> Settings (schema + `ProfileTraits`) -> Network -> Serial ->
   Bench/Debug/Camera -> dialogs. Exit: `iris-gui/src` holds no domain logic
   and the boundary script reports 0.
3. ~~egui takes the Workstation layout~~ dropped (D16); egui keeps its look.
4. **AppKit shell**: 0 app, window, menu, wake, library; 1 summary; 2 Metal
   presenter, physical-key input, capture, fullscreen, detach; 3 status bar;
   4 settings sheet (field kinds); 5 disks, CD, COW, sync-on-quit; 6 network;
   7 serial console; 8 benchmark, debug, camera; 9 bundle, signing,
   notarized DMG, own bundle id beside the egui app.
5. **Parity check and release** as a preview beside egui; then GTK4 and
   Win32 over the same controllers.

Testing: controller unit tests (no window); the existing egui headless tests;
later a flow driver over the accessibility ids, as rusty-backup's tier 8.

## Outside iris-gui (needs the user's OK, D10)

- Root `Cargo.toml`: one `members` line per shell crate (approved
  2026-11-20). Each shell gates its dependencies and code on its own
  `cfg(target_os = ...)`, so `cargo build --workspace` on another OS builds an
  empty binary. `iris-gui/shared` needs no line: a path dependency inside the
  workspace directory is a member automatically.
- Core counters for the new lights: per-SCSI-id read/write activity, audio
  active, serial activity. Approved (D18), on a separate branch off `main`.
  Without them only COW write activity can be inferred (from `dirty_cow`
  deltas).
- Human docs when it ships: `iris-gui-README.md`, `FEATURES.md`,
  `CHANGELOG.md`; the release workflow for the new bundle.

## Open

- Where the saved-state files of a machine are listed from (a directory scan
  of the machine folder vs. an index the controller keeps).
