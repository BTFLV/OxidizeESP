# Architecture

OxidizeESP consists of a Rust core, a thin Tauri integration layer and a Svelte frontend.

```text
┌──────────────────────────────────────────────────────────────┐
│ Frontend (apps/desktop/src)                                  │
│ Svelte 5 views, stores and components                        │
└───────────────▲───────────────────────────────┬──────────────┘
      job / monitor events (IPC channels)       │ typed commands
┌───────────────┴───────────────────────────────▼──────────────┐
│ Tauri layer (apps/desktop/src-tauri)                         │
│ Command handlers, permissions, settings and log locations    │
└───────────────▲───────────────────────────────┬──────────────┘
                │                               │
┌───────────────┴───────────────────────────────▼──────────────┐
│ oxidizeesp-core (crates/oxidizeesp-core)                     │
│ Services · device sessions · jobs · operations · monitor     │
│ partition tables · images · backups · settings               │
│ hardware::Backend ── EspflashBackend (espflash, serialport)  │
│                   └─ MockBackend (feature "mock", tests)     │
└──────────────────────────────────────────────────────────────┘
```

## Rust core

`oxidizeesp-core` has no dependency on Tauri or the frontend and is tested on its own.

| Module | Responsibility |
| --- | --- |
| `hardware` | The `Backend`, `FlashLink` and `SerialLink` traits. `EspflashBackend` implements them with espflash's `Flasher` and `Connection`; `MockBackend` simulates devices with in-memory flash. |
| `service` | `Services`, the entry point used by the UI layer. Runs jobs with exclusive device access, manages monitors and cancellation. |
| `session` | Device sessions and exclusive leases (see below). |
| `ops` | User-level operations: probe, reset, flash ELF, write binary, read flash, erase, checksum, read partition table. |
| `job` | Job events, state transitions, cancellation tokens and progress throttling. |
| `image` | ELF inspection and ESP-IDF image generation through `IdfBootloaderFormat`. |
| `partitions` | Parsing, validation, layout checks and conversion using `esp-idf-part`. |
| `backup` | Backup metadata sidecars and verification. |
| `monitor` | Serial monitor loop, UTF-8 reassembly, line assembly and ELF address resolution. |
| `region`, `fsutil` | Address parsing and region validation; atomic file writes and hashing. |
| `error` | The `Error` type with a `kind` used by the UI, and translation of espflash and serial errors. |
| `settings` | Persistent settings stored as JSON. |

Every value received from the frontend is validated in the core before hardware is touched: addresses,
lengths and overflows (`region`), file existence, size and type (`fsutil`, `image`), alignment and flash size,
chip compatibility of images and ESP image headers, and an explicit `confirmed` flag on destructive requests.

### Use of espflash

All device communication uses the public espflash library API (`default-features = false`,
`features = ["serialport"]`). The `cli` module of espflash is not used because it has no stability guarantees
and depends on a terminal. Where the CLI composes several library calls, the core does the same:

- Connecting follows `espflash::cli::connect`: open the native port at 115200 baud, create a `Connection` with the
  selected reset strategies and call `Flasher::connect`.
- ELF flashing builds an `IdfBootloaderFormat` and writes its segments with `Flasher::write_bins_to_flash`, so
  the layout can be shown and validated before writing.
- Backups call `Flasher::read_flash` for consecutive chunks of 256 KiB. The library reads a whole region into
  memory and reports no progress; chunking keeps memory bounded, provides progress and allows cancellation.
  Each chunk is still checked by espflash with an MD5 digest.
- Region erases are issued in 64 KiB steps for progress and cancellation; full erases use `Flasher::erase_flash`.
- Resetting without entering the bootloader uses `Connection::reset`, which applies the same DTR/RTS sequence
  espflash uses after flashing.
- The espflash monitor is part of the CLI, so the monitor is implemented in the core with `serialport` and
  `addr2line`, following espflash's approach to address resolution.

## Device sessions

A session represents one physical device. It records the port, the USB identity (VID, PID and serial number)
and, once identified, the verified chip information including the MAC address.

- Hardware activities acquire a `Lease` on the session. A session has at most one lease, so the monitor, a flash
  write and a backup can never use the same port at the same time. Leases are released when dropped, including
  on errors and panics.
- Distinct devices have independent sessions and can be used concurrently.
- Port enumeration never opens a port. After each enumeration, sessions whose port vanished are marked as
  disconnected, or moved to a new port that reports the same USB identity.
- Before any operation, the connected chip is compared with the identified one (chip type and MAC address) so
  that a board swapped on the same port is not written to by mistake.

## Jobs

Long-running operations run on blocking worker threads. Each emits ordered events through a Tauri channel:

`started` → `stage` / `progress` / `log` … → `completed` | `failed` | `cancelled`

Progress is only reported when espflash or the chunked implementation provides real values. Cancellation is
cooperative and only offered for operations that can stop between steps (backups, region and partition erases).
Flash writes are not cancellable; if an operation fails or is cancelled after flash was modified, the terminal
event says so explicitly.

## Frontend

The frontend is a Svelte 5 single-page application using runes-based stores:

- `stores/devices` keeps ports and sessions in sync through the `devices-changed` event and periodic passive
  port enumeration.
- `stores/job` holds a `JobRunner` per view; a pure reducer applies job events and is unit-tested.
- `stores/monitor` stores received lines in a fixed-size ring buffer; `LogView` renders only the visible rows.
- Accessible primitives (dialogs, selects, switches, tooltips) are built on bits-ui and styled with Tailwind CSS
  using theme tokens for the dark and light themes.

TypeScript types in `src/lib/bindings` are generated from the Rust types by ts-rs when running `cargo test`;
CI fails when they are out of date.

## Security

- Commands are registered as individual Tauri permissions and granted explicitly in
  `src-tauri/capabilities/main.json`. The frontend has no general file system or shell access; file paths come
  from native dialogs or drag and drop and are validated by the core.
- The content security policy only allows local resources and IPC.
- OxidizeESP makes no network requests and collects no telemetry. Logs stay in the application's log folder.
