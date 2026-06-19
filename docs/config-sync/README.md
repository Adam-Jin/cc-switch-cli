# Config Sync Architecture

本文档记录当前代码里 MCP、Skill、机器作用域和 WebDAV 同步的实际存储/投影方式。

结论先放前面：

- SQLite `~/.cc-switch/cc-switch.db` 是大多数配置元数据的持久化主库。
- MCP 的统一定义存在 DB 的 `mcp_servers` 表里；各 agent 的配置文件只是 live projection。
- Skill 的原始目录内容不在 DB 里；原始内容在 `~/.cc-switch/skills/`，DB 只存安装记录、启用状态、仓库和 selector。
- WebDAV 同步上传的是 `db.sql + skills.zip + manifest.json`，不是直接上传 SQLite `.db` 文件。
- 当前 WebDAV 会读取并保存 manifest 的 ETag，但上传没有使用 `If-Match` / `If-None-Match`，所以它还不是严格的乐观锁。
- Schema 版本使用 SQLite `PRAGMA user_version`，当前没有独立的 `schema_migrations` 表。

## Mental Model

```text
SQLite DB / SSOT metadata
  - providers, prompts, mcp_servers, skills, skill_repos, settings ...
  - source for app config state

~/.cc-switch/skills/
  - raw Skill directories and SKILL.md files
  - source for skill file content

live config files
  - Claude / Codex / Gemini / OpenCode config files
  - generated or reconciled projection
  - not the canonical store

WebDAV remote
  - transport snapshot
  - db.sql + skills.zip + manifest.json
```

数据流大致是：

```text
startup
  SQLite DB -> MultiAppConfig in memory

MCP edit
  MultiAppConfig -> state.save() -> SQLite DB
  then selector/app flags -> live config write/remove

Skill edit
  SQLite DB metadata + ~/.cc-switch/skills raw files
  then selector/app flags -> app skills dir symlink/copy/remove

WebDAV upload
  SQLite snapshot -> db.sql
  ~/.cc-switch/skills -> skills.zip
  hashes/sizes -> manifest.json

WebDAV download
  manifest validation -> artifact hash/size validation
  restore skills.zip
  import db.sql through temp SQLite DB
```

## Local Paths

Main local data:

- App config dir: `~/.cc-switch`, overridable by `CC_SWITCH_CONFIG_DIR`.
- Main DB: `~/.cc-switch/cc-switch.db`.
- App settings: `~/.cc-switch/settings.json`.
- Skill raw content: `~/.cc-switch/skills/`.
- Legacy config files `config.json` and `skills.json` may be migrated into DB and archived.

Live MCP targets:

- Claude: default `~/.claude.json`, root field `mcpServers`.
- Codex: `~/.codex/config.toml`, top-level `[mcp_servers]`.
- Gemini: `~/.gemini/settings.json`, root field `mcpServers`.
- OpenCode: `~/.config/opencode/opencode.json`, root field `mcp`.

Live Skill targets:

- Claude: `~/.claude/skills/`.
- Codex: `~/.codex/skills/`.
- Gemini: `~/.gemini/skills/`.
- OpenCode: `~/.config/opencode/skills/`.
- OpenClaw exists as an app type, but current Skill sync does not support it.

Several target dirs can be overridden from `settings.json`. MCP live writes are also gated by `sync_policy::should_sync_live`: if a target app does not look initialized, the MCP projection skips writing/deleting live files instead of creating them.

## MCP Storage

Runtime API uses a unified `McpServer` model:

```text
McpServer
  id
  name
  server: JSON MCP spec
  apps: enabled flags for claude/codex/gemini/opencode/hermes
  description/homepage/docs/tags
  machine_selector
```

Persistent storage is the SQLite `mcp_servers` table:

```text
mcp_servers
  id TEXT PRIMARY KEY
  name TEXT
  server_config TEXT      -- JSON
  description TEXT
  homepage TEXT
  docs TEXT
  tags TEXT               -- JSON array
  enabled_claude BOOLEAN
  enabled_codex BOOLEAN
  enabled_gemini BOOLEAN
  enabled_opencode BOOLEAN
  enabled_hermes BOOLEAN
  machine_selector TEXT   -- JSON MachineSelector
```

Startup loads DB into `AppState.config`:

```text
Database::get_all_mcp_servers()
  -> MultiAppConfig.mcp.servers
```

MCP changes write memory first, then persist:

```text
McpService::upsert_server / toggle_app / delete_server
  -> mutate AppState.config
  -> state.save()
  -> persist_multi_app_config_to_db()
  -> save_mcp_server/delete_mcp_server
```

## MCP Live Projection

Live projection is not the source of truth. It is a reconciliation step based on:

