"""Mock of the backend for looking at the guest pages in a browser (no Keycloak, no database).

Serves the built app (VIS_SITE, default ./site) on :8080 and the API on :8081. Team states by the last digit of the
team id: 1 normal and unconfirmed, 2 canceled, 3 confirmed and in review, 4 confirmed without email.
EDIT_DEADLINE=<iso time> sets the edit deadline (empty: none).
"""
import http.server, json, os, re, socketserver, threading, sys
ROOT=os.environ.get('VIS_SITE', os.path.join(os.path.dirname(os.path.abspath(__file__)), 'site'))
P='11111111-1111-4111-8111-111111111111'
def team(n, **kw):
    t={"id":"22222222-2222-4222-8222-22222222222%d"%n,"project_id":P,"name":"Chili Chasers","address":{"id":"33333333-3333-4333-8333-333333333333","address":"Hauptstraße 12, 60311 Frankfurt am Main","latitude":50.11,"longitude":8.68},
       "mail":"chili@example.org","phone":"+49 151 2345678","members":3,"diets":"vegetarisch, keine Nüsse",
       "created":"2026-11-10T10:00:00Z","edited":"2026-11-10T10:00:00Z","status":"active","canceled_at":None,"cancel_reason":None,
       "email_verified_at":None,"verification_resends_left":3}
    t.update(kw); return t
TEAMS={1:team(1),2:team(2,status="canceled",canceled_at="2026-11-12T09:00:00Z",cancel_reason="Krank geworden",diets=None,members=None,address={"id":"33333333-3333-4333-8333-333333333333","address":"","latitude":0,"longitude":0}),
       3:team(3,email_verified_at="2026-11-10T11:00:00Z",status="review"),4:team(4,email_verified_at="2026-11-10T11:00:00Z",mail=None)}
def config(edit_deadline):
    return {"project_name":"Cook & Run 2026","invite_text":"Willkommen! **Meldet euch an.**","require_email_verification":True,"default_needs_check":False,
      "allowed_fields":["mail","phone","members","diets"],"required_fields":["mail"],"max_teams":None,"registration_deadline":None,"edit_deadline":edit_deadline,
      "review_trigger_fields":["diets"],"registration_full":False,"plan_published":False,"created":"2026-11-01T10:00:00Z"}
class Api(http.server.BaseHTTPRequestHandler):
    def _cors(self):
        self.send_header("Access-Control-Allow-Origin","*"); self.send_header("Access-Control-Allow-Headers","*"); self.send_header("Access-Control-Allow-Methods","GET,POST,PATCH,DELETE,OPTIONS")
    def do_POST(self):
        n=int(self.headers.get('Content-Length') or 0); self.rfile.read(n); self.send_response(200); self._cors(); self.send_header('Content-Length','0'); self.end_headers()
    def do_OPTIONS(self): self.send_response(204); self._cors(); self.end_headers()
    def do_GET(self):
        if self.path.startswith("/health"):
            data=json.dumps({"status":"UP","checks":[]}).encode(); self.send_response(200); self._cors(); self.send_header("Content-Type","application/json"); self.send_header("Content-Length",str(len(data))); self.end_headers(); self.wfile.write(data); return
        m=re.match(r"/project/[^/]+/share_team_config/public",self.path)
        if m:
            body=config(os.environ.get("EDIT_DEADLINE","2026-12-24T18:00:00Z") or None)
        else:
            m=re.match(r"/project/[^/]+/team/[^/]*?(\d)$",self.path.split("?")[0])
            if not m: self.send_response(404); self._cors(); self.end_headers(); return
            body=TEAMS[int(m.group(1))]
        data=json.dumps(body).encode(); self.send_response(200); self._cors(); self.send_header("Content-Type","application/json"); self.send_header("Content-Length",str(len(data))); self.end_headers(); self.wfile.write(data)
    def log_message(self,*a): pass
class Static(http.server.SimpleHTTPRequestHandler):
    def __init__(self,*a,**k): super().__init__(*a,directory=ROOT,**k)
    def do_GET(self):
        path=self.path.split("?")[0].split("#")[0]
        if not os.path.exists(os.path.join(ROOT,path.lstrip("/"))) or path=="/": self.path="/index.html"
        return super().do_GET()
    def end_headers(self):
        if self.path.endswith(".wasm"): pass
        super().end_headers()
    def log_message(self,*a): pass
Static.extensions_map['.wasm']='application/wasm'; Static.extensions_map['.js']='text/javascript'; Static.extensions_map['.woff2']='font/woff2'
socketserver.TCPServer.allow_reuse_address=True
threading.Thread(target=lambda: socketserver.ThreadingTCPServer(("0.0.0.0",8081),Api).serve_forever(),daemon=True).start()
socketserver.ThreadingTCPServer(("0.0.0.0",8080),Static).serve_forever()
