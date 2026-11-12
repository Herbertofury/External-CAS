// SPDX-License-Identifier: GPL-3.0-or-later
// Independent selective graph over separately installed MIT @s4tk/models0.6.14.
// Production Rust never calls Node or this development oracle.
const fs=require('node:fs'),path=require('node:path');
const root=process.argv[2],pkg=JSON.parse(fs.readFileSync(path.join(root,'package.json'),'utf8'));
if(pkg.name!=='@s4tk/models'||pkg.version!=='0.6.14')throw Error('Unexpected S4TK version');
const raw=fs.readFileSync(0);if(raw.length>32*1024*1024)throw Error('Input bound');
const {CombinedTuningResource}=require(path.join(root,'models.js'));
const model=CombinedTuningResource.readBinaryDataModel(raw),tables=model.mTable,data=model.mTableData,NULL=-2147483648;
if(tables.length!==7||data.length!==7)throw Error('Tables');
const starts=tables.map(t=>Math.ceil((t.startof_mnRowOffset+t.mnRowOffset)/16)*16);
function ref(r){return r.mDataOffset===NULL?null:r.startof_mDataOffset+r.mDataOffset;}
function row(table,pos){const i=(pos-starts[table])/tables[table].mnRowSize;if(!Number.isSafeInteger(i)||i<0||i>=tables[table].mnRowCount)throw Error('Row');return i;}
let steps=0;
function list(r,table,target){let p=ref(r);if(p===null)return [];let i=row(table,p),out=[];while(i<tables[table].mnRowCount){if(++steps>6000000)throw Error('Reference budget');p=ref(data[table].mValue[i++]);if(p===null)return out;row(target,p);out.push(p);}throw Error('Unterminated list');}
const cache=new Map();
function text(i){if(cache.has(i))return cache.get(i);const p=ref(data[5].mValue[i]),end=raw.indexOf(0,p);if(p<starts[6]||end<p||end>=starts[6]+tables[6].mnRowCount||end-p>65536)throw Error('String');const s=new TextDecoder('utf-8',{fatal:true}).decode(raw.subarray(p,end));if(cache.size<32768)cache.set(i,s);return s;}
const meta=data[0].mRow[0],first=ref(meta.first_element);
function node(p){const n=data[1].mRow[row(1,p)];return {p,tag:p>=first?text(n.text):null,value:p<first?text(n.text):null,n};}
function attrs(n){const out={};for(const p of list(n.n.attrs,4,2)){const a=data[2].mRow[row(2,p)],k=text(a.name);if(Object.hasOwn(out,k))throw Error('Duplicate attr');out[k]=text(a.value);}return out;}
function children(n){return list(n.n.children,3,1);}
const top=node(ref(meta.top_element));if(top.tag!=='combined')throw Error('Root');
const shared=new Map(),records=[];
for(const p of children(top)){const c=node(p);if(c.tag==='g')for(const q of children(c)){const id=attrs(node(q)).x;if(shared.has(id))throw Error('Duplicate shared');shared.set(id,q);}else if(c.tag==='R')for(const q of children(c)){const r=node(q),a=attrs(r);if(r.tag==='I'&&a.c==='Client_CASModifierTuning')records.push(q);}}
function expand(p,chain=new Set(),depth=0,budget={n:0}){if(depth>64||++budget.n>100000||chain.has(p))throw Error('Graph budget/cycle');chain.add(p);const n=node(p);let out;if(n.tag===null)out={value:n.value};else {const a=attrs(n);if(n.tag==='r'){if(!shared.has(a.x))throw Error('Missing shared');out=expand(shared.get(a.x),chain,depth+1,budget);if(a.n!==undefined)out={...out,attrs:{n:a.n,...out.attrs}};}else out={tag:n.tag,attrs:a,children:children(n).map(q=>expand(q,chain,depth+1,budget))};}chain.delete(p);return out;}
function texts(n){return n.value!==undefined?[n.value]:(n.children??[]).flatMap(texts);}
const result=records.map(p=>{const r=expand(p),context={Ages:null,Genders:null,Species:null,OccultType:null},bindings=[];let current=null;
function walk(n){const name=n.attrs?.n;if(name in context)context[name]=texts(n).map(s=>s.trim()).filter(Boolean);if(name==='ModifierName')current=texts(n).join('');else if(name==='Scale'){if(current===null)throw Error('Scale without name');const b=Buffer.alloc(4);b.writeFloatLE(Number(texts(n).join('')));bindings.push([current,b.readUInt32LE().toString(16).toUpperCase().padStart(8,'0')]);}for(const c of n.children??[])walk(c);}
walk(r);return {instance:r.attrs.s,context,bindings};});
process.stdout.write(JSON.stringify(result));
