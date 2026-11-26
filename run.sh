#!/bin/sh
# 构建并启动 SuperLink 开发版（共用默认工作区）
set -e
cd "$(dirname "$0")"
exec python3 tools/dev_run.py "$@"
