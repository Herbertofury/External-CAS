// SPDX-License-Identifier: GPL-3.0-or-later
// In-memory attribute comparison against the original Blender GEOM reader.
import {readFile} from 'node:fs/promises';import {pathToFileURL,fileURLToPath} from 'node:url';import path from 'node:path';import {execFileSync} from 'node:child_process';import assert from 'node:assert/strict';import {createHash} from 'node:crypto';
const [configPath,parserRoot,rustReport,blenderGeomRoot]=process.argv.slice(2),config=JSON.parse(await readFile(configPath,'utf8')),report=JSON.parse((await readFile(rustReport,'utf8')).replace(/^\uFEFF/,''));
const {indexPackage,readResourceBatch}=await import(pathToFileURL(path.join(parserRoot,'dbpf.mjs')));const hash=b=>createHash('sha256').update(b).digest('hex');
const samples=[];
for(const [category,filename] of Object.entries(config.samples)){
 const index=await indexPackage(filename),expected=report.samples.find(s=>s.category===category);assert.equal(index.source.sha256,expected.sourceHash);const geometries=[];
 const selected=index.rows.filter(r=>r.key.startsWith('015A1849:')&&!r.deleted);if(!selected.length){samples.push({category,geometries});continue;}
 const resources=await readResourceBatch(filename,index,selected.map(r=>r.ordinal));
 for(const item of resources){assert.equal(item.status,'available');const ours=expected.geometry.find(g=>g.key===item.row.key);assert.ok(ours);assert.equal(hash(item.payload),ours.hash);assert.equal(ours.paletteError,null);assert.equal(ours.originalBytesPreserved,true);
  const output=execFileSync(process.env.CAS_ASSET_PYTHON??'python',[fileURLToPath(new URL('./geom-morph-oracle.py',import.meta.url)),blenderGeomRoot],{input:item.payload,maxBuffer:1024*1024,windowsHide:true});const reference=JSON.parse(output.toString('utf8'));assert.equal(reference.morphAttributesHash,ours.morphAttributesHash);assert.deepEqual(reference.counts,ours.morphAttributeCounts);assert.equal(reference.vertices,ours.vertices);
  geometries.push({key:item.row.key,payloadHash:ours.hash,version:ours.version,vertices:ours.vertices,morphAttributesHash:ours.morphAttributesHash,counts:ours.morphAttributeCounts,uvStitches:ours.uvStitches,seamStitches:ours.seamStitches,allMorphAttributeBytesEqual:true,originalBytesPreserved:true});
 }
 samples.push({category,geometries});
}
console.log(JSON.stringify({date:'2026-11-12',scope:'actual installed CC GEOM morphology-attribute decoding only',reference:{kind:'unmodified Blender GEOM getElementData',readerHash:hash(await readFile(path.join(blenderGeomRoot,'io','geom_load.py')))},samples,limitations:['stitch tail fields follow authoritative SimRipper layout and first-record semantics; synthetic fixture verifies order/duplicates, not a separate full-tail oracle','no native morph selection, deformation fidelity, full Sim or game appearance claim']},null,2));
