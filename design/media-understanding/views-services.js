function service(id=App.state.providerId){const original=PROVIDERS.find(p=>p.id===id)||PROVIDERS[0];return {...original,...App.state.providerOverrides[original.id]};}
function modelReady(m){return m.status==='ready'&&(!m.providerId||(service(m.providerId).enabled&&(!App.state.dirtyProviders.includes(m.providerId)||App.state.verifiedModels[m.id])));}
function modelLabel(m){return m.name+' · '+(m.providerId?service(m.providerId).name:'本机');}
function serviceTabs(active='models'){
  return `<div class="service-tabs" role="tablist">${[['models','模型服务'],['routing','默认模型'],['local','本地模型']].map(([id,label])=>`<button class="${id===active?'active':''}" data-nav="${id}">${label}</button>`).join('')}</div>`;
}
function serviceRow(p){
  const rows=MODELS.filter(m=>m.providerId===p.id);
  return `<button class="provider-row" data-provider="${p.id}"><span class="provider-mark">${p.mark}</span><span><b>${p.name}</b><small>${PROTOCOLS[p.protocol].name} · ${rows.length} 个模型</small></span><span class="provider-state ${p.enabled?'success':'muted'}">${p.enabled?'已启用':'未启用'}</span>${icon('chevron',13)}</button>`;
}
function providerModels(p,compact=false){
  const rows=MODELS.filter(m=>m.providerId===p.id);
  return `<div class="provider-models"><div class="between"><h3>模型 <small>${rows.length}</small></h3><div class="row">${UI.button('同步模型','sync-models','small quiet','refresh')}${UI.button('＋','add-provider-model','small quiet')}</div></div>${rows.map(m=>`<div class="provider-model-row"><span class="provider-model-symbol">${m.name[0]}</span><div><b>${m.name}</b><small>${m.remoteId}</small>${compact?'':`<div class="cap-line">${m.caps.map(c=>UI.pill(CAP_LABELS[c])).join('')}</div>`}</div><button class="icon-button" data-model-settings="${m.id}" aria-label="配置 ${m.name}">${icon('settings',15)}</button></div>`).join('')||'<p class="helper">同步列表，或手动添加模型 ID。</p>'}</div>`;
}
const ServiceViews={
  models(){
    return UI.settingsWindow(`${serviceTabs()}<div class="between"><p class="helper">统一管理服务地址、认证和服务内模型。</p>${UI.button('添加服务','add-service','small quiet','plus')}</div><div class="provider-list">${PROVIDERS.map(p=>serviceRow(service(p.id))).join('')}</div><div class="service-tip">${icon('info',14)}<span>一个服务可提供多个模型；多个用途可复用同一个模型。</span></div><div class="service-shortcuts">${UI.button('部署 Mage／Qwen','remote-guide','small quiet','cloud')}${UI.button('管理本地模型','local','small quiet','download')}</div>`,
      {title:'服务先配置一次，模型按需选',notes:[['参考 Cherry Studio 的组织','连接属于模型服务，模型列表属于服务；业务默认模型只引用已有模型，不再按媒体／文字重复填写地址和密钥。'],['适配当前窄设置窗口','保留原独立 600×620 窗口与左侧导航。右侧用“模型服务／默认模型／本地模型”三项页内导航，服务列表进入详情。'],['按协议接入','自建 Mage、Qwen 优先采用 OpenAI Chat Completions；Responses 与 Messages 由各自适配器处理。']]});
  },
  api(){
    const p=service();const protocol=PROTOCOLS[p.protocol];
    return UI.settingsWindow(`${serviceTabs()}<div class="provider-heading"><button class="text-button" data-nav="models">‹ 模型服务</button><strong>${UI.escape(p.name)}</strong><label class="provider-switch"><input type="checkbox" data-service-field="enabled" ${p.enabled?'checked':''}><span>启用</span></label></div><div class="provider-connection"><label><span>名称</span><input class="input" data-service-field="name" value="${UI.escape(p.name)}" aria-label="服务名称"></label><label><span>协议</span><select id="service-protocol" class="select" data-service-field="protocol" aria-label="服务协议">${Object.entries(PROTOCOLS).map(([id,v])=>`<option value="${id}" ${id===p.protocol?'selected':''}>${v.name}</option>`).join('')}</select></label><label><span>API 密钥</span><input class="input" type="password" placeholder="系统凭据存储；留空保留" autocomplete="off" aria-label="服务 API 密钥"></label><label class="provider-address"><span>API 地址</span><input class="input mono" data-service-field="baseUrl" value="${UI.escape(p.baseUrl)}" aria-label="服务 API 地址"></label><p class="endpoint-preview" id="endpoint-preview">请求地址：${UI.escape(p.baseUrl.replace(/\/$/,''))}/${protocol.path}</p></div>${providerModels(p,true)}<details class="provider-details"><summary>数据传输与高级设置</summary><p class="helper">${p.location}；回环地址可以是远程服务的 SSH 隧道。</p><label class="check"><input type="checkbox" checked><span>允许向此服务发送选定媒体与文字</span></label><label class="check"><input type="checkbox"><span>允许后台批量分析</span></label></details><div class="test-status" id="api-status">${icon(App.state.apiTested?'check':'info',14)}<span>${App.state.apiTested?'模拟连接测试通过；图文、采样帧与结构化能力仍按模型分别验证。':'列表可以同步，模型 ID 也可手填；连接成功不等于所有模型能力通过。'}</span></div>`,
      {footer:`<span class="helper">连接由服务统一管理</span><div class="row">${UI.button('保存','save-api','quiet')}${UI.button('保存并检测','test-api','primary','play')}</div>`,title:'地址和密钥属于服务',notes:[['一处配置，多处复用','服务详情包含协议、API 基础地址、密钥、启用状态和模型列表。媒体理解、生成和向量的默认项都从模型中选择。'],['真实请求预览','Chat 是 /v1/chat/completions，Responses 是 /v1/responses，Messages 是 /v1/messages；请求体和流事件也要分别转换。'],['你的远程部署场景','服务器运行模型，本机通过 SSH 隧道访问回环地址。示例协议和端口与部署脚本一致；本机无需安装模型权重。']]});
  },
  catalog(){
    const p=service();
    return UI.settingsWindow(`${serviceTabs()}${configBack('服务模型列表')}<h3>${p.name}</h3>${providerModels(p)}<p class="helper">同步只更新模型清单，不改变默认模型；不支持列举的服务可手动添加 ID。</p><div class="service-tip">${icon('info',14)}<span>能力来自声明与实测。名字带“VL”或列表接口成功，不代表已经验证视频与结构化输出。</span></div>`,
      {footer:`<span class="helper">API 不在本机下载权重</span>${UI.button('服务连接设置','api','quiet','settings')}`,title:'模型列表归属当前服务',notes:[['同步与手填并存','类似参考图，模型按服务展示；支持能力标记与单独编辑。模型列表不能提供能力时，保留未验证状态。'],['模型不重复存连接','模型条目保存服务内 ID、显示名、能力与参数覆盖。连接地址和密钥从上级服务继承。'],['一个进程通常服务一个模型','脚本分别启动 Mage :8000 和 Qwen :8001；每个地址单独配置服务。需要同端点多模型时才引入路由网关。']]});
  },
  model(){
    const m=UI.model(App.state.editingModelId),p=service(m.providerId);
    return UI.settingsWindow(`${serviceTabs()}${configBack('模型设置')}${configCard(m.name,`${UI.field('所属服务',`<div class="inherited-value">${p.name} · 继承地址与密钥</div>`)}${UI.field('模型 ID',`<input class="input mono" value="${m.remoteId||m.id}" aria-label="模型 ID">`)}${UI.field('显示名称',`<input class="input" value="${m.name}" aria-label="模型显示名称">`)}${UI.field('接口',`<div class="inherited-value">继承服务 · ${PROTOCOLS[p.protocol].name}</div>`)}`)}${configCard('模型能力',`<div class="model-capabilities">${['text','image','video_frames','json','embed_text','embed_image'].map(c=>`<label><input type="checkbox" ${m.caps.includes(c)?'checked':''}><span>${CAP_LABELS[c]}</span></label>`).join('')}</div><p class="helper">标记用于筛选，启用业务前仍需实际测试。</p><div class="row wrap">${UI.button('检测图文能力','check-model','small quiet','play')}${UI.button('检测采样帧','check-model','small quiet','video')}</div>`)}<details class="provider-details"><summary>上下文、输出与模型参数</summary><p class="helper">模型级参数覆盖服务默认值；不重复配置认证。图文上限按实际后端记录。</p></details>`,
      {footer:`<span class="helper">修改能力会更新候选用途</span>${UI.button('保存模型','save-model','primary','check')}`,title:'模型只管理自己的属性',notes:[['能力重叠不复制模型','一个 Mage／Qwen 模型可以同时支持语言生成、图片理解和采样帧理解，默认用途只引用它。'],['视频能力说清传输方式','采样帧是标准多图输入加时间说明；原生视频、精确时间定位与 Mage codec 另行声明，不混作一个图标。'],['Embedding 仍是独立契约','标准文本 embeddings 与图文／视频向量扩展分开验证；不能让所有生成模型都作为向量模型。']]});
  },
  routing(){
    return UI.settingsWindow(`${serviceTabs('routing')}<p class="helper">从已启用服务或本地模型中选择，可为多个用途选择同一项。</p><div class="default-model-settings">${configCard('媒体理解',`${compactRole('图片理解','image','image')}${compactRole('视频理解','video','video_frames','当前使用带时间说明的采样帧。')}`)}${configCard('语言生成',`${compactRole('文字生成／回答','text','text','可与媒体理解使用同一个模型。')}`)}${configCard('Embedding',`${compactRole('描述文本向量','textEmbed','embed_text')}${compactRole('多模态向量（可选）','visualEmbed','embed_image','只显示具备相应对齐查询契约的模型。')}`)}</div>`,
      {footer:`<span class="helper">修改只影响新任务</span>${UI.button('保存默认模型','save-routes','primary','check')}`,title:'默认模型仅保存引用',notes:[['用途与连接分开','这里不再出现密钥、基础地址或引擎选择，只选择“服务＋模型”。修改连接到模型服务页处理。'],['按能力过滤候选','图片、视频、文字与向量分别筛选。服务关闭或模型未验证时不可选；不会自动切到其他外部服务。'],['原业务链路保持一致','离线理解、建立索引、搜索和问答使用固定任务快照；角色能复用模型，不要求重复推理。']]});
  },
  local(){
    if(App.state.localDetails)return ConfigViews.local();
    const rows=MODELS.filter(m=>m.mode==='本机');
    return UI.settingsWindow(`${serviceTabs('local')}<p class="helper">本地执行与模型服务分开管理；已有专用识别继续保留。</p>${configCard('本机模型',rows.map(m=>`<div class="local-model-row"><div><b>${m.name}</b><small>${m.state}</small></div>${UI.button(m.id==='mage-local'?'准备资源':'配置','local-runtime','small quiet','settings')}</div>`).join(''))}${configCard('其他模型与资源',`<p class="helper">按已适配模型包下载引擎和权重。没有适用资源包时，可连接已部署的远程服务。</p>${UI.button('查看本机适配包','local-runtime','small quiet','download')}`)}<div class="service-tip">${icon('cloud',14)}<span>本机性能不足时，在远程 GPU 部署 Mage／Qwen，然后在“模型服务”里连接 API。</span></div>`,
      {title:'保留本地模型，远程服务并列',notes:[['不是把本机能力删掉','CLIP、人脸、OCR、对象等已有本机机制保留；资源下载与硬件配置继续服务于本机模型。'],['当前先走服务器路线','针对你的设备，优先部署远程标准 API；Lap 无需为远程推理配置 Python、CUDA 或模型下载。'],['同一能力，不同执行位置','业务仍按默认模型调用；执行位置、数据传输和模型能力分别记录。']]});
  },
};
