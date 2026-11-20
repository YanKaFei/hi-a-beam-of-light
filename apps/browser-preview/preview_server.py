#!/usr/bin/env python3
"""Local-only acceptance preview. Isolated from the production First Light API."""
import argparse, hashlib, json, os, secrets, sqlite3, time, tempfile
from contextlib import closing
from http.cookies import SimpleCookie
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit, parse_qs
ROOT=Path(__file__).resolve().parent

def database(path):
 db=sqlite3.connect(path,timeout=10)
 db.row_factory=sqlite3.Row
 db.execute('create table if not exists invitations (digest text primary key, sender text not null, receiver text, created integer not null, expires integer not null, state text not null)')
 return db

def make_handler(db_path,public_base=''):
 class Handler(SimpleHTTPRequestHandler):
  def __init__(self,*a,**kw):super().__init__(*a,directory=str(ROOT),**kw)
  def log_message(self,fmt,*args):pass
  def end_headers(self):
   self.send_header('Referrer-Policy','same-origin')
   self.send_header('X-Content-Type-Options','nosniff')
   super().end_headers()
  def actor(self):
   c=SimpleCookie()
   try:c.load(self.headers.get('Cookie',''))
   except Exception:pass
   val=c.get('fl_preview')
   self.new_cookie=False
   if val and len(val.value)==48 and all(x in '0123456789abcdef' for x in val.value):return val.value
   self.new_cookie=True
   return secrets.token_hex(24)
  def reply(self,status,data):
   body=json.dumps(data,ensure_ascii=False).encode()
   self.send_response(status);self.send_header('Content-Type','application/json; charset=utf-8');self.send_header('Cache-Control','no-store')
   if getattr(self,'new_cookie',False):self.send_header('Set-Cookie',f'fl_preview={self.user}; Path=/; HttpOnly; SameSite=Lax')
   self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
  def do_GET(self):
   if urlsplit(self.path).path.startswith('/api/'):return self.api('GET')
   return super().do_GET()
  def do_POST(self):return self.api('POST')
  def api(self,method):
   self.user=self.actor();parsed=urlsplit(self.path);route=parsed.path
   origin=self.headers.get('Origin')
   if method=='POST' and origin and urlsplit(origin).netloc!=self.headers.get('Host'):return self.reply(403,{'error':'来源不匹配'})
   try:
    size=int(self.headers.get('Content-Length','0'))
    if size>2048 or size<0:return self.reply(413,{'error':'请求过大'})
    body=json.loads(self.rfile.read(size) or b'{}') if method=='POST' else {}
   except (ValueError,UnicodeError):return self.reply(400,{'error':'请求格式不正确'})
   if not isinstance(body,dict):return self.reply(400,{'error':'请求必须是 JSON 对象'})
   now=int(time.time()*1000)
   with closing(database(db_path)) as db, db:
    if route=='/api/me' and method=='GET':
     count=db.execute("select count(*) from invitations where sender=? and state='RECEIVED'",(self.user,)).fetchone()[0]
     return self.reply(200,{'received_by_others':count,'mode':'LOCAL_PREVIEW'})
    if route=='/api/transfers' and method=='POST':
     count=db.execute('select count(*) from invitations where sender=? and created>?',(self.user,now-3600000)).fetchone()[0]
     if count>=30:return self.reply(429,{'error':'邀请过于频繁，请稍后再试'})
     token=secrets.token_urlsafe(24);digest=hashlib.sha256(token.encode()).hexdigest();expiry=now+900000
     db.execute("insert into invitations values (?,?,null,?,?,'OFFERED')",(digest,self.user,now,expiry))
     return self.reply(201,{'token':token,'state':'OFFERED','expires_at':expiry,'public_base':public_base})
    token=body.get('token','') if method=='POST' else parse_qs(parsed.query).get('token',[''])[0]
    if not isinstance(token,str) or not 20<=len(token)<=100:return self.reply(400,{'error':'邀请无效'})
    digest=hashlib.sha256(token.encode()).hexdigest()
    db.execute('BEGIN IMMEDIATE')
    row=db.execute('select * from invitations where digest=?',(digest,)).fetchone()
    if not row:return self.reply(404,{'error':'邀请不存在'})
    if row['state']=='OFFERED' and row['expires']<=now:
     db.execute("update invitations set state='EXPIRED' where digest=?",(digest,));row=dict(row);row['state']='EXPIRED'
    if route=='/api/transfers/accept' and method=='POST':
     if row['sender']==self.user:return self.reply(409,{'error':'这是你发出的光，请让另一位用户扫码接收。'})
     if row['state']=='RECEIVED' and row['receiver']==self.user:return self.reply(200,{'state':'RECEIVED','is_receiver':True})
     if row['state']!='OFFERED':return self.reply(409,{'error':'邀请已失效或已经被其他人接收。'})
     db.execute("update invitations set state='RECEIVED',receiver=? where digest=?",(self.user,digest))
     return self.reply(200,{'state':'RECEIVED','is_receiver':True})
    if route=='/api/transfers/cancel' and method=='POST':
     if row['sender']!=self.user:return self.reply(403,{'error':'只能撤销自己发出的邀请。'})
     if row['state']!='OFFERED':return self.reply(409,{'error':'邀请已被接收或失效，不能撤销。'})
     db.execute("update invitations set state='CANCELLED' where digest=?",(digest,));return self.reply(200,{'state':'CANCELLED'})
    if route=='/api/transfers/status' and method=='GET':return self.reply(200,{'state':row['state'],'expires_at':row['expires'],'is_owner':row['sender']==self.user,'is_receiver':row['receiver']==self.user})
    return self.reply(404,{'error':'接口不存在'})
 return Handler

def main():
 p=argparse.ArgumentParser();p.add_argument('--port',type=int,default=8896);p.add_argument('--host',default='127.0.0.1');p.add_argument('--public-base',default='');p.add_argument('--state',default=str(Path(tempfile.gettempdir())/f'firstlight-preview-{os.getuid()}.sqlite'))
 a=p.parse_args()
 server=ThreadingHTTPServer((a.host,a.port),make_handler(a.state,a.public_base.rstrip('/')))
 print(f'First Light preview: http://{a.host}:{a.port}/',flush=True)
 try:server.serve_forever()
 except KeyboardInterrupt:pass
 finally:server.server_close()
if __name__=='__main__':main()
