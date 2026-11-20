"""Start the desktop project preview after its HTTP server is ready."""
import importlib.util
import threading
import webbrowser
from pathlib import Path

BASE = Path(__file__).resolve().parent
SOURCE = BASE.parents[1] / 'apps/browser-preview/preview_server.py'
PORT = 8897

def main():
    spec = importlib.util.spec_from_file_location('firstlight_preview', SOURCE)
    preview = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(preview)
    state_dir = BASE / '.local'
    state_dir.mkdir(exist_ok=True)
    try:
        server = preview.ThreadingHTTPServer(('127.0.0.1', PORT), preview.make_handler(str(state_dir / 'preview.sqlite')))
    except OSError as error:
        print(f'无法使用端口 {PORT}：{error}。请先关闭之前启动的测试服务。')
        return 1
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    url = f'http://127.0.0.1:{PORT}/'
    print(f'一束光桌面工程测试页：{url}', flush=True)
    print('请保留此窗口。按 Control+C 停止服务。', flush=True)
    try:
        webbrowser.open(url)
        worker.join()
    except KeyboardInterrupt:
        print('\n测试服务已停止。')
    finally:
        server.shutdown()
        server.server_close()
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
