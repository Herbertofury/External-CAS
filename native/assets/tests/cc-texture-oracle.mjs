// SPDX-License-Identifier: GPL-3.0-or-later
// Optional real-source texture comparison; asset bytes and paths stay in memory.
import {readFile} from 'node:fs/promises';
import {pathToFileURL,fileURLToPath} from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
const [configPath,root,rustReport]=process.argv.slice(2);
const config=JSON.parse(await readFile(configPath,'utf8'));
const report=JSON.parse((await readFile(rustReport,'utf8')).replace(/^\uFEFF/,''));
const {indexPackage,readResourceBatch}=await import(pathToFileURL(path.join(root,'dbpf.mjs')));
const {decodeLrleV2}=await import(pathToFileURL(path.join(root,'lrle-v2-local.mjs')));
const {decodeRle2,decodeRles,decodeBc3}=await import(pathToFileURL(path.join(root,'rle2.mjs')));
const hash=b=>createHash('sha256').update(b).digest('hex');
const evidence=[],lrleOriginal=[];
for(const [kind,filename] of Object.entries(config.samples)){
 const index=await indexPackage(filename),observed=report.samples.find(s=>s.category===kind);
 assert.equal(index.source.sha256,observed.sourceHash);assert.equal(index.rows.length,observed.entries);
 const selected=index.rows.filter(r=>r.key.startsWith('2BC04EDF:')&&!r.deleted);
 for(const prefix of ['3453CF95:','BA856C78:']){const first=index.rows.find(r=>r.key.startsWith(prefix)&&!r.deleted);if(first)selected.push(first);}
 const rows=[];for(let i=0;i<selected.length;i+=128)rows.push(...await readResourceBatch(filename,index,selected.slice(i,i+128).map(e=>e.ordinal)));
 const texturePixels=[];
 for(const {row,payload,status} of rows){
  assert.equal(status,'available');const payloadHash=hash(payload);
  if(row.key.startsWith('2BC04EDF:')){
   const r=observed.textures.find(t=>t.key===row.key);assert.ok(r);assert.equal(payloadHash,r.hash);assert.ok(r.decoded?.rgbaHash,`${kind} Rust LRLE decoding failed`);
   const record={key:row.key,payloadHash,format:r.decoded.format,width:r.decoded.width,height:r.decoded.height,rgbaHash:r.decoded.rgbaHash,rustDecoded:true};
   if(payload.toString('ascii',4,8)==='V002'){const image=decodeLrleV2(payload);assert.equal(image.width,record.width);assert.equal(image.height,record.height);assert.equal(hash(image.rgba),record.rgbaHash);record.recoveredJsPixelsEqual=true;}
   lrleOriginal.push({record,payload});texturePixels.push(record);
  }else{
   const rles=payload.toString('ascii',4,8)==='RLES';const decoded=(rles?decodeRles:decodeRle2)(payload);const rgba=decodeBc3(decoded.width,decoded.height,decoded.mips[0].bc3);
   texturePixels.push({key:row.key,payloadHash,format:rles?'RLES/BC3':'RLE2/BC3',width:decoded.width,height:decoded.height,rgbaHash:hash(rgba),oracle:'recovered JavaScript decoder; native app comparison pending'});
  }
 }
 evidence.push({category:kind,sourceBytes:index.source.size,sourceHash:index.source.sha256,indexRows:index.rows.length,texturePixels});
}
let independentLrleSource=null;
if(process.env.CAS_LRLE_SOURCE){
 const source=process.env.CAS_LRLE_SOURCE,sourceHash=hash(await readFile(source)),count=Buffer.alloc(4);count.writeUInt32LE(lrleOriginal.length);
 const input=[count];for(const sample of lrleOriginal){const length=Buffer.alloc(4);length.writeUInt32LE(sample.payload.length);input.push(length,sample.payload);}
 const output=execFileSync(process.env.CAS_LRLE_SHELL??'pwsh',['-NoLogo','-NoProfile','-NonInteractive','-File',fileURLToPath(new URL('./lrle-source-oracle.ps1',import.meta.url)),'-SourcePath',source,'-SourceSha256',sourceHash],{input:Buffer.concat(input),maxBuffer:1024*1024,windowsHide:true});
 const reference=JSON.parse(output.toString('utf8').replace(/^\uFEFF/,''));assert.equal(reference.length,lrleOriginal.length);
 for(let i=0;i<reference.length;i++){const actual=lrleOriginal[i].record;assert.equal(reference[i].width,actual.width);assert.equal(reference[i].height,actual.height);assert.equal(reference[i].rgbaHash,actual.rgbaHash);actual.originalCSharpPixelsEqual=true;}
 independentLrleSource={kind:'SimRipper LRLE.cs',sourceHash,unchangedDecoder:true,unusedImportsRemoved:true,comparisons:reference.length};
}
console.log(JSON.stringify({date:'2026-11-12',scope:'five explicit real installed CC samples; texture component parity only; original sources unchanged',recoveredOracle:'cas-public-1972 resource-index JavaScript readers',independentLrleSource,samples:evidence,limitations:['filename category is a selection aid; skin-named sample is a tattoo/skin overlay, not native TONE','source selection does not establish game load order','native material, source alias semantics and complete Sim appearance remain unverified','not actual application performance or game synchronization evidence']},null,2));
