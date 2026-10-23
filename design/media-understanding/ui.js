const UI = {
  button(label, action='demo', kind='', name='') {
    return `<button class="button ${kind}" data-action="${action}">${name?icon(name,14):''}${label}</button>`;
  },
  pill(label, kind='', name='') {
    return `<span class="pill ${kind}">${name?icon(name,11):''}${label}</span>`;
  },
  escape(value) { return String(value).replace(/[&<>"']/g, c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); },
  model(id) { return MODELS.find(m=>m.id===id)||MODELS[0]; },
  options(cap, selected) {
    const rows=MODELS.filter(m=>!cap||m.caps.includes(cap));
    return [...new Set(rows.map(m=>m.providerId||'local'))].map(id=>`<optgroup label="${id==='local'?'本地模型':service(id).name}">${rows.filter(m=>(m.providerId||'local')===id).map(m=>`<option value="${m.id}" ${m.id===selected?'selected':''} ${modelReady(m)?'':'disabled'}>${modelLabel(m)}${modelReady(m)?'':' · 不可用／待验证'}</option>`).join('')}</optgroup>`).join('');
  },
  field(label, control, help='') { return `<div class="field"><label class="field-label">${label}</label>${control}${help?`<span class="helper">${help}</span>`:''}</div>`; },
  note(text, kind='info') { return `<div class="note">${icon(kind,14)}<span>${text}</span></div>`; },
  heading(title, subtitle='', actions='') { return `<div class="v2-heading"><div><h1>${title}</h1>${subtitle?`<p class="subtext">${subtitle}</p>`:''}</div><div class="row">${actions}</div></div>`; },
  titlebar(title='我的照片图库') {
    return `<div class="titlebar windows-titlebar"><span>${title==='我的照片图库'?'Lap':title}</span><span class="spacer"></span><span class="win-controls" aria-hidden="true"><i>−</i><i>□</i><i>×</i></span></div>`;
  },
  statusbar(text) { return `<div class="statusbar">${icon('info',12)}<span>${text}</span><span class="right">当前图库 · 1,286 项</span></div>`; },
  rail(active='understand') {
    return `<div class="icon-rail">${[['home','首页','workspace'],['gallery','图库','workspace'],['folder','文件夹','workspace'],['search','搜索','search'],['tag','标签','viewer'],['person','人物','workspace'],['text','文字识别','workspace'],['box','对象','workspace'],['understand','媒体理解（新增）','workspace']].map(([i,label,to])=>`<button class="rail-item ${active===i?'active':''}" data-nav="${to}" aria-label="${label}" title="${label}">${icon(i)}</button>`).join('')}<div class="spacer"></div><button class="rail-item" data-nav="models" aria-label="打开独立设置窗口" title="打开独立设置窗口">${icon('settings')}</button></div>`;
  },
  sidebar(active='workspace') {
    const search=active==='search';
    const panel=search?`<div class="sidebar-panel-heading">搜索 <span class="dim">···</span></div><div class="sidebar-search-modes"><button class="button quiet" data-action="existing-search">文本</button><button class="button quiet" data-action="existing-search">组合</button><button class="button selected">自然语言</button></div><div class="sidebar-query"><textarea id="natural-query" class="input" aria-label="自然语言查询">${UI.escape(App.state.searchQuery)}</textarea>${UI.button('搜索','run-search','primary','search')}</div><div class="sidebar-label">搜索历史</div><div class="side-item active">湖边木屋 · 2025</div><div class="side-item">雪山日落</div><div class="side-item">海边散步</div>`:`<div class="sidebar-panel-heading">媒体理解 <span class="dim">···</span></div>${[['全部媒体','1,286'],['待分析','620'],['待建向量','196'],['可检索','448'],['失败','22']].map(([label,count],i)=>`<button class="side-item nav-side ${i===0?'active':''}" data-action="index-status">${icon(i===0?'gallery':'folder',14)}${label}<span class="number">${count}</span></button>`).join('')}<div class="side-separator"></div>${UI.button('分析当前图库','library-batch','quiet','understand')}<div style="height:10px"></div>${UI.button('查看任务','task','quiet','history')}`;
    const library=`<div class="sidebar-panel-heading">图库</div>${[['首页',''],['全部媒体','1,286'],['收藏','46'],['媒体类型',''],['评分',''],['隐藏媒体','8'],['最近删除','0']].map(([label,count],i)=>`<div class="side-item ${i===1?'active':''}">${icon(i===0?'home':'gallery',14)}${label}<span class="number">${count}</span></div>`).join('')}`;
    return `<aside class="sidebar"><div class="library-picker">我的照片图库 ${icon('down',12)}</div>${active==='library'?library:panel}<div class="side-bottom"><p class="helper">当前图库 · 1,286 项<br>${search?'基于已建立的内容索引':'分析与检索数据保存在当前图库'}</p></div></aside>`;
  },
  settingsNav(active) {
    return `<aside class="settings-root-nav" aria-label="设置分类">${['通用','网络','布局','查看器','RAW','AI 模型','高级','快捷键','关于'].map(label=>`<button class="${label==='AI 模型'?'active':''}" ${label==='AI 模型'?'data-nav="models"':'data-action="existing-tab"'}>${label}</button>`).join('')}</aside>`;
  },
  settingsWindow(content,{baseline=false,footer='',title='',notes=[]}={}) {
    App.currentDesignNotes={title,baseline,notes};
    App.currentNativeView={task:App.currentNativeTask,html:content+(footer?`<div class="flex flex-wrap items-center gap-2 mt-3">${footer}</div>`:'')};
    if(App.currentNativeTask){
      const task=App.currentNativeTask.task;
      const cap={semantic:'clip_query',ocr:'ocr'}[task];
      let choices=MODELS.filter(m=>cap&&m.caps.includes(cap));
      if(task==='semantic')choices=choices.filter(m=>m.id==='clip-local');
      if(!choices.length)choices=[{id:'current-'+task,name:task==='face'?'当前人脸识别模型':'当前对象检测模型',mode:'本机',status:'ready',caps:[]}];
      const activeId=choices[0].id;
      const editingId=App.state.nativeEditingByTask?.[task]||activeId;
      const selected=choices.find(m=>m.id===editingId)||choices[0];
      App.state.editingModelId=selected.id;
      if(selected.providerId)App.state.providerId=selected.providerId;
      const number=(key,value,min=0,max=4096)=>({key,kind:'number',default:value,min,max,step:1});
      const parameters=[number('threads',2,1,64),{key:'device',kind:'choice',default:'auto',options:['auto','cpu','cuda','directml']},number('cuda_device_id',0,0,15),number('cuda_memory_limit_mb',0,0,65536),{key:'cuda_max_workspace',kind:'choice',default:true,options:[true,false]}];
      App.currentNativeView={...App.currentNativeView,activeId,editingId,models:choices.map(m=>({...m,name:m.id==='clip-local'?'CLIP ViT-B/32 multilingual':m.name,adapter:m.id==='clip-local'?'clip_onnx':'本机适配器',runtime:'ONNX Runtime',dimension:512,installed:m.status==='ready',contractTested:modelReady(m),parameters}))};
    }
    const compact=App.params.get('size')==='compact';
    return `<div class="settings-presentation"><div class="app-window settings-window native-host" style="width:${compact?600:1437}px;height:${compact?620:833}px"><iframe id="native-settings-frame" title="设置界面预览"></iframe></div></div>`;
  },
  batchModal(content,options={}) {
    const background=`${UI.heading('全部媒体','已选择 36 项 · 32 张图片，4 段视频')}<div class="v2-library-body"><div class="index-grid">${Array.from({length:9},(_,i)=>catalogMediaCard(i)).join('')}</div></div>`;
    return UI.shell(background,{...options,active:'library',modal:`<div class="context-modal"><section class="batch-modal" role="dialog" aria-label="媒体理解与资料填充">${content}</section></div>`});
  },
  shell(content, {kind='library',active='workspace',title='',status='',modal=''}={}) {
    if(kind==='settings') return UI.settingsWindow(content,{title:'沿用独立设置窗口'});
    const body=kind==='blueprint'?`<main class="v2-main">${content}</main>`:kind==='viewer'?content:`${UI.rail(active==='search'?'search':active==='library'?'gallery':'understand')}${UI.sidebar(active)}<main class="content v2-content">${content}</main>`;
    return `<div class="app-window">${UI.titlebar(title||'我的照片图库')}<div class="app-body">${body}</div>${modal}${UI.statusbar(status||'全流程设计预览')}</div>`;
  },
  roleLine(label,role,cap,help='') {
    return `<div class="route-row"><div><strong>${label}</strong>${help?`<p>${help}</p>`:''}</div><select class="select" data-role="${role}" aria-label="${label}">${UI.options(cap,App.state.roles[role])}</select></div>`;
  },
  sourceChip(label,action='evidence') { return `<button class="source-chip" data-action="${action}">${icon('folder',11)}${label}</button>`; },
};
const ASSETS='../home/assets/';
const PHOTO_LIST=[{image:'hero.jpg',name:'山湖与木屋',file:'DSC_1048.jpg'},{image:'coast.jpg',name:'海岸漫步',file:'DSC_1062.jpg'},{image:'forest.jpg',name:'林间小路',file:'DSC_1071.jpg'},{image:'mountain.jpg',name:'云间山峰',file:'DSC_1090.jpg'}];
