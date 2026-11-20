import unittest, importlib.util, tempfile, threading, urllib.request, urllib.error, http.cookiejar, json, sqlite3, hashlib, time
from pathlib import Path
spec=importlib.util.spec_from_file_location('preview',Path(__file__).resolve().parents[1]/'preview_server.py');app=importlib.util.module_from_spec(spec);spec.loader.exec_module(app)
class SharingTest(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.path=str(Path(self.tmp.name)/'db.sqlite');self.server=app.ThreadingHTTPServer(('127.0.0.1',0),app.make_handler(self.path));threading.Thread(target=self.server.serve_forever,daemon=True).start();self.base=f'http://127.0.0.1:{self.server.server_port}';self.a=self.client();self.b=self.client();self.c=self.client()
 def client(self):return urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
 def call(self,c,path,data=None,origin=None):
  req=urllib.request.Request(self.base+'/api/'+path,data=json.dumps(data).encode() if data is not None else None,headers={'Content-Type':'application/json',**({'Origin':origin} if origin else {})})
  try:
   with c.open(req) as r:return r.status,json.load(r)
  except urllib.error.HTTPError as e:return e.code,json.load(e)
 def tearDown(self):self.server.shutdown();self.server.server_close();self.tmp.cleanup()
 def invite(self):status,r=self.call(self.a,'transfers',{});self.assertEqual(status,201);return r['token']
 def test_real_two_session_receive_and_replay(self):
  token=self.invite();self.assertEqual(self.call(self.a,'transfers/accept',{'token':token})[0],409)
  self.assertEqual(self.call(self.b,'transfers/status?token='+token)[1]['state'],'OFFERED')
  self.assertEqual(self.call(self.b,'transfers/accept',{'token':token})[0],200)
  self.assertEqual(self.call(self.b,'transfers/accept',{'token':token})[0],200)
  self.assertEqual(self.call(self.c,'transfers/accept',{'token':token})[0],409)
  self.assertEqual(self.call(self.a,'transfers/status?token='+token)[1]['state'],'RECEIVED')
  self.assertEqual(self.call(self.a,'me')[1]['received_by_others'],1)
  self.assertEqual(self.call(self.b,'me')[1]['received_by_others'],0)
 def test_cancel_has_owner_check(self):
  token=self.invite();self.assertEqual(self.call(self.b,'transfers/cancel',{'token':token})[0],403);self.assertEqual(self.call(self.a,'transfers/cancel',{'token':token})[0],200);self.assertEqual(self.call(self.b,'transfers/accept',{'token':token})[0],409)
 def test_expiry(self):
  token=self.invite()
  with sqlite3.connect(self.path) as db:db.execute('update invitations set expires=?',(int(time.time()*1000)-1,))
  self.assertEqual(self.call(self.b,'transfers/accept',{'token':token})[0],409)
 def test_foreign_origin_denied(self):self.assertEqual(self.call(self.a,'transfers',{},'https://elsewhere.invalid')[0],403)
if __name__=='__main__':unittest.main()
