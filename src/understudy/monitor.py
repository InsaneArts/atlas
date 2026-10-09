"""Read-only loopback viewer. Captures on demand; no input or launch endpoint."""
import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
import secrets
import threading
import time
from urllib.parse import urlsplit

from .client import SessionError
from .doctor import diagnose


PAGE = """<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Understudy — live desktop</title>
<style>
:root{font:15px -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:#272720;background:#f4f3ed;color-scheme:light}
*{box-sizing:border-box}body{margin:0;padding:28px;max-width:1680px;margin-inline:auto}header,.toolbar,footer{display:flex;align-items:center;justify-content:space-between;gap:16px}
h1{font-size:28px;letter-spacing:-1px;margin:0}p{color:#66665d;line-height:1.5;margin:8px 0 0}.tag{background:#e6eadb;border-radius:30px;padding:8px 13px;font-size:12px}
.toolbar{margin:25px 0 14px;flex-wrap:wrap}button,select{font:inherit;background:#fffefa;border:1px solid #cecec2;border-radius:8px;padding:8px 12px;color:inherit}button{cursor:pointer}button:focus-visible,select:focus-visible{outline:3px solid #66884b;outline-offset:3px}
.controls{display:flex;align-items:center;gap:10px}.stage{position:relative;background:#181a18;border-radius:12px;overflow:hidden;min-height:300px;aspect-ratio:16/9;display:grid;place-items:center}.stage img{display:block;width:100%;height:100%;object-fit:contain}.stage img[hidden]{display:none}.empty{color:#e8e8dc;padding:40px;text-align:center;max-width:650px;line-height:1.6}.empty[hidden]{display:none}
footer{font-size:12px;color:#67685f;margin-top:12px;flex-wrap:wrap}.status{display:flex;gap:8px;align-items:center}.dot{height:8px;width:8px;border-radius:50%;background:#999}.dot.live{background:#567640}#checks{margin-top:24px;border-top:1px solid #d8d8cd;padding-top:16px;display:flex;gap:20px;flex-wrap:wrap}.check{font-size:13px}.check b{display:block;margin-bottom:5px}.check span{color:#686a60}.check.bad span{color:#9a4435}@media(max-width:650px){body{padding:16px}header{align-items:flex-start}.tag{white-space:nowrap}.stage{min-height:200px}footer{align-items:flex-start}}
</style>
<header><div><h1>Understudy</h1><p>A live view of the other desktop, while you stay here.</p></div><span class="tag">Observation only</span></header>
<div class="toolbar"><div class="status"><i class="dot" id="dot"></i><span id="state" role="status">Connecting…</span></div><div class="controls"><label for="rate">Refresh</label><select id="rate"><option value="1000">Every second</option><option value="2000" selected>Every 2 seconds</option><option value="5000">Every 5 seconds</option></select><button id="pause">Pause view</button></div></div>
<main class="stage"><img id="screen" alt="Live screen from the helper account" hidden><div class="empty" id="empty">Waiting for the helper desktop…</div></main>
<footer><span id="detail">No screen images are saved by this viewer.</span><span id="updated">Waiting for first frame</span></footer>
<section id="checks" aria-label="Session readiness"></section>
<script src="app.js"></script></html>"""

SCRIPT = """const el=id=>document.getElementById(id);let paused=false,timer,currentURL;
async function update(){if(paused||document.hidden)return;
 try{
  const response=await fetch('status.json',{cache:'no-store'});const info=await response.json();
  const box=el('checks');box.replaceChildren();
  for(const c of info.checks||[]){const item=document.createElement('div');item.className='check'+(c.ok?'':' bad');const title=document.createElement('b');title.textContent=c.name;const note=document.createElement('span');note.textContent=c.ok?'Ready':c.nextStep;item.append(title,note);box.append(item)}
  if(!info.canObserve)throw new Error((info.nextSteps||['Session unavailable']).join(' '));
  const frame=await fetch('frame.png',{cache:'no-store'});if(!frame.ok){const problem=await frame.json();throw new Error(problem.error||'Capture unavailable')}
  const next=URL.createObjectURL(await frame.blob());const picture=new Image();picture.src=next;await picture.decode();
  if(paused||document.hidden){URL.revokeObjectURL(next);return}
  const previous=currentURL;currentURL=next;el('screen').src=next;if(previous)URL.revokeObjectURL(previous);
  el('screen').hidden=false;el('empty').hidden=true;el('dot').className='dot live';el('state').textContent='Live · '+info.session.user.name;
  el('detail').textContent=picture.naturalWidth+' × '+picture.naturalHeight+' preview · no images saved';el('updated').textContent='Updated '+new Date().toLocaleTimeString();
 }catch(error){el('dot').className='dot';el('state').textContent='Waiting for helper';el('screen').hidden=true;el('empty').hidden=false;el('empty').textContent=error.message;el('updated').textContent='No current frame'}
 finally{clearTimeout(timer);if(!paused&&!document.hidden)timer=setTimeout(update,Number(el('rate').value))}
}
el('pause').onclick=()=>{paused=!paused;el('pause').textContent=paused?'Resume view':'Pause view';clearTimeout(timer);if(paused){el('state').textContent='View paused · agent continues';el('dot').className='dot'}else update()};
document.addEventListener('visibilitychange',()=>{clearTimeout(timer);if(!document.hidden&&!paused)update()});
update();"""


def serve(client, port=8765):
    token = secrets.token_urlsafe(24)
    prefix = "/" + token + "/"
    frame_lock = threading.Lock()
    cached = {"time": 0, "frame": None}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass  # Do not put the capability URL into an access log.

        def respond(self, code, content_type, body):
            self.send_response(code)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("Referrer-Policy", "no-referrer")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.send_header("Content-Security-Policy", "default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; img-src 'self' blob:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'")
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

        def do_GET(self):
            authority = "127.0.0.1:" + str(self.server.server_port)
            if self.headers.get("Host") != authority or self.headers.get("Origin") not in (None, "http://" + authority):
                self.respond(403, "text/plain", b"Local access only")
                return
            path = urlsplit(self.path).path
            if not path.startswith(prefix):
                self.respond(404, "text/plain", b"Use the monitor URL printed by Understudy")
                return
            route = path[len(prefix):]
            try:
                if route == "":
                    self.respond(200, "text/html; charset=utf-8", PAGE.encode())
                elif route == "app.js":
                    self.respond(200, "application/javascript", SCRIPT.encode())
                elif route == "status.json":
                    result = diagnose(client.status(), client.uid)
                    self.respond(200, "application/json", json.dumps(result).encode())
                elif route == "frame.png":
                    with frame_lock:
                        # A fresh identity check remains mandatory even for cached pixels.
                        client.identity()
                        if time.monotonic() - cached["time"] > 0.75:
                            _, cached["frame"] = client.frame()
                            cached["time"] = time.monotonic()
                        frame = cached["frame"]
                    self.respond(200, "image/png", frame)
                else:
                    self.respond(404, "text/plain", b"Unknown view")
            except (SessionError, OSError, ValueError) as exc:
                self.respond(503, "application/json", json.dumps({"error": str(exc)}).encode())

    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    url = "http://127.0.0.1:" + str(server.server_port) + prefix
    record = {"url": url, "pid": os.getpid(), "readOnly": True,
              "startedAt": datetime.datetime.now(datetime.timezone.utc).isoformat()}
    (client.output_dir / "monitor.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps(record, indent=2), flush=True)
    print("Open this URL in your main browser. Ctrl+C stops the viewer server.", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0
