"""Inventory installed bytecode signatures without importing/executing it.

Run only with the matching Python bytecode interpreter. Output contains names,
signatures and SHA256, never bytecode or decompiled EA source.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib,importlib.util,json,marshal,sys,types,zipfile
TARGETS={
 'sims/sim_info.pyc':{'save_sim','load_sim_info','resend_outfits','resend_physical_attributes'},
 'sims/sim_info_base_wrapper.pyc':{'save_sim_info','save_outfits','load_outfits','get_current_outfit'},
 'server/client.pyc':{'set_active_sim','active_sim','active_sim_info','register_active_sim_changed','unregister_active_sim_changed'},
 'services/persistence_service.pyc':{'get_save_slot_proto_buff','get_save_slot_proto_guid','get_sim_proto_buff','get_zone_proto_buff','get_world_proto_buff'},
 'services/__init__.pyc':{'get_active_sim','active_sim_info','sim_info_manager','active_household','current_zone_id'},
 'zone.pyc':{'world_id','__init__'},
}
def walk(code,path=()):
 for value in code.co_consts:
  if isinstance(value,types.CodeType):
   yield path+(value.co_name,),value
   for row in walk(value,path+(value.co_name,)):yield row
def main():
 if len(sys.argv)!=2:raise ValueError('Explicit installed simulation.zip path required')
 result=[]
 with zipfile.ZipFile(sys.argv[1])as archive:
  for member,names in TARGETS.items():
   if member not in archive.namelist():result.append({'member':member,'status':'missing'});continue
   info=archive.getinfo(member)
   if info.file_size>8*1024*1024:raise ValueError('Module extent')
   raw=archive.read(member)
   if raw[:4]!=importlib.util.MAGIC_NUMBER:raise ValueError('Matching game Python interpreter required')
   root=marshal.loads(raw[16:])
   selected=[]
   for path,code in walk(root):
    if code.co_name in names:selected.append({'name':'.'.join(path),'args':list(code.co_varnames[:code.co_argcount+code.co_kwonlyargcount]),'referenced_api_names':[n for n in code.co_names if n in {'SerializeToString','save_outfits','sim_id','household_id','world_id','slot_id','get_active_sim','active_sim_info','_base','active_sim','_active_sim_changed_callbacks'}]})
   result.append({'member':member,'sha256':hashlib.sha256(raw).hexdigest(),'signatures':selected})
 print(json.dumps({'python':sys.version.split()[0],'game_code_executed':False,'runtime_qualified':False,'modules':result},indent=2))
if __name__=='__main__':main()
