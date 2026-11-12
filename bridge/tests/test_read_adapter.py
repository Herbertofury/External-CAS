"""Fixture tests only: importing this file never imports/contact the game."""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib,io,sys,threading,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from external_cas_bridge.protocol import FrameCodec,ProtocolError
from external_cas_bridge.sims4_read_adapter import Sims4ReadAdapter,UnsupportedGameState
from external_cas_bridge.game_thread import GameThreadBridge
class Values:
 def __init__(self,**values):self.__dict__.update(values)
class Sim:
 sim_id=7;household_id=12;physique='fixture';facial_attributes=b'\x00\xff';skin_tone=42;skin_tone_val_shift=0.125;genetic_data=b'unknown-genetics';pelt_layers=b'future-pelt'
 def save_outfits(self):return b'raw-unknown-outfits'
class Services:
 def get_persistence_service(self):return self
 def get_save_slot_proto_buff(self):return Values(slot_id=4)
 def get_save_slot_proto_guid(self):return 12345
 def current_zone(self):return Values(world_id=8)
 def active_sim_info(self):return Sim()
 def sim_info_manager(self):return self
 def get(self,sim):return Sim()if sim==7 else None
class Tests(unittest.TestCase):
 def test_dispatch_is_bounded_detached_and_refuses_any_write(self):
  adapter=Sims4ReadAdapter(Services(),'fixture',bytes([2])*16);bridge=GameThreadBridge(adapter,max_pending=1)
  request={'kind':'SnapshotRequest','body':{'ticket':{'session_nonce':[2]*16,'identity':adapter.selected(),'navigation_epoch':1,'request_id':1}}};bridge.enqueue(request);request['body']['ticket']['identity']['sim_id']=99
  with self.assertRaises(Exception):bridge.enqueue({'kind':'Disconnect'})
  bridge.pump(budget_ms=100);self.assertEqual(bridge.outbox.get_nowait()['kind'],'Snapshot')
  bridge.enqueue({'kind':'Apply','body':{'operation_id':[3]*16}});bridge.pump(budget_ms=100);self.assertEqual(bridge.outbox.get_nowait()['kind'],'Refused')
  with self.assertRaises(ValueError):bridge.enqueue({'kind':'Pause','body':None})
 def test_read_boundary_preserves_raw_outfits_and_refuses_writes_forms_wrong_identity(self):
  adapter=Sims4ReadAdapter(Services(),'fixture',bytes([2])*16);identity=adapter.selected();snapshot=adapter.snapshot(identity);self.assertEqual(bytes(snapshot['fields']['outfits']),b'raw-unknown-outfits');self.assertIn('Partial',snapshot['coverage']);self.assertFalse(adapter.capabilities()['real_game_qualified']);self.assertTrue(all(not field['live_write']and not field['cas_write']for field in adapter.capabilities()['fields']))
  with self.assertRaises(UnsupportedGameState):adapter.apply({})
  other=dict(identity,sim_id=8)
  with self.assertRaises(UnsupportedGameState):adapter.snapshot(other)
  with self.assertRaises(UnsupportedGameState):adapter.snapshot(dict(identity,form=1))
 def test_game_thread_affinity(self):
  adapter=Sims4ReadAdapter(Services(),'fixture',bytes([2])*16);errors=[]
  def other_thread():
   try:adapter.selected()
   except UnsupportedGameState:errors.append(True)
  thread=threading.Thread(target=other_thread);thread.start();thread.join();self.assertEqual(errors,[True])
 def test_wire_replay_bounds_and_authentication(self):
  sender=FrameCodec(bytes([3])*32,bytes([5])*16,0,1024);frame=sender.encode({'kind':'Disconnect'});receiver=FrameCodec(bytes([3])*32,bytes([5])*16,1,1024);self.assertEqual(receiver.read(io.BytesIO(frame)),{'kind':'Disconnect'})
  with self.assertRaises(ProtocolError):receiver.read(io.BytesIO(frame))
  bad=bytearray(frame);bad[-1]^=1
  with self.assertRaises(ProtocolError):FrameCodec(bytes([3])*32,bytes([5])*16,1,1024).read(io.BytesIO(bad))
  self.assertNotIn(str(bytes([3])*32),repr(receiver))
if __name__=='__main__':unittest.main()
