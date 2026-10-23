// Render and verify the standalone design using a separate headless browser profile.
import {spawn} from 'node:child_process';
import {readFile,writeFile,mkdtemp,rm,access} from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import {fileURLToPath,pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
const root=path.dirname(fileURLToPath(import.meta.url));
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const candidates=[process.env.LAP_DESIGN_BROWSER,'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe','C:/Program Files/Google/Chrome/Application/chrome.exe','/Applications/Google Chrome.app/Contents/MacOS/Google Chrome','/usr/bin/chromium','/usr/bin/google-chrome'].filter(Boolean);
let executable;for(const c of candidates){try{await access(c);executable=c;break}catch{}}
assert.ok(executable,'Set LAP_DESIGN_BROWSER to an installed Chrome or Edge executable');
const profile=await mkdtemp(path.join(os.tmpdir(),'lap-ai-design-'));
const child=spawn(executable,['--headless=new','--disable-gpu','--no-first-run','--no-default-browser-check','--hide-scrollbars','--allow-file-access-from-files','--remote-debugging-address=127.0.0.1','--remote-debugging-port=0','--user-data-dir='+profile,'about:blank'],{windowsHide:true,stdio:'ignore'});
let serial=0;
class Session{
  constructor(ws){this.ws=ws;this.pending=new Map();this.errors=[];this.network=[];ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id){const p=this.pending.get(m.id);if(p){this.pending.delete(m.id);m.error?p.reject(new Error(JSON.stringify(m.error))):p.resolve(m.result)}}else if(m.method==='Runtime.exceptionThrown')this.errors.push(m.params.exceptionDetails.text);else if(m.method==='Network.requestWillBeSent'&&/^https?:/.test(m.params.request.url))this.network.push(m.params.request.url)});}
  send(method,params={}){return new Promise((resolve,reject)=>{const id=++serial;this.pending.set(id,{resolve,reject});this.ws.send(JSON.stringify({id,method,params}))})}
  async eval(expression){const r=await this.send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw new Error(JSON.stringify(r.exceptionDetails));return r.result.value;}
}
async function connect(url){const ws=new WebSocket(url);await new Promise((resolve,reject)=>{ws.addEventListener('open',resolve,{once:true});ws.addEventListener('error',reject,{once:true})});return new Session(ws)}
async function ready(c,screen){for(let i=0;i<100;i++){try{if(await c.eval(`document.body?.dataset.ready==='true'&&window.LapDesign?.screen==='${screen}'`)){await c.eval('Promise.all([document.fonts.ready,...Array.from(document.images).map(i=>i.decode().catch(()=>{}))])');await c.eval('new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))');return}}catch{}await sleep(50)}throw new Error('Page not ready: '+screen)}
let browser;
const pages=[];
try{
 let port;for(let i=0;i<120;i++){try{port=Number((await readFile(path.join(profile,'DevToolsActivePort'),'utf8')).split('\n')[0]);if(port)break}catch{}await sleep(100)}assert.ok(port,'Render browser did not start');
 const host='http://127.0.0.1:'+port;
 browser=await connect((await fetch(host+'/json/version').then(r=>r.json())).webSocketDebuggerUrl);
 const initial=await fetch(host+'/json/new?about:blank',{method:'PUT'}).then(r=>r.json());
 const first=await connect(initial.webSocketDebuggerUrl);await first.send('Page.enable');await first.send('Page.navigate',{url:pathToFileURL(path.join(root,'index.html')).href});await ready(first,'models');
 const manifest=await first.eval('LapDesign.screens');first.ws.close();await browser.send('Target.closeTarget',{targetId:initial.id});
 for(const descriptor of manifest){
  const target=await fetch(host+'/json/new?about:blank',{method:'PUT'}).then(r=>r.json());const c=await connect(target.webSocketDebuggerUrl);
  await c.send('Page.enable');await c.send('Runtime.enable');await c.send('Network.enable');await c.send('Emulation.setDeviceMetricsOverride',{width:1600,height:1000,deviceScaleFactor:1,mobile:false});await c.send('Emulation.setEmulatedMedia',{features:[{name:'prefers-reduced-motion',value:'reduce'}]});
  const url=pathToFileURL(path.join(root,'index.html')).href+'?screen='+descriptor.id+(descriptor.query?'&'+descriptor.query:'');
  await c.send('Page.navigate',{url});await ready(c,descriptor.id);
  const layout=await c.eval(`(()=>{const win=document.querySelector('.app-window');const app=win.getBoundingClientRect();const settings=win.classList.contains('settings-window');const selector='.commit-bar .button,.qa-footer .button,.v2-heading .button,.job-strip>.button,.settings-actions .button,.settings-task-buttons .button';const controls=Array.from(document.querySelectorAll(selector)).map(e=>{const b=e.getBoundingClientRect();let visible=b.bottom<=app.bottom-(settings?1:26)&&b.top>=app.top&&b.left>=app.left&&b.right<=app.right;for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p);if(/hidden|auto|scroll|clip/.test(s.overflowY+' '+s.overflowX)){const r=p.getBoundingClientRect();if(b.bottom>r.bottom+1||b.top<r.top-1||b.left<r.left-1||b.right>r.right+1)visible=false}}return {label:e.innerText,visible}});return {window:{width:app.width,height:app.height,settings},nav:Array.from(document.querySelectorAll('.settings-root-nav button')).map(e=>e.innerText),settingsHorizontalOverflow:!!document.querySelector('.settings-content')&&document.querySelector('.settings-content').scrollWidth>document.querySelector('.settings-content').clientWidth,brokenImages:Array.from(document.images).filter(i=>!i.complete||!i.naturalWidth).map(i=>i.src),horizontalOverflow:document.documentElement.scrollWidth>innerWidth,controls}})()`);
  const shot=await c.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false,fromSurface:true});await writeFile(path.join(root,descriptor.file),Buffer.from(shot.data,'base64'));
  if(descriptor.id==='baseline'||descriptor.id==='models'){
   const clip=await c.eval("(()=>{const r=document.querySelector('.settings-window').getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,scale:1}})()");
   const crop=await c.send('Page.captureScreenshot',{format:'png',clip,fromSurface:true});await writeFile(path.join(root,descriptor.id==='baseline'?'current-settings-window.png':'proposed-settings-window.png'),Buffer.from(crop.data,'base64'));
  }
  const checks=[];
  try{
   assert.deepEqual(layout.brokenImages,[]);assert.equal(layout.horizontalOverflow,false);assert.ok(layout.controls.every(x=>x.visible),'Clipped primary controls: '+JSON.stringify(layout.controls.filter(x=>!x.visible)));checks.push('layout');
   if(layout.window.settings){assert.equal(layout.window.width,600);assert.equal(layout.window.height,620);assert.deepEqual(layout.nav,['通用','网络','布局','查看器','RAW','AI 模型','高级','快捷键','关于']);assert.equal(layout.settingsHorizontalOverflow,false);assert.equal(await c.eval("document.querySelector('.settings-window .icon-rail')===null"),true);checks.push('existing 600x620 settings window and navigation');}
   if(descriptor.id==='api'){await c.eval("document.querySelector('[data-action=save-api]').click();document.querySelector('[data-action=test-api]').click()");assert.equal(await c.eval('LapDesign.state.apiTested'),true);checks.push('save without activation; separate simulated deployment test');}
   if(descriptor.id==='models'){await c.eval("document.querySelector('[data-task=embedding]').click()");assert.ok((await c.eval("document.querySelector('#configured-model').innerText")).includes('本机检索模型'));checks.push('new purpose filters configured deployments');}
   if(descriptor.id==='local'){await c.eval("document.querySelector('#local-model').value='qwen';document.querySelector('#local-model').dispatchEvent(new Event('change',{bubbles:true}))");assert.equal(await c.eval('LapDesign.state.localEngine'),'onnx');await c.eval("document.querySelector('[data-action=prepare-local]').click()");assert.equal(await c.eval('LapDesign.state.localReady'),true);checks.push('model-scoped runtime choices');}
   if(descriptor.id==='viewer'){const before=await c.eval('LapDesign.state.title');assert.equal(before,'');await c.eval("document.querySelector('[data-tag=\"森林\"]').click()");await c.eval("document.querySelector('[data-action=adopt-tags]').click()");assert.equal(await c.eval("LapDesign.state.tags.includes('森林')"),false);await c.eval("document.querySelector('[data-action=adopt-title]').click()");assert.equal(await c.eval('LapDesign.state.title'),'山湖与木屋');checks.push('explicit adoption and rejected-tag protection');}
   if(descriptor.id==='qa'){
    await c.eval("document.querySelector('[data-action=clear-qa]').click()");const before=await c.eval('JSON.stringify({title:LapDesign.state.title,description:LapDesign.state.description,tags:LapDesign.state.tags})');
    await c.eval("document.querySelector('[data-action=ask-summary]').click();document.querySelector('[data-action=ask-detail]').click()");assert.equal(await c.eval('LapDesign.state.qaPending'),true);assert.equal(await c.eval('LapDesign.state.qaInspected'),false);
    await c.eval("document.querySelector('[data-action=inspect-current]').click()");assert.equal(await c.eval('LapDesign.state.inspectionCount'),1);assert.equal(await c.eval('LapDesign.state.qaInspected'),true);assert.equal(await c.eval('JSON.stringify({title:LapDesign.state.title,description:LapDesign.state.description,tags:LapDesign.state.tags})'),before);checks.push('indexed answer → insufficient evidence → bounded inspection; no implicit writes');
   }
   if(descriptor.id==='video'){await c.eval("document.querySelector('[data-seek=\"00:52\"]').click()");assert.ok((await c.eval("document.querySelector('#play-time').textContent")).startsWith('00:52'));checks.push('source-time jump');}
   if(descriptor.id==='search'){const before=await c.eval('LapDesign.state.createdJobs');await c.eval("document.querySelector('[data-action=run-search]').click()");assert.equal(await c.eval('LapDesign.state.createdJobs'),before);checks.push('search does not create analysis jobs');}
   if(descriptor.id==='batch'){
    const scope=descriptor.query?'library':'selected';assert.equal(await c.eval('LapDesign.state.scope'),scope);await c.eval("document.querySelector('[data-action=start-index-job]').click()");await ready(c,'task');const snapshot=await c.eval('JSON.stringify(LapDesign.state.job.roles)');await c.eval("LapDesign.navigate('routing')");await ready(c,'routing');await c.eval("document.querySelector('[data-role=visualEmbed]').value='clip-local';document.querySelector('[data-role=visualEmbed]').dispatchEvent(new Event('change',{bubbles:true}))");assert.equal(await c.eval('JSON.stringify(LapDesign.state.job.roles)'),snapshot);checks.push('scope and deployment snapshot');
    await c.eval("LapDesign.navigate('batch');document.querySelector('#index-enabled').click();for(const key of ['title','description','tags'])document.querySelector('[data-fill-field='+key+']').click();document.querySelector('[data-action=start-index-job]').click()");await ready(c,'task');assert.equal(await c.eval("document.querySelectorAll('[data-action=retry-vector]').length"),0);assert.equal(await c.eval("document.querySelector('.run-stream').innerText.match(/本次已跳过/g)?.length"),2);checks.push('optional vectors and formal writes stay skipped');
   }
   assert.deepEqual(c.errors,[]);assert.deepEqual(c.network,[]);checks.push('no script errors or external requests');
   pages.push({...descriptor,result:'passed',checks,window:layout.window,controls:layout.controls});console.log(descriptor.file+' ✓');
  }catch(error){pages.push({...descriptor,result:'failed',error:error.message,controls:layout.controls,scriptErrors:c.errors,externalRequests:c.network});console.error(descriptor.file+' FAILED: '+error.message);}
  c.ws.close();await browser.send('Target.closeTarget',{targetId:target.id});
 }
 const compositionTarget=await fetch(host+'/json/new?about:blank',{method:'PUT'}).then(r=>r.json());
 const composition=await connect(compositionTarget.webSocketDebuggerUrl);await composition.send('Page.enable');await composition.send('Page.navigate',{url:pathToFileURL(path.join(root,'gallery.html')).href});
 const art=await composition.eval(`(async()=>{
  await document.fonts.ready;
  const load=src=>new Promise((resolve,reject)=>{const i=new Image();i.onload=()=>resolve(i);i.onerror=()=>reject(new Error(src));i.src=src});
  const screens=${JSON.stringify(pages.map(p=>({title:p.title,file:p.file})))};
  const canvas=document.createElement('canvas');canvas.width=1800;canvas.height=100+Math.ceil(screens.length/4)*324;
  const ctx=canvas.getContext('2d');ctx.fillStyle='#101319';ctx.fillRect(0,0,canvas.width,canvas.height);ctx.fillStyle='#edf1f8';ctx.font='26px "Microsoft YaHei",sans-serif';ctx.fillText('Lap AI · 完整设计总览',36,44);ctx.fillStyle='#9aa8bb';ctx.font='14px "Microsoft YaHei",sans-serif';ctx.fillText('现有独立设置窗口 · 媒体理解与索引 · 自然语言搜索 · 当前媒体问答',36,75);
  for(let i=0;i<screens.length;i++){const x=36+(i%4)*438,y=108+Math.floor(i/4)*324;const im=await load(screens[i].file);ctx.drawImage(im,x,y,414,259);ctx.fillStyle='#c8d1df';ctx.font='16px "Microsoft YaHei",sans-serif';ctx.fillText(String(i+1).padStart(2,'0')+'  '+screens[i].title,x,y+285);}
  const overview=canvas.toDataURL('image/png').split(',')[1];
  canvas.width=1360;canvas.height=766;ctx.fillStyle='#101319';ctx.fillRect(0,0,1360,766);ctx.fillStyle='#edf1f8';ctx.font='21px "Microsoft YaHei",sans-serif';ctx.fillText('现有设置布局（源码重建）',40,42);ctx.fillText('修订方案（同一独立设置窗口）',720,42);
  ctx.drawImage(await load('current-settings-window.png'),40,66,600,620);ctx.drawImage(await load('proposed-settings-window.png'),720,66,600,620);ctx.fillStyle='#9aa8bb';ctx.font='15px "Microsoft YaHei",sans-serif';ctx.fillText('窗口均为 600×620 · 保留左侧分类与单列滚动区域 · 模型和能力状态为示例',40,726);ctx.font='12px "Microsoft YaHei",sans-serif';ctx.fillText('当前布局依据工作区源码重建，并非真实 Tauri 运行截图。新增功能集中扩展 AI 模型页。',40,750);
  return {overview,comparison:canvas.toDataURL('image/png').split(',')[1],galleryCards:document.querySelectorAll('article').length};
 })()`);
 assert.equal(art.galleryCards,pages.length);await writeFile(path.join(root,'overview.png'),Buffer.from(art.overview,'base64'));await writeFile(path.join(root,'settings-comparison.png'),Buffer.from(art.comparison,'base64'));composition.ws.close();await browser.send('Target.closeTarget',{targetId:compositionTarget.id});
 const report={date:'2026-10-23',viewport:{width:1600,height:1000},result:pages.every(p=>p.result==='passed')?'passed':'failed',artifacts:['overview.png','settings-comparison.png','current-settings-window.png','proposed-settings-window.png'],galleryCards:art.galleryCards,pages};await writeFile(path.join(root,'render-check.json'),JSON.stringify(report,null,2)+'\n');assert.equal(report.result,'passed','Review render-check.json for failed checks');
}finally{
 try{if(browser)await browser.send('Browser.close')}catch{}if(browser)browser.ws.close();
 await Promise.race([new Promise(r=>child.once('exit',r)),sleep(2000)]);
 const resolved=path.resolve(profile),base=path.resolve(os.tmpdir())+path.sep;
 assert.ok(resolved.startsWith(base)&&path.basename(resolved).startsWith('lap-ai-design-'),'Refuse unexpected temporary profile cleanup');
 try{await rm(resolved,{recursive:true,force:true,maxRetries:3,retryDelay:150})}catch{console.warn('Temporary browser profile remains at '+resolved)}
}
