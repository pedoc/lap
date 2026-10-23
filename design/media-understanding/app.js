const VIEWS={...ConfigViews,...LibraryViews,...MediaViews,...FlowViews};
let toastTimer;
let messageSerial=10;
function toast(text){const box=document.getElementById('toast');box.textContent=text;box.classList.add('visible');clearTimeout(toastTimer);toastTimer=setTimeout(()=>box.classList.remove('visible'),3000);}
function currentScreenIndex(){return SCREENS.findIndex(s=>s.id===App.screen&&(s.id!=='batch'||(s.query||'')===(App.state.scope==='library'?'scope=library':'')));}
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
  if(App.screen==='qa'&&!App.state.qaSeeded)seedQA();
  const descriptor=SCREENS[currentScreenIndex()]||SCREENS.find(s=>s.id===App.screen)||SCREENS[0];
  document.getElementById('app').innerHTML=(VIEWS[App.screen]||VIEWS.models)();
  document.getElementById('screen-caption').innerHTML=`<b>${descriptor.title}</b>：${descriptor.caption}`;
  document.getElementById('screen-select').innerHTML=SCREENS.map((s,i)=>`<option value="${i}" ${s===descriptor?'selected':''}>${String(i+1).padStart(2,'0')} / ${SCREENS.length}　${s.group} · ${s.title}</option>`).join('');
  document.title=`Lap · ${descriptor.title} · 全流程设计`;
  document.body.dataset.screen=App.screen;document.body.dataset.ready='true';
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
  const q=new URLSearchParams({screen,...params});
  history.pushState(null,'','?'+q.toString());App.params=q;render();
}
function loadLocation(){
  const q=new URLSearchParams(location.search);App.screen=VIEWS[q.get('screen')]?q.get('screen'):'models';
  App.params=q;if(App.screen==='batch')App.state.scope=SCOPES[q.get('scope')]?q.get('scope'):'selected';
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
  App.state.createdJobs++;App.state.jobScope=App.state.scope;App.state.jobStopped=false;
  App.state.job={scope:App.state.scope,roles:{...App.state.roles},indexEnabled:App.state.indexEnabled,fields:{...App.state.fields},existingTagsOnly:App.state.existingTagsOnly};
  navigate('task');
}
function runSearch(){
  const field=document.getElementById('natural-query');App.state.searchQuery=field?.value.trim()||App.state.searchQuery;
  App.state.searchRan=true;navigate('search');toast('这是检索流程示意，没有运行真实 Embedding 或媒体分析。');
}
document.addEventListener('click',event=>{
  const task=event.target.closest('[data-task]');if(task){App.state.task=task.dataset.task;render();return;}
  const cap=event.target.closest('[data-filter]');if(cap){App.state.filter=cap.dataset.filter;render();return;}
  const indexFilter=event.target.closest('[data-index-filter]');if(indexFilter){indexFilter.parentElement.querySelectorAll('button').forEach(b=>b.classList.remove('active'));indexFilter.classList.add('active');toast('示例：筛选分析与索引状态。');return;}
  const seek=event.target.closest('[data-seek]');if(seek){App.state.videoTime=seek.dataset.seek;render();return;}
  const tag=event.target.closest('[data-tag]');if(tag){const value=tag.dataset.tag;const set=new Set(App.state.rejectedTags||[]);set.has(value)?set.delete(value):set.add(value);App.state.rejectedTags=[...set];render();return;}
  const target=event.target.closest('[data-action]');
  if(!target){const nav=event.target.closest('[data-nav]');if(nav)navigate(nav.dataset.nav);return;}
  const action=target.dataset.action;
  if(VIEWS[action]){navigate(action);return;}
  if(action==='library-batch'){navigate('batch',{scope:'library'});return;}
  if(action==='add-model'){navigate('catalog');return;}
  if(action==='gallery'){location.href='gallery.html';return;}
  if(action==='previous-screen'||action==='next-screen'){let i=currentScreenIndex();i=(i+(action==='next-screen'?1:-1)+SCREENS.length)%SCREENS.length;const s=SCREENS[i];navigate(s.id,Object.fromEntries(new URLSearchParams(s.query||'')));return;}
  if(action==='prepare-local'){App.state.localReady=true;render();toast('模拟资源准备完成；没有下载或安装真实运行环境。');return;}
  if(action==='probe-local'){toast('模拟检测入口：加载能力、设备条件与实际推理分别报告。');return;}
  if(action==='test-api'){App.state.apiTested=true;render();document.getElementById('api-status')?.scrollIntoView({block:'nearest'});toast('模拟保存并测试完成；没有保存真实配置或发送请求。');return;}
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
});
document.addEventListener('change',event=>{
  const el=event.target;
  if(el.id==='configured-model'){navigate(el.value==='mage-local'?'local':el.value==='vision-lan'||el.value==='cloud-text'?'api':'models');return;}
  if(el.id==='screen-select'){const s=SCREENS[Number(el.value)];navigate(s.id,Object.fromEntries(new URLSearchParams(s.query||'')));return;}
  if(el.id==='local-model'){App.state.localModel=el.value;App.state.localEngine=el.value==='mage'?'python':'onnx';App.state.localReady=false;render();return;}
  if(el.name==='engine'){App.state.localEngine=el.value;render();return;}
  if(el.dataset.role){App.state.roles[el.dataset.role]=el.value;render();return;}
  if(el.id==='scope-select'){App.state.scope=el.value;navigate('batch',{scope:el.value});return;}
  if(el.id==='index-enabled'){App.state.indexEnabled=el.checked;render();return;}
  if(el.dataset.fillField){App.state.fields[el.dataset.fillField]=el.checked;render();return;}
  if(el.id==='existing-tags-only'){App.state.existingTagsOnly=el.checked;return;}
});
document.addEventListener('keydown',event=>{
  if(event.key!=='Enter')return;
  if(event.target.id==='photo-question'){event.preventDefault();appendQuestion(event.target.value);}
  if(event.target.id==='natural-query'){event.preventDefault();runSearch();}
});
addEventListener('popstate',loadLocation);
window.LapDesign={screens:SCREENS,get screen(){return App.screen;},get state(){return JSON.parse(JSON.stringify(App.state));},navigate};
loadLocation();
