#!/bin/zsh
cd -- "${0:A:h}" || exit 1
if ! command -v python3 >/dev/null 2>&1; then
  print '需要 Python 3 才能启动测试页。'
  read '?按回车关闭…'
  exit 1
fi
python3 launch_preview.py
if [[ $? -ne 0 ]]; then
  read '?启动未成功，按回车关闭…'
fi
