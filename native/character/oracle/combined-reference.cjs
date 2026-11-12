// SPDX-License-Identifier: GPL-3.0-or-later
// Independent shared-graph path through separately installed MIT S4TK models.
const fs=require('node:fs'),path=require('node:path');
const root=process.argv[2];const pkg=JSON.parse(fs.readFileSync(path.join(root,'package.json'),'utf8'));
if(pkg.name!=='@s4tk/models'||pkg.version!=='0.6.14')throw Error('Unexpected S4TK version');
const input=fs.readFileSync(0);if(input.length>32*1024*1024)throw Error('Input bound');
const {CombinedTuningResource}=require(path.join(root,'models.js'));
const resources=CombinedTuningResource.extractTuning(input,{filter:n=>n.attributes?.c==='Client_CASModifierTuning'});
function texts(n){if(n.value!==undefined)return [String(n.value)];return(n.children??[]).flatMap(texts);}
const result=resources.map(resource=>{const r=resource.dom.child;const context={Ages:null,Genders:null,Species:null,OccultType:null};const bindings=[];let current=null;
function walk(n){if(n.name in context)context[n.name]=texts(n).map(s=>s.trim()).filter(Boolean);if(n.name==='ModifierName')current=String(n.innerValue);else if(n.name==='Scale'){if(current===null)throw Error('Scale without name');const b=Buffer.alloc(4);b.writeFloatLE(Number(n.innerValue));bindings.push([current,b.readUInt32LE().toString(16).toUpperCase().padStart(8,'0')]);}for(const child of n.children??[])walk(child);}
walk(r);return {instance:String(r.attributes.s),context,bindings};});
process.stdout.write(JSON.stringify(result));
