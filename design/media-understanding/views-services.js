function service(id=App.state.providerId){const original=PROVIDERS.find(p=>p.id===id)||PROVIDERS[0];return {...original,...App.state.providerOverrides[original.id]};}
function modelReady(m){return m.status==='ready'&&(!m.providerId||(service(m.providerId).enabled&&(!App.state.dirtyProviders.includes(m.providerId)||App.state.verifiedModels[m.id])));}
function modelLabel(m){return m.name+' · '+(m.providerId?service(m.providerId).name:'本机');}
function providerModels(p){
  const rows=MODELS.filter(m=>m.providerId===p.id);
  return `<div class="provider-models"><div class="between"><span class="helper">${rows.length} 个模型</span><div class="row">${UI.button('同步模型','sync-models','small quiet','refresh')}${UI.button('添加模型','add-provider-model','small quiet','plus')}</div></div>${rows.map(m=>`<div class="provider-model-row"><div><b>${UI.escape(m.name)}${!modelReady(m)?` <small>${UI.escape(m.state)}</small>`:''}</b><div class="cap-line">${m.caps.filter(c=>c!=='json').map(c=>UI.pill(CAP_LABELS[c])).join('')}</div></div><button class="btn btn-xs" data-model-settings="${m.id}" aria-label="配置 ${UI.escape(m.name)}">配置</button></div>`).join('')||'<p class="helper">同步模型列表，或手动添加模型 ID。</p>'}</div>`;
}
function serviceConnection(p){
  return `<div class="provider-connection">${UI.field('服务名称',`<input class="input" data-service-field="name" value="${UI.escape(p.name)}" aria-label="服务名称">`)}${UI.field('API 基础地址',`<input class="input" data-service-field="baseUrl" value="${UI.escape(p.baseUrl)}" aria-label="服务 API 地址">`)}${UI.field('API 密钥',`<input class="input" type="password" placeholder="留空保留已保存的密钥" autocomplete="off" aria-label="服务 API 密钥">`)}<p class="endpoint-preview" id="endpoint-preview">OpenAI 兼容 · ${UI.escape(p.baseUrl.replace(/\/$/,''))}/chat/completions · /embeddings</p><div class="row wrap">${UI.button('保存连接','save-api','small primary')}${UI.button('检测连接','test-api','small quiet')}<span class="helper" id="api-status">${App.state.apiTested?'模拟连接检测通过；模型能力仍需分别检测。':''}</span></div><details class="provider-details"><summary>数据传输与高级设置</summary><label class="check"><input type="checkbox" checked><span>允许发送本次选定的媒体与文字</span></label><label class="check"><input type="checkbox"><span>允许后台批量处理</span></label></details></div>`;
}
function mediaSettings(connection=false){
  App.state.task='media';
  const expanded=connection?App.state.providerId:null;
  const services=PROVIDERS.map((original,index)=>{
    const p=service(original.id),open=expanded===p.id;
    return `<section class="setting-card service-card"><div class="between"><div><strong>${UI.escape(p.name)}</strong><p class="helper">${UI.escape(p.baseUrl)}</p></div><div class="row"><label class="provider-switch"><input type="checkbox" data-provider-enabled="${p.id}" ${p.enabled?'checked':''}><span>启用</span></label><button class="btn btn-xs" data-provider="${p.id}">${open?'连接设置':'配置连接'}</button></div></div>${open?serviceConnection(p):''}<details ${index===0||open?'open':''}><summary>${MODELS.filter(m=>m.providerId===p.id).length} 个模型 · 查看能力</summary>${providerModels(p)}</details></section>`;
  }).join('');
  return UI.settingsWindow(`<p class="helper">每类任务选择全局模型，所有图库共用；媒体理解统一管理服务与默认模型。</p>${taskButtons()}<div class="between"><h3 class="text-sm font-semibold">模型服务</h3>${UI.button('添加服务','add-service','small quiet','plus')}</div><p class="helper">连接 OpenAI 兼容 API，可使用自建或托管服务。</p><div class="space-y-3">${services}</div>${configCard('默认模型',`${compactRole('媒体理解','image','image')}${compactRole('语言生成','text','text')}${compactRole('Embedding','textEmbed','embed_text')}`)}<div class="row">${UI.button('保存默认模型','save-routes','primary')}<span class="helper">修改用于新任务</span></div>`,{title:'一个媒体理解标签页',notes:[['服务、模型、默认用途三层','上方管理多个 OpenAI 兼容后端，每个后端管理多个模型；下方只有媒体理解、语言生成、Embedding 三个默认选择，不新增语言生成或 Embedding 标签页。'],['部署位置与品牌无关','服务只填写名称、API 基础地址和密钥。可以连接任意已部署的兼容接口；示例多模型服务可以是具有模型路由能力的网关。'],['首版范围收敛','首版只连接 API，不提供 Mage／Qwen 本机安装、环境下载或引擎选择；现有人脸、OCR、对象、CLIP 本机能力继续保留。'],['能力标记与验证','图片、视频、文本生成、文本 Embedding、多模态 Embedding 是可重叠的模型能力。默认列表按能力筛选；接口检测不代替能力检测。多模态向量输入扩展尚未验收时保持不可选。'],['视频与检索边界','首版视频以采样帧和时间说明输入；单个媒体默认模型同时引用到图片和视频用途，若不支持视频则在任务预检明确提示。Embedding 不要求每次搜索重新理解媒体。']]});
}
const ServiceViews={
  api(){return mediaSettings(true);},
  services(){return mediaSettings();},
  catalog(){return mediaSettings();},
  routing(){return mediaSettings();},
  model(){
    App.state.task='media';
    const m=UI.model(App.state.editingModelId),p=service(m.providerId);
    return UI.settingsWindow(`<p class="helper">每类任务选择全局模型，所有图库共用；媒体理解统一管理服务与默认模型。</p>${taskButtons()}<button class="text-button" data-nav="understand">‹ 返回服务与默认模型</button>${configCard('模型属性',`${UI.field('所属服务',`<div class="inherited-value">${UI.escape(p.name)} · 继承地址与密钥</div>`)}${UI.field('模型 ID',`<input class="input" data-model-field="remoteId" value="${UI.escape(m.remoteId)}" aria-label="模型 ID">`)}${UI.field('显示名称',`<input class="input" data-model-field="name" value="${UI.escape(m.name)}" aria-label="模型显示名称">`)}`)}${configCard('模型能力',`<div class="model-capabilities">${['image','video_frames','text','embed_text','embed_multimodal','json'].map(c=>`<label><input type="checkbox" data-model-cap="${c}" ${m.caps.includes(c)?'checked':''}><span>${CAP_LABELS[c]}</span></label>`).join('')}</div><p class="helper">能力可多选；用于筛选默认模型，实际可用性还需检测。</p><p class="helper">视频当前使用带时间说明的采样帧。多模态 Embedding 需要验证服务端的输入扩展。</p><div class="row">${UI.button('检测已标记能力','check-model','small quiet')}${UI.button('保存模型','save-model','small primary')}</div>`)}<details class="provider-details"><summary>模型参数</summary><p class="helper">设置上下文、输出长度和超时；连接信息继承服务。</p></details>`,{title:'同一个模型可以标记多项能力',notes:[['模型能力不等于业务分类','Mage／Qwen-VL 可同时提供图片、视频采样帧与文本生成能力，一次配置即可在多个用途复用。'],['不会将能力标记当成测试','更改能力或模型 ID 会使验证状态失效；按具体部署分别检测。文本模型不进入媒体理解候选，生成模型不自动进入 Embedding 候选。'],['向量接口保留明确边界','多模态 Embedding 标记保留，但不在首版承诺任意服务都能经标准文本 /embeddings 接收图片或视频。']]});
  },
};
