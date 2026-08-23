#!/usr/bin/env bash
# 本地从源码构建 cc-switch 并安装到本机（覆盖已有的可执行文件）。
# 用法：
#   ./build-install.sh            # build --release 并安装到现有 cc-switch 位置
#   ./build-install.sh --debug    # 用 debug 构建（更快，体积大）
#   DEST=~/bin ./build-install.sh # 指定安装目录
set -euo pipefail

# 仓库根（脚本所在目录），保证在哪都能跑
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$REPO_ROOT/src-tauri"
BIN_NAME="cc-switch"

# 构建模式
PROFILE="release"
PROFILE_FLAG="--release"
if [[ "${1:-}" == "--debug" ]]; then
  PROFILE="debug"
  PROFILE_FLAG=""
fi

# 安装目标目录：DEST 环境变量 > 现有 cc-switch 所在目录 > ~/.local/bin
if [[ -n "${DEST:-}" ]]; then
  DEST_DIR="$DEST"
elif command -v "$BIN_NAME" >/dev/null 2>&1; then
  DEST_DIR="$(dirname "$(command -v "$BIN_NAME")")"
else
  DEST_DIR="$HOME/.local/bin"
fi

echo ">> 构建 ($PROFILE) ..."
( cd "$CRATE_DIR" && cargo build $PROFILE_FLAG )

SRC_BIN="$CRATE_DIR/target/$PROFILE/$BIN_NAME"
[[ -f "$SRC_BIN" ]] || { echo "!! 未找到构建产物: $SRC_BIN" >&2; exit 1; }

mkdir -p "$DEST_DIR"
install -m 0755 "$SRC_BIN" "$DEST_DIR/$BIN_NAME"

echo ">> 已安装到: $DEST_DIR/$BIN_NAME"
"$DEST_DIR/$BIN_NAME" --version
