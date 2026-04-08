#!/usr/bin/env python3
import http.server, json, os, shutil, signal, subprocess, sys, tempfile, threading, time

G, R, N = '\033[0;32m', '\033[0;31m', '\033[0m'
B = './target/debug/spout'
D = tempfile.mkdtemp(prefix='spout.')
os.chmod(D, 0o700)
os.environ.update({
    'XDG_CONFIG_HOME': f'{D}/cfg',
    'XDG_CACHE_HOME':  f'{D}/cache',
    'XDG_DATA_HOME':   f'{D}/data',
})
C = f'{D}/cfg/spout'
mock_server = None

def ok(): print(f' [ {G}ok{N} ]')
def ko(): print(f' [ {R}!!{N} ]'); sys.exit(1)
def b(s): print(f' * {s}', end='', flush=True)
def e(v): ok() if v else ko()
def q(*a): return subprocess.run(a, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0

def cleanup(*_):
    if mock_server: mock_server.shutdown()
    shutil.rmtree(D, ignore_errors=True)
    sys.exit(0)

for sig in (signal.SIGINT, signal.SIGTERM):
    signal.signal(sig, cleanup)

def m():
    global mock_server, U
    class H(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            self.send_response(200); self.end_headers()
            self.wfile.write(json.dumps({"imageUrl": "http://127.0.0.1/i.png", "path": "/tmp/i.png"}).encode())
        def log_message(self, *a): pass
    mock_server = http.server.HTTPServer(('127.0.0.1', 0), H)
    U = f'http://127.0.0.1:{mock_server.server_address[1]}'
    threading.Thread(target=mock_server.serve_forever, daemon=True).start()
    return True

def tb(): return q('cargo', 'build', '--quiet') and os.access(B, os.X_OK)

def tc():
    return (subprocess.run([B, '--help'], capture_output=True, text=True).stdout + 
            subprocess.run([B, '--help'], capture_output=True, text=True).stderr).find('Usage') >= 0 and \
           'v' in subprocess.run([B, '--version'], capture_output=True, text=True).stdout

def tg():
    return (q(B, '-g') and
            os.path.isfile(f'{C}/config.kdl') and
            not q(B, '-g') and
            q(B, '-G'))

def write_config(path_key):
    os.makedirs(C, exist_ok=True)
    with open(f'{C}/config.kdl', 'w') as f:
        f.write(f'default "m"\nyolo true\nprofile "m" {{\n url "{U}"\n method "POST"\n format "binary"\n path "{path_key}"\n}}\n')
    os.chmod(f'{C}/config.kdl', 0o600)

def tu():
    write_config('path')
    r = subprocess.run([B, 'm'], input=b'data', capture_output=True)
    return r.returncode == 0 and b'/tmp/i.png' in r.stdout

def tf():
    write_config('nope')
    return subprocess.run([B, 'm'], input=b'data', capture_output=True).returncode != 0

def tp(): return q(B, '-p')

print(' * Initializing test environment')
b('Starting mock server');      e(m())
b('Building binary');           e(tb())
b('Checking CLI flags');        e(tc())
b('Config lifecycle');          e(tg())
b('Upload flow');               e(tu())
b('Invalid response path fails'); e(tf())
b('Config parse check');        e(tp())
print(' * All tests passed')
cleanup()