```text
enabled for app && machine_selector matches current machine labels
```

If true, the server is written to that app's live config. If false, any existing entry for that server is removed from that app's live config.

Per-app behavior:

- Claude writes only `mcpServers` in `~/.claude.json`, preserving other JSON fields.
- Codex edits only the top-level `[mcp_servers]` table in `~/.codex/config.toml`; it also removes the old/wrong `[mcp.servers]` shape if present.
- Gemini writes only `mcpServers` in `~/.gemini/settings.json`, preserving other JSON fields.
- OpenCode writes/removes one entry under `mcp` in `opencode.json`.

## Skills Storage

There are two distinct parts:

```text
Skill metadata and enablement
  SQLite DB:
    skills
    skill_repos
    settings.skill_sync_method
    settings.skills_ssot_migration_pending

Skill raw content
  ~/.cc-switch/skills/<directory>/
    SKILL.md
    other skill files
```

So the DB does not store the raw Skill directory content. It stores records such as:

```text
InstalledSkill
  id
  name
  description
  directory
  repo_owner/repo_name/repo_branch
  readme_url
  apps
  machine_selector
  installed_at
```

The `services/skill.rs` file still has a `SkillsIndex` type and comments mentioning `skills.json`, but the current `load_index()` / `save_index()` path assembles and persists that index through SQLite DB plus settings. `skills.json` is treated as a legacy migration input in `store.rs`, then archived.

## Skills Live Projection

Skill sync uses the raw SSOT directory as source:

```text
~/.cc-switch/skills/<directory>
  -> app skills dir
```

Sync method:

- `auto`: try symlink first, fallback to copy.
- `symlink`: require symlink.
- `copy`: copy the directory.

Like MCP, a Skill is projected only when:

```text
enabled for app && machine_selector matches current machine labels
```

If not active on the current machine/app, the managed app directory entry is removed.

Uninstall removes:

- the app-dir projection from supported apps,
- the raw `~/.cc-switch/skills/<directory>` directory,
- the DB `skills` row.

## Machine Labels And Selectors

The code already implements the "labels + selector" abstraction.

Current machine labels:

```text
auto labels
  os:<std::env::consts::OS>
  arch:<std::env::consts::ARCH>

manual labels
  settings["machine_labels"]
  local-only; excluded from WebDAV sync
```

Selectors live with each MCP/Skill record and are synced:

```rust
MachineSelector {
  include: Vec<String>,
  exclude: Vec<String>,
}
```

Matching rule:

```text
(include is empty OR any include label is present)
AND
(no exclude label is present)
```

Empty selector means "active on all machines", which preserves backward compatibility.

This is the right boundary:

- "what this item requires" is shared and synced with the item,
- "what machine am I" is local and not synced.

## WebDAV Snapshot Layout

Current WebDAV v2 layout:

```text
{remote_root}/v2/db-v6/{profile}/
  db.sql
  skills.zip
  manifest.json
```

There is also a legacy fallback for:

```text
{remote_root}/v2/{profile}/
```

and a v1 migration path.

Manifest fields:

```text
format
version
dbCompatVersion
deviceName
createdAt
artifacts:
  db.sql:
    sha256
    size
  skills.zip:
    sha256
    size
snapshotId
```

`snapshotId` is computed from artifact hashes. It identifies the content snapshot, not a distributed lock.

## WebDAV Upload

Upload flow:

```text
load settings
ensure remote directories
build local snapshot:
  Database::export_sql_string_for_sync() -> db.sql
  zip ~/.cc-switch/skills -> skills.zip
  build manifest with size/sha256
PUT db.sql
PUT skills.zip
PUT manifest.json last
GET manifest back and compare bytes
HEAD manifest and store ETag best-effort
cleanup v1 remote best-effort
```

Manifest is uploaded last so a reader that sees the new manifest should be able to fetch the referenced artifacts.

Current transport `put_bytes` is plain HTTP PUT. It does not send `If-Match`, `If-None-Match`, or WebDAV `LOCK`.

## WebDAV Download And Restore

Download flow:

```text
find remote manifest
validate format/version/dbCompatVersion
download db.sql and skills.zip
verify each artifact size and sha256
acquire restore mutation guard
ensure restore is allowed
apply snapshot
store manifest hash and ETag best-effort
cleanup v1 remote best-effort
```

`apply_snapshot` intentionally restores skills and DB as one logical operation:

```text
backup current skills dir
restore skills.zip into ~/.cc-switch/skills
import db.sql for sync
if DB import fails:
  restore previous skills backup
```

## DB Sync Format

The synced DB artifact is SQL text, not raw SQLite bytes.

Export uses:

```text
Database::snapshot_to_memory()
  -> SQLite backup API copy from live connection to in-memory DB
  -> dump schema + INSERT statements
```

