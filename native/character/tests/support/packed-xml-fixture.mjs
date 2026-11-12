// SPDX-License-Identifier: GPL-3.0-or-later
// Entirely fabricated minimal DATA layout for bounded parser tests.
export function makePackedXmlFixture(){
 const b=Buffer.alloc(1024),tableOffset=32,schemaOffset=368,starts=[240,272,288,304,320,336,352],sizes=[20,12,8,4,4,4,1],types=[13,13,13,13,13,11,1];b.write('DATA');b.writeUInt32LE(0x101,4);b.writeInt32LE(tableOffset-8,8);b.writeInt32LE(7,12);b.writeInt32LE(schemaOffset-16,16);b.writeInt32LE(3,20);
 const lists=[[['first_element',13,0],['top_element',13,4],['element_count',7,8],['string_table',14,12]],[['text',7,0],['attrs',13,4],['children',13,8]],[['name',7,0],['value',7,4]]],columnStarts=[448,528,588];let stringAt=640;
 for(let i=0;i<7;i++){const p=tableOffset+i*28;b.writeInt32LE(i<3?schemaOffset+i*24-(p+8):-2147483648,p+8);b.writeUInt32LE(types[i],p+12);b.writeUInt32LE(sizes[i],p+16);b.writeInt32LE(starts[i]-(p+20),p+20);b.writeUInt32LE(1,p+24);}
 for(let i=0;i<3;i++){const p=schemaOffset+i*24;b.writeUInt32LE(sizes[i],p+12);b.writeInt32LE(columnStarts[i]-(p+16),p+16);b.writeUInt32LE(lists[i].length,p+20);
  for(const [j,[name,type,offset]] of lists[i].entries()){const c=columnStarts[i]+j*20;b.writeInt32LE(stringAt-c,c);b.writeUInt16LE(type,c+8);b.writeUInt32LE(offset,c+12);stringAt+=b.write(name,stringAt)+1;}}
 return b;
}
export function makePackedXmlTreeFixture({withShared=false,variant='normal'}={}){
 const NULL=-2147483648,align=n=>Math.ceil(n/16)*16,strings=['combined','g','R','I','r','c','Client_CASModifierTuning','s','1','n','Ages','x','2','L','E','TEEN','casmodifiertuning'],si=s=>strings.indexOf(s);
 const nodes=withShared?[{value:'TEEN'},{tag:'combined',children:[2,3]},{tag:'g',children:[7]},{tag:'R',attrs:[['n','casmodifiertuning']],children:[4,5]},{tag:'I',attrs:[['c','Client_CASModifierTuning'],['s','1']],children:[6]},{tag:'I',attrs:[['c','Client_CASModifierTuning'],['s','1']]},{tag:'r',attrs:[['n','Ages'],['x','2']]},{tag:'L',attrs:[['x','2']],children:[8]},{tag:'E',children:[0]}]:[{tag:'combined',children:[1,2]},{tag:'g'},{tag:'R',attrs:[['n','casmodifiertuning']],children:[3,4]},{tag:'I',attrs:[['c','Client_CASModifierTuning'],['s','1']],children:[5]},{tag:'I',attrs:[['c','Client_CASModifierTuning'],['s','1']]},{tag:'r',attrs:[['n','Ages'],['x','2']]}];
 if(withShared&&variant==='duplicate'){nodes[2].children.push(nodes.length);nodes.push({tag:'L',attrs:[['x','2']],children:[8]});}
 else if(withShared&&variant==='cycle'){nodes[7].children=[nodes.length];nodes.push({tag:'r',attrs:[['x','2']]});}
 else if(variant!=='normal')throw Error('unsupported fixture variant');
 const attrs=[],nodeRefs=[],attrRefs=[];for(const n of nodes){if(n.attrs){n.attrStart=attrRefs.length;for(const a of n.attrs){attrRefs.push(attrs.length);attrs.push(a);}attrRefs.push(null);}if(n.children){n.childStart=nodeRefs.length;nodeRefs.push(...n.children,null);}}
 const charData=Buffer.concat(strings.map(s=>Buffer.from(s+'\0'))),counts=[1,nodes.length,attrs.length,nodeRefs.length,attrRefs.length,strings.length,charData.length],sizes=[20,12,8,4,4,4,1],types=[13,13,13,13,13,11,1],tableOffset=32;
 let pos=align(tableOffset+7*28);const starts=[];for(let i=0;i<7;i++){starts.push(pos);pos=align(pos+counts[i]*sizes[i]);}const schemaOffset=pos,lists=[[['first_element',13,0],['top_element',13,4],['element_count',7,8],['string_table',14,12]],[['text',7,0],['attrs',13,4],['children',13,8]],[['name',7,0],['value',7,4]]];
 pos=align(schemaOffset+72);const columnStarts=[];for(const list of lists){columnStarts.push(pos);pos+=list.length*20;}let nameAt=pos;const b=Buffer.alloc(pos+256);b.write('DATA');b.writeUInt32LE(0x101,4);b.writeInt32LE(tableOffset-8,8);b.writeInt32LE(7,12);b.writeInt32LE(schemaOffset-16,16);b.writeInt32LE(3,20);
 for(let i=0;i<7;i++){const p=tableOffset+i*28;b.writeInt32LE(i<3?schemaOffset+i*24-(p+8):NULL,p+8);b.writeUInt32LE(types[i],p+12);b.writeUInt32LE(sizes[i],p+16);b.writeInt32LE(starts[i]-(p+20),p+20);b.writeUInt32LE(counts[i],p+24);}
 for(let i=0;i<3;i++){const p=schemaOffset+i*24;b.writeUInt32LE(sizes[i],p+12);b.writeInt32LE(columnStarts[i]-(p+16),p+16);b.writeUInt32LE(lists[i].length,p+20);for(const [j,[name,type,offset]] of lists[i].entries()){const c=columnStarts[i]+j*20;b.writeInt32LE(nameAt-c,c);b.writeUInt16LE(type,c+8);b.writeUInt32LE(offset,c+12);nameAt+=b.write(name,nameAt)+1;}}
 const meta=starts[0],rootOffset=starts[1]+(withShared?12:0);b.writeInt32LE(rootOffset-meta,meta);b.writeInt32LE(rootOffset-(meta+4),meta+4);b.writeUInt32LE(nodes.length-(withShared?1:0),meta+8);b.writeInt32LE(starts[5]-(meta+12),meta+12);b.writeUInt32LE(strings.length,meta+16);
 for(const [i,n] of nodes.entries()){const p=starts[1]+i*12;b.writeUInt32LE(si(n.tag??n.value),p);b.writeInt32LE(n.attrStart===undefined?NULL:starts[4]+n.attrStart*4-(p+4),p+4);b.writeInt32LE(n.childStart===undefined?NULL:starts[3]+n.childStart*4-(p+8),p+8);}
 for(const [i,a] of attrs.entries()){const p=starts[2]+i*8;b.writeUInt32LE(si(a[0]),p);b.writeUInt32LE(si(a[1]),p+4);}for(const [i,v] of nodeRefs.entries()){const p=starts[3]+i*4;b.writeInt32LE(v===null?NULL:starts[1]+v*12-p,p);}for(const [i,v] of attrRefs.entries()){const p=starts[4]+i*4;b.writeInt32LE(v===null?NULL:starts[2]+v*8-p,p);}
 let charAt=starts[6];for(const [i,s] of strings.entries()){const p=starts[5]+i*4;b.writeInt32LE(charAt-p,p);charAt+=b.write(s,charAt)+1;}return b.subarray(0,nameAt);
}
