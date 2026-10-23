const VIEWS={...ServiceViews,...LibraryViews,...MediaViews,...FlowViews,...NativeSettingsViews};
let toastTimer;
let messageSerial=10;
function toast(text){const box=document.getElementById('toast');box.textContent=text;box.classList.add('visible');clearTimeout(toastTimer);toastTimer=setTimeout(()=>box.classList.remove('visible'),3000);}
function currentScreenIndex(){return SCREENS.findIndex(s=>s.id===App.screen&&(s.id!=='batch'||(s.query||'')===(App.state.scope==='library'?'scope=library':''))&&(s.id!=='understand'||(s.query||'')===(App.params.get('size')==='compact'?'size=compact':'')));}
function seedQA(){
  App.state.qaMessages=[
    {id:'u1',role:'user',text:'概括这张图片。'},
    {id:'a1',role:'assistant',basis:'indexed',text:'已有分析记录描述了山峰、森林、湖泊和近处木屋，水面可以看到倒影。',ref:'分析记录 A1048'},
    {id:'u2',role:'user',text:'右侧尖顶建筑是什么？'},
    {id:'a2',role:'assistant',basis:'visual',text:'已有记录不足以确认，我只复核了当前图片。右侧林间可见一座带尖顶的建筑，外观可能是小教堂，但实际用途无法仅凭画面确定。',ref:'当前原图 · 局部画面'},
  ];
  App.state.qaInspected=true;App.state.qaSeeded=true;
}
function render(){
  document.body.dataset.ready='false';
  App.currentDesignNotes=null;
  App.currentNativeTask=null;App.currentNativeView=null;
  if(App.screen==='qa'&&!App.state.qaSeeded)seedQA();
  const descriptor=SCREENS[currentScreenIndex()]||SCREENS.find(s=>s.id===App.screen)||SCREENS[0];
  document.getElementById('app').innerHTML=(VIEWS[App.screen]||VIEWS.models)();
  document.getElementById('screen-caption').innerHTML=`<b>${descriptor.title}</b>：${descriptor.caption}`;
  document.getElementById('screen-select').innerHTML=SCREENS.map((s,i)=>`<option value="${i}" ${s===descriptor?'selected':''}>${String(i+1).padStart(2,'0')} / ${SCREENS.length}　${s.group} · ${s.title}</option>`).join('');
  document.title=`Lap · ${descriptor.title} · 全流程设计`;
  document.body.dataset.screen=App.screen;
  const nativeFrame=document.getElementById('native-settings-frame');
  if(nativeFrame){
    const source=window.LapNativeSource;
    if(!source)throw new Error('请先运行 node design/media-understanding/prepare-native.mjs，生成当前前端样式。');
    const css=source.css.replace(/<\/style/gi,'<\\/style');
    nativeFrame.srcdoc=`<!doctype html><html lang="zh-CN" data-theme="light"><head><meta charset="utf-8"><style>${css}</style><link rel="stylesheet" href="native-frame.css"></head><body><div id="native-root"></div><script src="../../src-vite/node_modules/vue/dist/vue.global.prod.js"><\/script><script src="native-frame.js"><\/script></body></html>`;
  }else document.body.dataset.ready='true';
  document.querySelectorAll('.detail-marker').forEach(marker=>{
    const frame=marker.parentElement,img=frame.querySelector('img');
    const place=()=>{const w=frame.clientWidth,h=frame.clientHeight,k=Math.min(w/img.naturalWidth,h/img.naturalHeight),iw=img.naturalWidth*k,ih=img.naturalHeight*k;Object.assign(marker.style,{left:((w-iw)/2+iw*.78)+'px',top:((h-ih)/2+ih*.58)+'px',width:(iw*.12)+'px',height:(ih*.17)+'px'});};
    if(img.complete&&img.naturalWidth)place();else img.addEventListener('load',place,{once:true});
  });
  if(App.screen==='qa')requestAnimationFrame(()=>{const h=document.getElementById('qa-history');if(h)h.scrollTop=h.scrollHeight;});
}
function navigate(screen,params={}){
  if(!VIEWS[screen])return;
  App.screen=screen;
  if(screen==='batch'&&params.scope)App.state.scope=params.scope;
  const q=new URLSearchParams({screen,...(App.params.get('size')?{size:App.params.get('size')}:{}),...params});
  history.pushState(null,'','?'+q.toString());App.params=q;render();
}
function loadLocation(){
  const q=new URLSearchParams(location.search);App.screen=VIEWS[q.get('screen')]?q.get('screen'):'models';
  App.params=q;if(App.screen==='batch')App.state.scope=SCOPES[q.get('scope')]?q.get('scope'):'selected';
  if(PROVIDERS.some(p=>p.id===q.get('provider')))App.state.providerId=q.get('provider');
  if(App.screen==='understand')App.state.task='media';
  render();
}
function appendQuestion(text){
  const question=String(text||'').trim();if(!question){toast('请输入针对当前图片的问题。');return;}
  App.state.qaSeeded=true;App.state.qaMessages.push({id:'u'+(++messageSerial),role:'user',text:question});
  if(/尖顶|门牌|写着|建筑用途|什么字/.test(question)){
    App.state.qaPending=true;
    App.state.qaMessages.push({id:'a'+(++messageSerial),role:'assistant',basis:'insufficient',text:'已有分析只记录了自然景物和木屋，没有足够证据回答这个细节。可以只复核当前图片，或保留为无法确定。',ref:'分析记录 A1048 · 未覆盖该细节'});
  }else{
    App.state.qaMessages.push({id:'a'+(++messageSerial),role:'assistant',basis:'indexed',text:'根据已保存记录，画面包含山峰、森林、湖泊和湖畔木屋，湖面有山体与天空的倒影。回答仅依据当前图片的分析记录。',ref:'分析记录 A1048'});
  }
  render();
}
function inspectCurrent(){
  App.state.qaInspected=true;App.state.qaPending=false;App.state.inspectionCount=(App.state.inspectionCount||0)+1;
  App.state.qaMessages.push({id:'a'+(++messageSerial),role:'assistant',basis:'visual',text:'复核仅针对当前图片。右侧林间可见带尖顶的建筑，外观可能是小教堂，但具体用途仍无法仅凭图片确定。没有读取其它媒体，也没有修改标题、说明或标签。',ref:'当前原图 · 局部画面'});
  render();
}
function startJob(){
  const roles=['image','video',...(App.state.indexEnabled?['textEmbed']:[])];
  if(roles.some(role=>!modelReady(UI.model(App.state.roles[role])))){toast('所选服务已停用或模型能力待验证，请到默认模型检查。');return;}
  if(scopeData().videos&&!UI.model(App.state.roles.video).caps.includes('video_frames')){toast('当前媒体理解模型未提供视频采样帧能力，请修改本次范围或模型。');return;}
  App.state.createdJobs++;App.state.jobScope=App.state.scope;App.state.jobStopped=false;
  App.state.job={scope:App.state.scope,roles:{...App.state.roles},providers:PROVIDERS.map(p=>({...service(p.id)})),indexEnabled:App.state.indexEnabled,fields:{...App.state.fields},existingTagsOnly:App.state.existingTagsOnly};
  navigate('task');
}
function runSearch(){
  const field=document.getElementById('natural-query');App.state.searchQuery=field?.value.trim()||App.state.searchQuery;
  App.state.searchRan=true;navigate('search');toast('这是检索流程示意，没有运行真实 Embedding 或媒体分析。');
}
function handleClick(event){
  const provider=event.target.closest('[data-provider]');if(provider){App.state.providerId=provider.dataset.provider;App.state.apiTested=false;App.state.renamingService=false;navigate('api');return;}
  const modelSettings=event.target.closest('[data-model-settings]');if(modelSettings){App.state.editingModelId=modelSettings.dataset.modelSettings;App.state.providerId=UI.model(App.state.editingModelId).providerId;navigate('model');return;}
  const task=event.target.closest('[data-task]');if(task){App.state.task=task.dataset.task;navigate(App.state.task==='media'?'understand':'models');return;}
  const cap=event.target.closest('[data-filter]');if(cap){App.state.filter=cap.dataset.filter;render();return;}
  const indexFilter=event.target.closest('[data-index-filter]');if(indexFilter){indexFilter.parentElement.querySelectorAll('button').forEach(b=>b.classList.remove('active'));indexFilter.classList.add('active');toast('示例：筛选分析与索引状态。');return;}
  const seek=event.target.closest('[data-seek]');if(seek){App.state.videoTime=seek.dataset.seek;render();return;}
  const tag=event.target.closest('[data-tag]');if(tag){const value=tag.dataset.tag;const set=new Set(App.state.rejectedTags||[]);set.has(value)?set.delete(value):set.add(value);App.state.rejectedTags=[...set];render();return;}
  const target=event.target.closest('[data-action]');
  if(!target){const nav=event.target.closest('[data-nav]');if(nav)navigate(nav.dataset.nav);return;}
  const action=target.dataset.action;
  if(action==='add-service'){const id='custom-'+(PROVIDERS.length+1);PROVIDERS.push({id,name:'新服务',baseUrl:'https://api.example.com/v1',enabled:false});App.state.providerId=id;App.state.apiTested=false;App.state.providerSearch='';App.state.renamingService=true;navigate('api');return;}
  if(action==='rename-service'){App.state.renamingService=true;render();return;}
  if(action==='toggle-api-key'){const field=target.ownerDocument.getElementById('service-api-key');field.type=field.type==='password'?'text':'password';target.setAttribute('aria-label',field.type==='password'?'显示 API 密钥':'隐藏 API 密钥');return;}
  if(action==='remove-provider-model'){toast('移除模型入口示意；实际操作前检查默认用途和任务引用。');return;}
  if(action==='remote-guide'){window.open('../../scripts/ai-server/README.md','_blank');return;}
  if(action==='sync-models'){toast('模拟同步当前服务模型；不改变默认模型，未发送真实网络请求。');return;}
  if(action==='add-provider-model'){toast('示例：可手填服务端模型 ID；不支持列举模型的服务也可配置。');return;}
  if(action==='check-model'){const m=UI.model(App.state.editingModelId);if(m.caps.includes('embed_multimodal')){toast('多模态 Embedding 接口尚未接入，当前保持待验证。');return;}m.status='ready';m.state='示例已验证';App.state.verifiedModels[m.id]=true;render();toast('模拟能力检测通过；没有调用真实模型。');return;}
  if(action==='save-model'){toast('模型属性保存示意；连接信息仍继承所属服务。');return;}
  if(VIEWS[action]){navigate(action);return;}
  if(action==='library-batch'){navigate('batch',{scope:'library'});return;}
  if(action==='add-model'){navigate('catalog');return;}
  if(action==='gallery'){location.href='gallery.html';return;}
  if(action==='previous-screen'||action==='next-screen'){let i=currentScreenIndex();i=(i+(action==='next-screen'?1:-1)+SCREENS.length)%SCREENS.length;const s=SCREENS[i];navigate(s.id,Object.fromEntries(new URLSearchParams(s.query||'')));return;}
  if(action==='test-api'){App.state.apiTested=true;render();document.getElementById('api-status')?.scrollIntoView({block:'nearest'});toast('模拟连接检测完成；模型能力仍需分别检测，没有发送请求。');return;}
  if(action==='save-api'){toast('已模拟保存参数；保存不自动验证、启用或切换用途。');return;}
  if(action==='save-routes'){toast('示例用途绑定已保留；已提交任务仍使用原快照。');return;}
  if(action==='start-index-job'){startJob();return;}
  if(action==='toggle-job'){App.state.jobStopped=!App.state.jobStopped;render();toast(App.state.jobStopped?'任务停止示意：已完成结果保留。':'继续未完成任务示意。');return;}
  if(action==='retry-vector'){App.state.vectorRetried=true;render();toast('只重试向量阶段，复用已有分析记录。');return;}
  if(action==='adopt-title'){
    const text=document.getElementById('title-proposal')?.textContent.trim()||'';
    if(!text||[...text].length>256||/[\r\n\x00-\x1f]/.test(text)){toast('标题应为 1–256 个字符，不能包含换行或控制字符。');return;}
    App.state.title=text;render();toast('已采用示例标题；没有修改真实图库。');return;
  }
  if(action==='adopt-description'){App.state.description=document.getElementById('description-proposal')?.textContent.trim()||'';render();toast('已采用示例说明。');return;}
  if(action==='adopt-tags'){
    const rejected=new Set(App.state.rejectedTags||[]);
    App.state.tags=[...new Set([...App.state.tags,...['湖泊','山峰','森林','木屋'].filter(t=>!rejected.has(t))])];
    render();toast('已采用未拒绝的示例标签，保留原有人工标签。');return;
  }
  if(action==='adopt-answer'){
    const id=target.closest('[data-msg]')?.dataset.msg;const message=App.state.qaMessages.find(m=>m.id===id);
    if(message&&message.basis!=='insufficient'){App.state.description=message.text;render();toast('明确采用后才写入示例说明。');}return;
  }
  if(action==='clear-qa'){App.state.qaMessages=[];App.state.qaSeeded=true;App.state.qaInspected=false;App.state.qaPending=false;render();return;}
  if(action==='ask-summary'){appendQuestion('概括这张图片。');return;}
  if(action==='ask-detail'){appendQuestion('右侧尖顶建筑是什么？');return;}
  if(action==='ask-photo'){appendQuestion(document.getElementById('photo-question')?.value);return;}
  if(action==='inspect-current'){inspectCurrent();return;}
  if(action==='run-search'){runSearch();return;}
  const messages={
    'baseline-control':'当前布局对照，控件与值均为源码重建示意。',
    'existing-tab':'现有设置分类保持原入口；预览仅演示 AI 模型部分。',
    'existing-search':'保留现有文本与组合搜索，本预览演示新增自然语言模式。',
    'test-config':'模拟保存并测试：先保留配置，再测试实际部署的输入与输出能力。',
    'evidence':'证据示例：来源图片 #1048、当前源版本、分析方案与生成部署。',
    'list-api-models':'模型列表为示意；服务无列表接口时允许手填模型 ID。',
    'preview-sample':'样例：标题“山湖与木屋”；说明与标签从同一次有效分析中生成。',
    'polish-description':'示例：把已有说明交给语言生成模型，不重新读取原图。',
    'reanalyze-current':'仅重新分析当前图片；生成新分析版本，不自动覆盖正式资料。',
    'media-info':'信息页沿用现有文件、EXIF、标题和人工标签编辑功能。',
    'demo-video':'视频使用现有照片作静态画面示意，不包含真实视频播放。',
    'inspect-video':'只复核 00:24–01:00 片段。真实实现按已验证的传输方式读取或发送媒体。',
    'ask-video':'示例：从当前范围内片段记录获取证据，信息不足时再局部复核。',
    'edit-filters':'复用现有组合筛选编辑生成的条件；模型不直接执行任意 SQL。',
  };
  toast(messages[action]||'设计交互示例，未连接真实业务。');
}
document.addEventListener('click',handleClick);
function handleChange(event){
  const el=event.target;
  if(el.hasAttribute('data-ai-model')){App.state.nativeEditingByTask={...App.state.nativeEditingByTask,[App.state.task]:el.value};App.state.editingModelId=el.value;render();return;}
  if(el.dataset.providerEnabled){const id=el.dataset.providerEnabled;App.state.providerOverrides[id]={...App.state.providerOverrides[id],enabled:el.checked};render();return;}
  if(el.dataset.modelField||el.dataset.modelCap){const m=UI.model(App.state.editingModelId);if(el.dataset.modelField)m[el.dataset.modelField]=el.value;else{const caps=new Set(m.caps);el.checked?caps.add(el.dataset.modelCap):caps.delete(el.dataset.modelCap);m.caps=[...caps];}if(el.dataset.modelCap||el.dataset.modelField==='remoteId'){m.status='untested';m.state='待验证';delete App.state.verifiedModels[m.id];}render();return;}
  if(el.dataset.serviceField){const id=App.state.providerId,key=el.dataset.serviceField;App.state.providerOverrides[id]={...App.state.providerOverrides[id],[key]:el.value};if(key==='name')App.state.renamingService=false;if(key==='baseUrl'){App.state.apiTested=false;App.state.dirtyProviders=[...new Set([...App.state.dirtyProviders,id])];MODELS.filter(m=>m.providerId===id).forEach(m=>delete App.state.verifiedModels[m.id]);}render();return;}
  if(el.id==='compact-provider'){App.state.providerId=el.value;App.state.apiTested=false;App.state.renamingService=false;navigate('api');return;}
  if(el.id==='screen-select'){const s=SCREENS[Number(el.value)];navigate(s.id,Object.fromEntries(new URLSearchParams(s.query||'')));return;}
  if(el.dataset.role){App.state.roles[el.dataset.role]=el.value;if(el.dataset.role==='image')App.state.roles.video=el.value;render();return;}
  if(el.id==='scope-select'){App.state.scope=el.value;navigate('batch',{scope:el.value});return;}
  if(el.id==='index-enabled'){App.state.indexEnabled=el.checked;render();return;}
  if(el.dataset.fillField){App.state.fields[el.dataset.fillField]=el.checked;render();return;}
  if(el.id==='existing-tags-only'){App.state.existingTagsOnly=el.checked;return;}
}
document.addEventListener('change',handleChange);
function handleInput(event){
  if(event.target.id!=='provider-search')return;
  App.state.providerSearch=event.target.value;
  const doc=event.target.ownerDocument,query=App.state.providerSearch.trim().toLowerCase();
  const rows=Array.from(doc.querySelectorAll('[data-provider]'));rows.forEach(row=>{row.hidden=!row.dataset.providerName.toLowerCase().includes(query);});
  doc.getElementById('provider-search-empty').hidden=rows.some(row=>!row.hidden);
}
document.addEventListener('input',handleInput);
function handleKeydown(event){
  if(event.key!=='Enter')return;
  if(event.target.id==='photo-question'){event.preventDefault();appendQuestion(event.target.value);}
  if(event.target.id==='natural-query'){event.preventDefault();runSearch();}
}
document.addEventListener('keydown',handleKeydown);
addEventListener('popstate',loadLocation);
window.LapDesign={screens:SCREENS,get screen(){return App.screen;},get state(){return JSON.parse(JSON.stringify(App.state));},get designNotes(){return App.currentDesignNotes?JSON.parse(JSON.stringify(App.currentDesignNotes)):null;},get nativeView(){return App.currentNativeView;},get nativeDocument(){return document.getElementById('native-settings-frame')?.contentDocument||null;},query(selector){return this.nativeDocument?.querySelector(selector)||document.querySelector(selector);},nativeEvent(type,event){({click:handleClick,change:handleChange,input:handleInput,keydown:handleKeydown})[type]?.(event);},nativeReady(frame){if(frame===document.getElementById('native-settings-frame'))document.body.dataset.ready='true';},nativeAction(action,value){if(action==='test-model'){App.state.verifiedModels[App.state.editingModelId]=true;toast('模拟模型能力检测，不调用真实模型。');}else toast(value||'现有功能入口保持原位。');},navigate};
loadLocation();
