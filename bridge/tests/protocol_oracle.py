"""Explicit fixture peer for native/Python protocol and revision conformance."""
# SPDX-License-Identifier: GPL-3.0-or-later
import io,sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from external_cas_bridge.protocol import FrameCodec
from external_cas_bridge.sims4_read_adapter import Sims4ReadAdapter
class Values:
 def __init__(self,**values):self.__dict__.update(values)
class Sim:
 sim_id=7;household_id=12;physique='fixture';facial_attributes=b'\x00\xff\x03';skin_tone=42;skin_tone_val_shift=0.125;genetic_data=b'future-genetics';pelt_layers=b'future-pelt'
 def save_outfits(self):return b'\x08\x05\x80\x02\xff\x00'
class Services:
 def get_persistence_service(self):return self
 def get_save_slot_proto_buff(self):return Values(slot_id=4)
 def get_save_slot_proto_guid(self):return 12345
 def current_zone(self):return Values(world_id=8)
 def active_sim_info(self):return Sim()
 def sim_info_manager(self):return self
 def get(self,sim):return Sim()if sim==7 else None
codec=FrameCodec(bytes([3])*32,bytes([5])*16,1)
request=codec.read(sys.stdin.buffer)
if request['kind']!='SnapshotRequest':raise ValueError('Expected typed request')
adapter=Sims4ReadAdapter(Services(),'fixture-version',bytes([5])*16)
ticket=request['body']['ticket'];snapshot=adapter.snapshot(ticket['identity'])
output={'kind':'Snapshot','body':{'ticket':ticket,'snapshot':snapshot,'origin_operation':None}}
sys.stdout.buffer.write(codec.encode(output));sys.stdout.buffer.flush()