For WebDAV sync, export applies a preservation policy:

- Do not export local runtime/log tables:
  - `proxy_request_logs`
  - `stream_check_logs`
  - `proxy_live_backup`
  - `usage_daily_rollups`
- Clear/reset remote-unsafe health state:
  - `provider_health`
- Do not sync local-only setting keys:
  - `proxy_runtime_session`
  - `machine_labels`
- Neutralize remote-unsafe proxy fields during export:
  - `proxy_enabled`
  - `listen_address`
  - `listen_port`
  - `enabled`
  - `live_takeover_active`

On import, the local values for `proxy_enabled`, `listen_address`, `listen_port`, and `enabled` are restored by `app_type`. `live_takeover_active` is reset through the exported snapshot rather than restored from the previous local DB.

This is why `machine_labels` can be used locally without leaking to other machines.

## DB Import Safety

Import does not execute downloaded SQL directly against the main DB.

Actual restore sequence:

```text
validate SQL header
backup current DB file
snapshot local DB if sync policy is active
create temp SQLite DB
execute downloaded SQL in temp DB
create missing tables/indexes
apply schema migrations
validate basic state
restore local-only overlay into temp DB
SQLite Backup API: temp DB -> main DB
```

This extra staging matters because it prevents several bad states:

- corrupt or non-CC-Switch SQL cannot touch the main DB,
- half-imported SQL cannot leave the main DB partially overwritten,
- older snapshots can be migrated before they become the active DB,
- local runtime state such as machine labels and proxy listening state survives sync,
- there is a DB file backup before replacement.

## Schema Versioning

Current schema version:

```rust
pub(crate) const SCHEMA_VERSION: i32 = 11;
```

Version storage is SQLite native:

```sql
PRAGMA user_version;
```

There is no `schema_migrations` table in the current implementation.

Migration flow:

```text
Database::init()
  create_tables()
  read PRAGMA user_version
  if old version: backup DB file
  apply_schema_migrations()
    SAVEPOINT schema_migration
    loop version -> SCHEMA_VERSION
    set PRAGMA user_version after each step
    RELEASE or ROLLBACK savepoint
```

Version 10 -> 11 adds `machine_selector` to `mcp_servers` and `skills`.

## Conflict Boundaries

Current WebDAV state stores:

```text
last_remote_etag
last_local_manifest_hash
last_remote_manifest_hash
```

But these fields are only status/bookkeeping today. They are not used to reject conflicting uploads.

Current conflict behavior:

- If two machines upload concurrently, last writer can win.
- Manifest-last upload makes snapshots easier to read consistently, but it does not prevent overwriting a newer manifest.
- Downloading remote before upload can overwrite local changes unless the application adds a merge/conflict workflow above this layer.

A safer future design would use:

```text
pre-upload:
  HEAD manifest.json
  compare remote ETag/hash with last known remote

if unchanged:
  PUT artifacts
  PUT manifest with If-Match: <old etag>

if changed:
  stop and show conflict:
    local snapshot
    remote snapshot
    choose upload / download / fork / manual merge
```

For providers/MCP/Skill metadata, a true merge would need record-level timestamps or revision IDs; current `db.sql` snapshot is closer to whole-state replacement with local overlays.

## Important Source Files

- `src-tauri/src/store.rs`: DB <-> `MultiAppConfig`, legacy migration, `state.save()`.
- `src-tauri/src/database/schema.rs`: tables, schema version, migrations.
- `src-tauri/src/database/backup.rs`: SQL dump/import, sync preservation policy, safe restore.
- `src-tauri/src/database/dao/mcp.rs`: MCP DB CRUD.
- `src-tauri/src/database/dao/skills.rs`: Skill metadata DB CRUD.
- `src-tauri/src/services/mcp.rs`: MCP business flow and selector-based reconciliation.
- `src-tauri/src/services/skill.rs`: Skill SSOT directory, metadata index, app sync.
- `src-tauri/src/services/webdav_sync/mod.rs`: WebDAV v2 protocol, manifest, upload/download.
- `src-tauri/src/services/webdav_sync/archive.rs`: `skills.zip` packaging and restore.
- `src-tauri/src/services/webdav.rs`: raw WebDAV HTTP transport.
- `src-tauri/src/machine.rs`: machine labels.
- `src-tauri/src/app_config.rs`: `MachineSelector`, MCP/Skill app flags and models.
- `src-tauri/src/sync_policy.rs`: live-write gating.
- `src-tauri/src/claude_mcp.rs`, `src-tauri/src/mcp.rs`, `src-tauri/src/gemini_mcp.rs`, `src-tauri/src/opencode_config.rs`: per-app live MCP file formats.
