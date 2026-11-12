"""Actual Sims API read boundary, inactive until explicitly constructed.

No game imports at module import; no install/start hooks, profile writes, console
commands or mutation endpoints. Installed signatures establish API existence,
not runtime preservation/refresh safety. All write capabilities remain false.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib,json,threading
from collections import OrderedDict
from .protocol import canonical
ADAPTER_BUILD='0.1.0-unqualified'
class UnsupportedGameState(ValueError):pass
def integer(value):
 if type(value)is not int or not 0<=value<1<<64:raise UnsupportedGameState('Unsigned native identity required')
 return value
def opaque(value):
 if isinstance(value,bytes):return value
 if isinstance(value,str):return value.encode('utf-8')
 serialize=getattr(value,'SerializeToString',None)
 if callable(serialize):return serialize()
 if value is None or type(value)in(bool,int,float,list,tuple,dict):return canonical(value)
 raise UnsupportedGameState('Unsupported native field type; no string coercion')
class Sims4ReadAdapter:
 def __init__(self,services,game_build,session_nonce,max_snapshot_bytes=32*1024*1024):
  if not isinstance(game_build,str)or not game_build or len(game_build)>128 or not isinstance(session_nonce,bytes)or len(session_nonce)!=16 or session_nonce==bytes(16):raise UnsupportedGameState('Explicit game build and fresh session required')
  self.services=services;self.game_build=game_build;self.session_nonce=session_nonce;self.owner_thread=threading.get_ident();self.max_snapshot_bytes=max_snapshot_bytes;self.client=None;self.on_selection=None;self.sequence=0
 def _thread(self):
  if threading.get_ident()!=self.owner_thread:raise UnsupportedGameState('Sim APIs require the bound game thread')
 def capabilities(self):
  return {'game_build':self.game_build,'adapter_build':ADAPTER_BUILD,'real_game_qualified':False,'fields':[{'field':name,'read':True,'cas_write':False,'live_write':False,'rollback':False,'preservation_verified':False}for name in('outfits','physique','facial_attributes','skin_tone','skin_tone_val_shift','genetic_data','pelt_layers')],'unsupported':['Real-game runtime qualification pending','No qualified live or CAS write/rollback fields','Partial appearance observation; complete Sim/unknown/form preservation not established']}
 def identity(self,sim_info,form=None):
  self._thread();persistence=self.services.get_persistence_service();slot=persistence.get_save_slot_proto_buff();guid=integer(int(persistence.get_save_slot_proto_guid()));zone=self.services.current_zone()
  if zone is not None and not hasattr(zone,'world_id')and hasattr(persistence,'get_zone_proto_buff'):zone=persistence.get_zone_proto_buff(self.services.current_zone_id())
  if guid==0 or zone is None or not hasattr(zone,'world_id')or not hasattr(slot,'slot_id'):raise UnsupportedGameState('Save lineage/world/slot identity unavailable; linking refused')
  lineage=hashlib.sha256(('sims4-save-guid:'+str(guid)).encode('ascii')).digest();save=OrderedDict((('slot_id',integer(int(slot.slot_id))),('world_id',integer(int(zone.world_id))),('lineage',list(lineage))))
  result=OrderedDict((('save',save),('household_id',integer(int(sim_info.household_id))),('sim_id',integer(int(sim_info.sim_id))),('form',form)))
  if result['household_id']==0 or result['sim_id']==0:raise UnsupportedGameState('Household/Sim identity unavailable')
  return result
 def selected(self):
  self._thread();sim_info=self.services.active_sim_info();return None if sim_info is None else self.identity(sim_info)
 def attach_selection(self,callback):
  self._thread()
  if self.client is not None:raise UnsupportedGameState('Already observing selected client')
  client=self.services.client_manager().get_first_client()
  if client is None:raise UnsupportedGameState('No active client')
  client.register_active_sim_changed(self._selected_changed);self.client=client;self.on_selection=callback
 def _selected_changed(self,*args,**kwargs):
  self._thread();self.sequence+=1
  if self.on_selection is not None:self.on_selection({'kind':'ObserveSelection','body':{'sequence':self.sequence,'identity':self.selected()}})
 def detach(self):
  self._thread()
  if self.client is not None:self.client.unregister_active_sim_changed(self._selected_changed)
  self.client=None;self.on_selection=None
 def snapshot(self,identity):
  self._thread();sim_info=self.services.sim_info_manager().get(integer(identity['sim_id']))
  if sim_info is None:raise UnsupportedGameState('Exact Sim no longer exists')
  # Requested occult form cannot be inferred from a current-form observation.
  if identity.get('form')is not None:raise UnsupportedGameState('Explicit occult-form snapshot is not qualified')
  actual=self.identity(sim_info)
  if actual!=identity:raise UnsupportedGameState('Save/world/household/Sim identity changed')
  fields={};missing=['complete-sim-protobuf','all-occult-forms','relationships-and-progress','unrecognized-native-fields']
  for name in('physique','facial_attributes','skin_tone','skin_tone_val_shift','genetic_data','pelt_layers'):
   try:fields[name]=opaque(getattr(sim_info,name))
   except(AttributeError,TypeError,UnsupportedGameState):missing.append(name)
  try:fields['outfits']=opaque(sim_info.save_outfits())
  except(AttributeError,TypeError,UnsupportedGameState):missing.append('outfits')
  if sum(map(len,fields.values()))>self.max_snapshot_bytes:raise UnsupportedGameState('Snapshot budget; original game state untouched')
  fields=OrderedDict((k,list(v))for k,v in sorted(fields.items()));coverage={'Partial':{'missing':missing}};revision=hashlib.sha256(canonical([actual,fields,coverage])).digest()
  return {'identity':actual,'fields':fields,'coverage':coverage,'revision':list(revision)}
 def apply(self,request):
  self._thread();raise UnsupportedGameState('No real-game-qualified write/rollback contract; no mutation performed')
def create_from_game(game_build,session_nonce):
 """Call only from an explicit game-thread installation/bootstrap action."""
 import services
 return Sims4ReadAdapter(services,game_build,session_nonce)
