#!/bin/zsh
cd -- "${0:A:h}" || exit 1
if ! command -v python3 >/dev/null 2>&1; then
  print '未找到 Python 3。请先安装 Python 3，再运行这个启动文件。'
  read '?按回车关闭…'
  exit 1
fi
print '第一束光测试页：http://127.0.0.1:8896/'
print '请保留此窗口；关闭窗口会停止预览。停止服务请按 Control+C。'
open 'http://127.0.0.1:8896/'
python3 preview_server.py --port 8896
if [[ $? -ne 0 ]]; then
  print '启动失败。若端口已被占用，可尝试打开上面的链接，或将端口 8896 改为其他端口。'
  read '?按回车关闭…'
fi
