![Azusa Minecraft Launcher](assets/brand/readme-header.png)

# Azusa Minecraft Launcher

Azusa Minecraft Launcher (AZULC) is a next-generation lightweight, high-performance Minecraft launcher and technology validation platform. It is native and all-Rust, built with Iced around a borderless English pixel interface, isolated instances, and one observable state machine for the complete Minecraft + mod-loader installation flow.

## Current features

- Multiple Microsoft accounts through device-code OAuth, with on-demand launch verification, session reuse, and active-skin avatars.
- Temporary offline test profiles with stable Java-compatible offline UUIDs.
- Minecraft release, snapshot, legacy, and April Fools catalogs.
- Vanilla, Fabric, Forge, and NeoForge installation.
- Live compatible loader-build catalogs with source-aware fallback and automatic selection of the newest stable build.
- Configurable official and mirror download routes with parallel transfers.
- Java discovery and automatic or per-instance runtime selection.
- Per-instance worlds, mods, resource packs, screenshots, settings, and play-time telemetry.
- CurseForge and Modrinth modpack discovery plus local CurseForge, Modrinth, and MultiMC/Prism archive import.
- Per-instance CurseForge and Modrinth downloads for mods, resource packs, shaders, and data packs, including required dependencies.
- Concurrent per-instance launch monitoring with independent readiness, live logs, failure reporting, and open-log actions.
- A cancellable and retryable install pipeline with live stage, file, byte, speed, and processor output.

## Run

Rust 1.88 or newer is required.

```powershell
cargo run
```

Copy `.env.example` to `.env` and configure the required credentials:

```dotenv
AZULC_CURSEFORGE_API_KEY='<your approved key>'
AZULC_MICROSOFT_CLIENT_ID='<Azusa Minecraft Launcher application client ID>'
```

AZULC loads `.env` automatically. Process-level environment variables take precedence.

Build an optimized binary:

```powershell
cargo build --release
```

Application data uses the operating system's application-data location. On Windows this is normally `%APPDATA%\AZULC\AZULC`:

```text
minecraft/                shared versions, libraries, assets, and installers
instances/<uuid>/         isolated game directories, content, and launch logs
state.json                accounts/tokens, instances, download policy, and settings
```

## Architecture

The UI and launcher operations are intentionally separate:

```text
src/ui/*                                stateless Iced pages and brand components
src/app/{launch,install,instance}/*     feature-scoped request state and orchestration
src/app/{message,update,navigation}.rs  root event dispatch and cross-feature coordination
src/domain.rs                           serializable accounts, instances, settings, and pipeline types
src/services/auth/*                     account authentication and profile retrieval
src/services/catalog/*                  provider-neutral catalog models, discovery, and resource installation
src/services/providers/*                internal CurseForge and Modrinth protocol clients and DTOs
src/services/download/*                 bounded downloads, mirrors, checksums, and atomic file operations
src/services/content.rs                 local instance-content scanning
src/services/insights.rs                dashboard aggregation, version highlights, and service pings
src/services/loader_catalog.rs          compatible Forge, Fabric, and NeoForge build discovery
src/services/minecraft.rs               Minecraft metadata, planning, verification, and parallel downloads
src/services/modpack.rs                 safe CurseForge/Modrinth/MultiMC archive inspection and overrides
src/services/installer.rs               the continuous loader-aware install state machine
src/services/launcher.rs                arguments, LWJGL/native merging, process output, and readiness monitoring
```

An install does not poll disconnected "download" and "install" jobs. Each stage awaits the preceding future:

```text
resolve Minecraft metadata
  → plan and verify all base files
  → resolve and download the loader
  → write Fabric metadata or run Forge-family processors
  → download verified modpack content and apply overrides
  → finalize the isolated instance
  → complete
```

Every transition emits a `PipelineEvent` into the same root subscription. Cancelling drops that future; Forge-family child processes use `kill_on_drop`. Retrying rebuilds the plan and reuses files that already pass verification.

## Acknowledgements

- [SJMCL](https://mc.sjtu.cn/sjmcl/) — source-code and implementation reference.
- [BMCLAPI](https://bmclapidoc.bangbang93.com/) — Minecraft download mirror provider.

## License

Azusa Minecraft Launcher is licensed under GPL-3.0-or-later.
