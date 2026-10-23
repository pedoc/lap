// Design-only form: configuration and detection are kept in memory, with no network calls.
let embeddingSetupSecret='';
function openEmbeddingSetup({add=false,providerId=null,modelId=null}={}){
  const model=add?null:modelId?MODELS.find(m=>m.id===modelId):roleModel('textEmbed');
  const p=model?service(model.providerId):providerId?service(providerId):null;
  const e=model?.embedding;
  embeddingSetupSecret='';
  App.state.embeddingDraft={modelId:model?.id||null,serviceId:p?.id||'new',serviceName:p?.name||'',baseUrl:p?.baseUrl||'',
    remoteId:model?.remoteId||'',name:model?.name||'',preset:e?.queryInstruction===DEPLOYMENT.embedding.queryInstruction?'qwen3':e?.queryInstruction?'custom':'plain',
    queryInstruction:e?.queryInstruction||'',detected:model&&modelReady(model)?e?.dimensions||null:null};
  navigate(add?'embeddingAdd':'embedding');
}
function embeddingSetupView(add=false){
  App.state.task='media';
  if(!App.state.embeddingDraft){
    const model=add?null:UI.model('embed-text-api'),p=model?service(model.providerId):null,e=model?.embedding;
    App.state.embeddingDraft={modelId:model?.id||null,serviceId:p?.id||'new',serviceName:p?.name||'',baseUrl:p?.baseUrl||'',remoteId:model?.remoteId||'',name:model?.name||'',
      preset:e?'qwen3':'plain',queryInstruction:e?.queryInstruction||'',detected:model&&modelReady(model)?e?.dimensions||null:null};
  }
  const d=App.state.embeddingDraft,existing=d.serviceId!=='new';
  return UI.settingsWindow(`<p class="helper">把分析文字和搜索问题转为向量，用于语义检索。</p>${taskButtons()}<div class="between"><button class="text-button" data-nav="understand">‹ 返回模型服务</button>${!d.modelId?nativeButton('填入部署脚本示例','embedding-example'):nativeButton('添加其他 Embedding 模型','new-embedding')}</div>${configCard('服务连接',`${UI.field('所属服务',`<select class="select" data-embedding-setup="serviceId" aria-label="Embedding 所属服务"><option value="new" ${existing?'':'selected'}>＋ 添加新服务</option>${PROVIDERS.map(p=>`<option value="${p.id}" ${p.id===d.serviceId?'selected':''}>${UI.escape(service(p.id).name)}</option>`).join('')}</select>`)}${UI.field('API 地址',`<input class="input" data-embedding-setup="baseUrl" value="${UI.escape(d.baseUrl)}" placeholder="填写向量服务的 API 基础地址，例如 https://…/v1" aria-label="Embedding API 地址">`)}${UI.field('API Key',`<input class="input" type="password" data-embedding-setup="apiKey" value="${UI.escape(embeddingSetupSecret)}" autocomplete="off" placeholder="${existing?'留空保留该服务已保存的密钥':'按服务要求填写'}" aria-label="Embedding API Key">`)}<p class="helper">${existing?'复用并编辑所选服务的连接；保存后该服务下其他模型也使用同一地址和密钥。':'保存时创建一个可自由命名的服务；以后可在该服务下添加其他模型。'}</p>`)}${configCard('Embedding 模型',`${UI.field('Model ID',`<input class="input" data-embedding-setup="remoteId" value="${UI.escape(d.remoteId)}" placeholder="例如：qwen3-embedding-0.6b" aria-label="Embedding Model ID">`)}<p class="helper">从服务提供方复制向量模型 ID；它与上方的服务名称不同。</p><div class="row"><span class="badge badge-sm">文本 Embedding</span><span class="helper">用于分析描述与查询编码。</span></div>`)}<details class="provider-details"><summary>高级设置（通常无需修改）</summary><div class="embedding-advanced">${UI.field('服务显示名称',`<input class="input" data-embedding-setup="serviceName" value="${UI.escape(d.serviceName)}" placeholder="留空自动命名">`)}${UI.field('模型显示名称',`<input class="input" data-embedding-setup="name" value="${UI.escape(d.name)}" placeholder="留空使用 Model ID">`)}${UI.field('编码方案',`<select class="select" data-embedding-setup="preset" aria-label="Embedding 编码方案"><option value="plain" ${d.preset==='plain'?'selected':''}>通用文本</option><option value="qwen3" ${d.preset==='qwen3'?'selected':''}>Qwen3-Embedding 检索</option><option value="custom" ${d.preset==='custom'?'selected':''}>自定义查询指令</option></select>`)}${d.preset!=='plain'?UI.field('查询指令',`<input class="input" data-embedding-setup="queryInstruction" value="${UI.escape(d.queryInstruction)}">`):''}<p class="helper">维度由检测结果自动填写；查询和入库编码方案应与模型说明一致。</p></div></details><div class="embedding-detection" role="status"><strong id="embedding-detection-status">${d.detected?'检测通过 · 文本输入 → '+d.detected+' 维向量':'尚未检测'}</strong><p class="helper">分别验证正文与查询向量；不使用聊天请求检测向量模型。</p></div><div class="row wrap"><button class="btn btn-sm" data-action="test-embedding-setup">检测 Embedding</button><button class="btn btn-sm btn-primary" data-action="save-embedding-setup" ${d.detected?'':'disabled'}>保存并设为默认 Embedding</button></div>`,{title:'填写连接、检测并设为默认',notes:[['从默认项直接进入','媒体理解页的默认 Embedding 旁始终有“配置”按钮；未配置时可直接添加，已配置时可编辑或添加其他模型。无需知道某个预置服务编号。'],['最少只填写连接和 Model ID','选择已有服务，或在同一表单新建服务，填写 API 地址、按服务要求填写 Key、填写 Embedding Model ID。文本 Embedding 能力自动选中；检测读取维度。'],['一次保存完成绑定','检测通过后点击“保存并设为默认 Embedding”，创建或更新服务与模型并绑定默认用途。地址与密钥仍归属于服务，模型只引用服务，不复制连接。保存不会自动分析全库或创建向量任务。'],['高级参数折叠','显示名和查询编码放在高级设置。部署脚本示例自动填入 8002、固定 Model ID 和 Qwen3 查询方案；其他 API 按其模型说明配置。'],['原型边界','此表单只演示内存状态，不发送真实请求。密钥草稿不进入可导出的 App 状态或保存配置；正式实现应由 Rust 写入系统凭据库。']]});
}
function updateEmbeddingSetup(field,value,doc){
  const d=App.state.embeddingDraft;
  if(field==='apiKey')embeddingSetupSecret=value;else d[field]=value;
  d.detected=null;
  if(field==='serviceId'){
    const p=value==='new'?null:service(value);d.baseUrl=p?.baseUrl||'';d.serviceName=p?.name||'';embeddingSetupSecret='';render();return;
  }
  if(field==='preset'){
    d.queryInstruction=value==='qwen3'?DEPLOYMENT.embedding.queryInstruction:value==='plain'?'':d.queryInstruction;render();return;
  }
  if(doc){doc.getElementById('embedding-detection-status').textContent='配置已修改，请重新检测';doc.querySelector('[data-action=save-embedding-setup]').disabled=true;}
}
function testEmbeddingSetup(){
  const d=App.state.embeddingDraft;
  try{
    const url=new URL(d.baseUrl.trim());
    if(!['http:','https:'].includes(url.protocol)||url.username||url.password||url.search||url.hash)throw new Error();
  }catch{toast('请填写不含密钥、查询参数或片段的 HTTP(S) API 基础地址。');return;}
  if(!d.remoteId.trim()||d.remoteId.trim().length>256||/[\x00-\x1f\x7f]/.test(d.remoteId)){toast('请填写有效的 Embedding Model ID。');return;}
  const known=MODELS.find(m=>m.providerId===d.serviceId&&m.remoteId===d.remoteId.trim());
  if(known&&!known.caps.includes('embed_text')){toast('这个样例模型提供生成能力，请填写文本 Embedding 模型 ID。');return;}
  // The real implementation learns this from the API; these are explicit design fixtures.
  const model=MODELS.find(m=>m.id===d.modelId);
  d.detected=d.preset==='qwen3'?DEPLOYMENT.embedding.dimensions:model?.embedding?.dimensions||1536;
  render();toast('模拟文本向量检测通过；未请求真实 API。');
}
function saveEmbeddingSetup(){
  const d=App.state.embeddingDraft;if(!d.detected){toast('请先检测 Embedding。');return;}
  const fingerprint=model=>model?JSON.stringify([model.id,model.remoteId,model.modelRevision,model.embedding,service(model.providerId).baseUrl]):'';
  const previous=fingerprint(roleModel('textEmbed'));
  let providerId=d.serviceId;
  if(providerId==='new'){
    providerId='custom-'+(PROVIDERS.length+1);PROVIDERS.push({id:providerId,name:d.serviceName.trim()||'模型服务 '+(PROVIDERS.length+1),baseUrl:d.baseUrl.trim(),enabled:true});
  }else{
    const old=service(providerId),changed=old.baseUrl!==d.baseUrl.trim()||!!embeddingSetupSecret;
    App.state.providerOverrides[providerId]={...App.state.providerOverrides[providerId],name:d.serviceName.trim()||old.name,baseUrl:d.baseUrl.trim(),enabled:true};
    if(changed){App.state.dirtyProviders=[...new Set([...App.state.dirtyProviders,providerId])];MODELS.filter(m=>m.providerId===providerId).forEach(m=>delete App.state.verifiedModels[m.id]);}
  }
  let model=MODELS.find(m=>m.id===d.modelId)||MODELS.find(m=>m.providerId===providerId&&m.remoteId===d.remoteId.trim());
  const embedding={format:'openai-text',dimensions:d.detected,documentEncoding:'plain-text',queryInstruction:d.preset==='plain'?'':d.queryInstruction};
  if(!model){model={id:'embedding-custom-'+(MODELS.length+1),mode:'API',location:'API',group:'其他',caps:['embed_text'],api:'embeddings',modelRevision:'unknown'};MODELS.push(model);}
  model.caps=[...new Set([...model.caps,'embed_text'])];
  Object.assign(model,{providerId,remoteId:d.remoteId.trim(),name:d.name.trim()||d.remoteId.trim(),embedding,status:'ready',state:'示例已验证'});
  if(previous!==fingerprint(model))App.state.indexBuilt=false;
  App.state.verifiedModels[model.id]=true;App.state.roles.textEmbed=model.id;App.state.providerId=providerId;App.state.chainVerified=false;App.state.indexEnabled=true;
  embeddingSetupSecret='';App.state.embeddingDraft=null;navigate('understand');toast('已在原型中保存并设为默认 Embedding；尚未建立新的媒体索引。');
}
const EmbeddingSetupViews={embedding(){return embeddingSetupView();},embeddingAdd(){return embeddingSetupView(true);}};
