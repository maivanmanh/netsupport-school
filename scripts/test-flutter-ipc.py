#!/usr/bin/env python3
"""Framed native IPC smoke; explicit endpoint path, never logs authentication token."""
import argparse,json,socket,struct,time
p=argparse.ArgumentParser();p.add_argument('endpoint');p.add_argument('--action',default='stop');a=p.parse_args()
d=json.load(open(a.endpoint));s=socket.create_connection(('127.0.0.1',d['port']),timeout=5)
def send(v):
 b=json.dumps(v).encode();s.sendall(struct.pack('!I',len(b))+b)
def exact(n):
 b=b''
 while len(b)<n:
  c=s.recv(n-len(b))
  if not c: raise EOFError('Agent disconnected')
  b+=c
 return b
def receive():
 n=struct.unpack('!I',exact(4))[0]
 assert 0<n<=8*1024*1024
 return json.loads(exact(n))
send({'type':'hello','protocol':1,'token':d['token']});v=receive();print('STATE',v['phase'],'students',len(v['students']))
send({'id':1,'action':a.action})
while True:
 try:v=receive()
 except EOFError:
  if a.action=='quit':print('QUIT disconnected');break
  raise
 if v.get('id')==1:print('ACK',v);assert v['ok'];break
s.close()
