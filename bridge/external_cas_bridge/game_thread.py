"""Bounded adapter dispatch. A game-thread scheduler must explicitly call pump.

This has no socket startup, injection, console-command or installation hooks.
It is a compatibility dispatcher, not another editor or synchronization core.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import json,queue,time
from .protocol import canonical
from .sims4_read_adapter import UnsupportedGameState
class GameThreadBridge:
 def __init__(self,adapter,max_pending=16,max_message_bytes=8*1024*1024):
  if max_pending<1 or max_message_bytes<1:raise ValueError('Explicit positive queue bounds')
  self.adapter=adapter;self.inbox=queue.Queue(max_pending);self.outbox=queue.Queue(max_pending);self.max_message_bytes=max_message_bytes;self.paused=False;self.disconnected=False
 def enqueue(self,message):
  if not isinstance(message,dict)or set(message)-{'kind','body'}or message.get('kind')not in('SnapshotRequest','ReadbackRequest','Pause','Disconnect','Apply','Rollback','OperationStatus'):raise ValueError('Unsupported typed bridge request')
  kind=message['kind'];body=message.get('body')
  if kind=='Disconnect':
   if 'body'in message:raise ValueError('Disconnect has no request body')
  elif not isinstance(body,dict):raise ValueError('Typed request body required')
  if kind=='Pause'and(set(body)!={'reason'}or not isinstance(body['reason'],str)or len(body['reason'])>1024):raise ValueError('Bounded pause reason required')
  if kind=='SnapshotRequest'and(set(body)!={'ticket'}or not isinstance(body['ticket'],dict)):raise ValueError('Exact snapshot ticket required')
  if kind in('OperationStatus','Apply','Rollback','ReadbackRequest'):
   operation=body.get('operation_id')
   if not isinstance(operation,list)or len(operation)!=16 or any(type(n)is not int or not 0<=n<=255 for n in operation):raise ValueError('Exact operation identity required')
  raw=canonical(message)
  if len(raw)>self.max_message_bytes:raise ValueError('Bridge request extent')
  self.inbox.put_nowait(json.loads(raw.decode('utf-8')))
 def pump(self,max_requests=2,budget_ms=2.0):
  self.adapter._thread();start=time.monotonic();done=0
  # This budget gates starting another request. A native getter itself cannot
  # be interrupted, so this is not a strict game-tick-time guarantee.
  while done<max_requests and (time.monotonic()-start)*1000<budget_ms and not self.outbox.full():
   try:request=self.inbox.get_nowait()
   except queue.Empty:break
   kind=request['kind'];body=request.get('body',{})
   if kind=='Disconnect':self.adapter.detach();self.disconnected=True;self.outbox.put_nowait({'kind':'Disconnect'})
   elif kind=='Pause':self.paused=True;self.outbox.put_nowait({'kind':'Pause','body':{'reason':str(body.get('reason','User paused'))[:1024]}})
   elif self.disconnected or self.paused:self.outbox.put_nowait({'kind':'Refused','body':{'operation_id':body.get('operation_id'),'reason':'Adapter is paused/disconnected'}})
   elif kind in('SnapshotRequest','ReadbackRequest'):
    try:
     ticket=body['ticket']if kind=='SnapshotRequest'else body
     if bytes(ticket['session_nonce'])!=self.adapter.session_nonce:raise UnsupportedGameState('Snapshot ticket belongs to another game process/session')
     snapshot=self.adapter.snapshot(ticket['identity']);response={'kind':'Snapshot','body':{'ticket':ticket,'snapshot':snapshot,'origin_operation':None}}if kind=='SnapshotRequest'else{'kind':'Readback','body':{'operation_id':body['operation_id'],'session_nonce':body['session_nonce'],'snapshot':snapshot}}
     if len(canonical(response))>self.max_message_bytes:raise UnsupportedGameState('Snapshot exceeds negotiated wire budget; no partial snapshot emitted')
     self.outbox.put_nowait(response)
    except Exception as error:self.outbox.put_nowait({'kind':'Refused','body':{'operation_id':None,'reason':str(error)[:1024]}})
   else:self.outbox.put_nowait({'kind':'Refused','body':{'operation_id':body.get('operation_id'),'reason':'No real-game-qualified write/rollback or operation ledger; no mutation performed'}})
   done+=1
  return done
