// Design-only renderer. It imports templates and CSS, never the production SFC scripts or Tauri APIs.
(async()=>{
  const source=parent.LapNativeSource,design=parent.LapDesign,native=design.nativeView;
  if(!source)throw new Error('Run node design/media-understanding/prepare-native.mjs first.');
  const V=Vue;
  const extra={
    'settings.ai_models.media':'媒体理解','settings.ai_models.text':'语言生成','settings.ai_models.embedding':'Embedding',
    'settings.ai_models.parameters.analysis_language':'结果语言','settings.ai_models.parameters.max_tokens':'最大输出长度',
    'settings.ai_models.parameters.video_frames':'每个视频片段的采样帧数','settings.cuda.states.missing':'未安装 CUDA 运行组件',
  };
  const t=key=>extra[key]??key.split('.').reduce((value,part)=>value?.[part],source.locale)??key;
  const clean=template=>template.replace(/\s+as\s+(?:HTML\w+Element|any)/g,'').replace(/\((\w+): any\)/g,'($1)');
  const noop=()=>{};
  const feedback=message=>design.nativeAction('feedback',message);
  const bytes=value=>(value/1024**3).toFixed(1)+' GiB';
  const Dummy={template:'<span></span>'};
  const Close={template:'<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M6 6l12 12M18 6 6 18"/></svg>'};
  const Title={props:['titlebar','resizable','viewName','icon'],template:clean(source.templates.title),setup(){return {isMac:false,isMaximized:false,showDesktopWindowControls:true,minimizeWindow:noop,toggleMaximizeWindow:noop,closeWindow:()=>feedback('设计预览，不关闭实际应用窗口。')};}};
  const cuda={template:clean(source.templates.cuda),setup(){return {t,state:{supported:true,installed:false,state:'missing',totalBytes:1.7*1024**3,devices:[{id:0,name:'NVIDIA GeForce GTX 1650',memoryBytes:4*1024**3}],reason:'尚未安装运行组件，安装后可进行推理检测。',operators:{}},error:'',active:false,testing:false,feedback:null,formatModelDownloadBytes:bytes,download:()=>feedback('下载入口示意，不下载真实组件。'),test:noop,openDirectory:noop,cancel:noop};}};
  let ai=clean(source.templates.ai);
  ai=ai.replace('id="global-ai-model"','id="global-ai-model" data-ai-model');
  if(native.task&&!native.task.baseline){
    ai=ai.replace("['semantic', 'face', 'ocr', 'detection']","['semantic', 'face', 'ocr', 'detection', 'media']")
      .replace('class="flex gap-2"','class="flex flex-wrap gap-2"')
      .replace(':key="kind" class="btn btn-sm"',':key="kind" :data-task="kind" class="btn btn-sm"');
    const start=ai.indexOf('<div v-if="isRemote" class="space-y-2">');
    const end=ai.indexOf('<div class="text-xs opacity-60">{{ t(\'settings.ai_models.runtime\') }}',start);
    ai=ai.slice(0,start)+`<div v-if="isRemote" class="space-y-2 text-xs"><div>所属模型服务：{{ nativeService.name }} <button class="btn btn-xs ml-2" data-action="api">配置服务</button></div><div class="opacity-60">{{ nativeService.protocolName }} · {{ nativeService.baseUrl }}</div><div class="opacity-60">实际执行位置：{{ nativeService.location }}</div><p class="opacity-60">地址与密钥继承模型服务；图片、视频采样帧与结构化输出按该模型分别检测。</p></div>`+ai.slice(end);
    ai=ai.replace("!['ocr','detection'].includes(task)","!['ocr','detection','media','text'].includes(task)");
    ai=ai.replace("isRemote ? 'HTTP' : 'ONNX Runtime'","isRemote ? 'HTTP' : selectedModel.definition.runtime");
  }
  const config=V.reactive({settings:{tabIndex:5,language:'zh-CN',face:{enabled:true}}});
  const Ai={template:ai,setup(){
    const task=native.task?.task||'semantic';
    const choices=native.models||[];
    const selected=choices.find(m=>m.id===native.editingId)||choices[0];
    const parameters=selected?.parameters||[];
    const row=m=>({definition:{id:m.id,name:m.name,adapter:m.adapter,runtime:m.runtime,dimension:m.dimension||512,files:m.mode==='API'?[]:[{}],license:m.id==='clip-local'?'MIT':'模型许可见定义'},installed:m.installed,builtin:true,contractTested:m.contractTested??m.installed,parameters:m.parameters||[],sessions:[]});
    const modelOptions=choices.map(row),selectedModel=selected?row(selected):null;
    const draft=V.reactive({parameters:Object.fromEntries(parameters.map(p=>[p.key,p.default])),allowCloud:true,allowBackgroundUpload:false});
    const number=key=>draft.parameters[key];
    return {t,config,task,view:{models:modelOptions,selection:{[task]:native.activeId},runtime:{physicalDetectionComplete:true,adapters:[{id:0,name:'NVIDIA GeForce GTX 1650'}]}},draft,modelOptions,selectedModel,editingId:selected?.id,
      isRemote:selected?.mode==='API',nativeService:native.service||{},activeName:choices.find(m=>m.id===native.activeId)?.name||selected?.name,isActive:native.activeId===selected?.id,
      physicalDevices:[{physicalId:'sample-gpu',name:'NVIDIA GeForce GTX 1650',memoryBytes:3.8*1024**3}],
      loading:false,busy:false,error:'',draftDirty:false,apiKey:'',manifest:'',downloading:'',activeDownload:null,downloadFeedback:{},
      aiModelDisplayName:definition=>definition.name,aiModelDescription:()=>'',isRecommendedAiModel:()=>false,canActivateSavedModel:m=>m?.installed&&m.contractTested,
      formatGpuMemory:bytes,formatModelDownloadBytes:bytes,parameterValue:spec=>number(spec.key),optionAvailable:()=>true,
      optionLabel:(spec,value)=>spec.key.includes('device')?({auto:'自动（优先 GPU）',cpu:'CPU',cuda:'CUDA GPU',directml:'DirectML GPU'}[value]||value):typeof value==='boolean'?(value?'开启':'关闭'):value,
      setParameter:(spec,event)=>{draft.parameters[spec.key]=Number(event.target.value);},
      changeTask:noop,selectModel:noop,activate:()=>feedback('模型用途切换示意。'),refreshRuntime:()=>feedback('硬件重新检测入口。'),
      save:()=>feedback('保存参数示意，未修改真实配置。'),test:()=>design.nativeAction('test-model'),remove:noop,template:noop,importModel:noop,download:()=>feedback('模型下载入口示意。'),openDirectory:noop,cancelDownload:noop};
  }};
  function rawMarkup(){
    const box=document.createElement('div');box.innerHTML=native.html||'';
    box.querySelectorAll('.service-tabs').forEach(e=>e.remove());
    box.querySelectorAll('.button').forEach(e=>{const classes=e.classList;e.className='btn '+(classes.contains('small')?'btn-xs':'btn-sm')+(classes.contains('primary')?' btn-primary':'');});
    box.querySelectorAll('.input').forEach(e=>e.classList.add('input-bordered','input-sm'));
    box.querySelectorAll('.select').forEach(e=>e.classList.add('select-bordered','select-sm'));
    box.querySelectorAll('.pill').forEach(e=>{e.classList.add('badge','badge-sm');});
    return box.innerHTML;
  }
  const Raw={render:()=>V.h('div',{class:'space-y-3',innerHTML:rawMarkup()})};
  let shell=clean(source.templates.settings).replace('w-40 m-1 p-2','settings-root-nav w-40 m-1 p-2').replace('p-2 mr-1 mb-2 flex-1','settings-content p-2 mr-1 mb-2 flex-1').replace(':key="tab.id"',':key="tab.id" :data-native-settings-tab="tab.id"');
  const App={template:shell,setup(){return {config,SETTINGS_TAB:{GENERAL:1,NETWORK:9,GRID:2,IMAGE_VIEW:3,RAW:4,IMAGE_SEARCH:5,ADVANCED:6,SHORTCUTS:7,ABOUT:8},settingsTabs:[['通用',1],['网络',9],['布局',2],['查看器',3],['RAW',4],['AI 模型',5],['高级',6],['快捷键',7],['关于',8]].map(([label,id])=>({label,id})),showChangeDbStorageDialog:false,showResetDbStorageDialog:false,showBackupDialog:false,showRestoreDialog:false,selectSettingsTab:()=>feedback('保留现有设置分类，本原型仅扩展 AI 模型。')};}};
  const app=V.createApp(App);
  const localT=key=>['通用','网络','布局','查看器','RAW','AI 模型','高级','快捷键','关于'].includes(key)?key:t(key);
  app.config.globalProperties.$t=localT;
  for(const name of new Set((shell+ai+source.templates.title+source.templates.cuda).match(/(?<=<)[A-Z][A-Za-z0-9]+/g)||[]))if(!['Transition','Teleport'].includes(name))app.component(name,Dummy);
  app.component('TitleBar',Title);app.component('IconClose',Close);app.component('AiModelSettings',native.task?Ai:Raw);app.component('CudaRuntimeSettings',cuda);
  app.mount('#native-root');
  document.addEventListener('click',event=>design.nativeEvent('click',event));
  document.addEventListener('change',event=>design.nativeEvent('change',event));
  document.addEventListener('keydown',event=>design.nativeEvent('keydown',event));
  await V.nextTick();await document.fonts.ready;
  document.documentElement.dataset.ready='true';
  design.nativeReady(window.frameElement);
})();
