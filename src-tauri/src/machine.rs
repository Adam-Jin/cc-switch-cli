//! 机器标签（machine labels）
//!
//! 「当前机器」的有效标签集，用于配合各条目（MCP/Skill）的
//! [`crate::app_config::MachineSelector`] 决定是否在本机生效。
//!
//! 标签分两类：
//! - **自动标签**：运行时从 `std::env::consts` 推导（`os:*`、`arch:*`），不落库、不同步。
//! - **手动标签**：用户在本机自定义（如 `work`、`home`），存 settings 表的
//!   [`MACHINE_LABELS_KEY`]，并通过 [`crate::database`] 的同步策略排除出 WebDAV
//!   同步（见 `database/backup.rs` 的 `SYNC_LOCAL_SETTINGS_KEYS`）。
//!
//! 因此每台机器都能拿到同一份 selector 规则（随条目同步），但「我是谁」这一信息
//! 始终是本机私有的。

use std::collections::BTreeSet;

use crate::database::Database;
use crate::error::AppError;

/// 本机手动标签在 settings 表中的存储键。
///
/// 同时被 `database/backup.rs` 的 `SYNC_LOCAL_SETTINGS_KEYS` 引用，
/// 确保该 key 不参与 WebDAV 同步。
pub const MACHINE_LABELS_KEY: &str = "machine_labels";

/// 当前操作系统标签，如 `os:linux` / `os:macos` / `os:windows`。
pub fn os_label() -> String {
    format!("os:{}", std::env::consts::OS)
}

/// 当前架构标签，如 `arch:x86_64` / `arch:aarch64`。
pub fn arch_label() -> String {
    format!("arch:{}", std::env::consts::ARCH)
}

/// 运行时自动推导的标签（os/arch）。
pub fn auto_labels() -> Vec<String> {
    vec![os_label(), arch_label()]
}

/// 读取本机手动标签（已去重、去空白）。
pub fn get_machine_labels(db: &Database) -> Result<Vec<String>, AppError> {
    let Some(raw) = db.get_setting(MACHINE_LABELS_KEY)? else {
        return Ok(Vec::new());
    };
    let parsed: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
    Ok(normalize_labels(parsed))
}

/// 写入本机手动标签（自动去重、去空白、保持顺序）。
pub fn set_machine_labels(db: &Database, labels: &[String]) -> Result<(), AppError> {
    let normalized = normalize_labels(labels.to_vec());
    let json = serde_json::to_string(&normalized)
        .map_err(|e| AppError::Database(format!("序列化本机标签失败: {e}")))?;
    db.set_setting(MACHINE_LABELS_KEY, &json)
}

/// 当前机器的有效标签集 = 自动标签 ∪ 手动标签。
pub fn current_labels_with(db: &Database) -> Result<BTreeSet<String>, AppError> {
    let mut labels: BTreeSet<String> = auto_labels().into_iter().collect();
    labels.extend(get_machine_labels(db)?);
    Ok(labels)
}

/// 便捷封装：自行打开数据库后计算有效标签集。
///
/// 用于无法直接拿到 `&Database` 的静态调用点（如 Skill 同步流程）。
pub fn current_labels() -> Result<BTreeSet<String>, AppError> {
    let db = Database::init()?;
    current_labels_with(&db)
}

/// 去空白 + 去重，保持首次出现顺序。
fn normalize_labels(labels: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for label in labels {
        let trimmed = label.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
}
