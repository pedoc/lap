// Extend the current Settings.vue / AiModelSettings.vue layout.
const TASKS=[['semantic','图文语义检索'],['face','人脸识别'],['ocr','文字识别'],['detection','对象检测'],['media','媒体理解'],['text','语言生成'],['embedding','Embedding']];
function taskButtons(baseline=false){
  return `<div class="settings-task-buttons" aria-label="AI 任务">${TASKS.slice(0,baseline?4:7).map(([id,label])=>`<button class="button ${id===(baseline?'semantic':App.state.task)?'primary':'quiet'}" ${baseline?'data-action="baseline-control"':`data-task="${id}"`}>${label}</button>`).join('')}</div>`;
}
function configIntro(){return `<div class="settings-intro"><p>模型配置全局共用，按能力分配用途。</p><button class="text-button" data-nav="routing">默认用途</button></div>${taskButtons()}`;}
function configBack(label){return `<div class="settings-subhead"><button class="text-button" data-nav="models">‹ 返回任务与模型</button><span>${label}</span></div>`;}
function configCard(title,body){return `<section class="setting-card"><h3>${title}</h3>${body}</section>`;}
function compactRole(label,role,cap,help=''){return `<div class="compact-role"><label for="route-${role}">${label}</label><select id="route-${role}" class="select" data-role="${role}">${UI.options(cap,App.state.roles[role])}</select>${help?`<p class="helper">${help}</p>`:''}</div>`;}
const ConfigViews={
  baseline(){
    return UI.settingsWindow(`<p class="helper">每类任务选择一个全局模型，所有图库共用；每个模型独立保存参数。</p>${taskButtons(true)}
      ${configCard('选择要配置的模型',`<select class="select" aria-label="当前模型示意"><option>CLIP · 图文语义检索</option></select><span>全局使用的模型：CLIP</span><p class="helper">选中模型后在下方配置。就绪模型会在验证及必要的确认后全局切换。</p>`)}
      ${configCard('模型配置：CLIP <span class="pill brand">已启用</span>',`<p class="helper">clip · 512D</p><p class="helper">已保存模型配置：推理契约已验证</p><p>执行后端：ONNX Runtime</p><p>已确认的物理 GPU：待检测</p><p class="helper">自动模式优先尝试可用加速，失败时显示原因并回退 CPU。</p>${UI.button('重新检测硬件','baseline-control','small quiet')}<div class="setting-parameter"><span>执行设备</span><select class="select" aria-label="执行设备示意"><option>自动（优先 GPU）</option></select></div><div class="setting-parameter"><span>每个模型会话的 CPU 线程数</span><input class="input" value="4" aria-label="线程数示意"></div><div class="row wrap">${UI.button('恢复默认值','baseline-control','small quiet')}${UI.button('保存参数','baseline-control','small primary')}${UI.button('保存并测试','baseline-control','small quiet')}</div><p class="helper">保存参数不会切换模型。测试会先保存当前参数，再实际运行模型。</p>`)}
      ${configCard('模型目录','<p class="helper">下载、打开模型目录和导入清单位于此处。</p>')}`,
      {baseline:true,title:'先对齐现有设置',notes:[['独立窗口','从主窗口齿轮打开，复用 settings 窗口。基准 600×620，并随设置比例缩放。'],['导航与内容区','左侧仍为现有 9 个分类，AI 模型位于 RAW 下方。右侧单列滚动，不带主应用图标栏。'],['AI 模型页现状','任务按钮 → 模型选择 → 参数与测试 → 模型目录／下载 → 导入清单。图中模型和设备值为示例。']]});
  },
  models(){
    const task=App.state.task;
    const items=task==='media'?MODELS.filter(m=>m.caps.includes('image')):task==='text'?MODELS.filter(m=>m.caps.includes('text')):task==='embedding'?MODELS.filter(m=>m.caps.includes('embed_text')):task==='semantic'?MODELS.filter(m=>m.id==='clip-local'):task==='ocr'?MODELS.filter(m=>m.id==='ocr-local'):[];
    const role={media:'image',text:'text',embedding:'textEmbed'}[task];
    const m=items.find(m=>m.id===App.state.roles[role])||items[0];
    return UI.settingsWindow(`${configIntro()}
      ${configCard('选择要配置的模型',`<select class="select" id="configured-model" aria-label="选择模型部署">${items.length?items.map(row=>`<option value="${row.id}" ${row.id===m.id?'selected':''}>${row.name} · ${row.mode==='API'?'API':'本机'}${row.status==='ready'?'':'（待准备）'}</option>`).join(''):'<option>保留现有人脸／对象模型配置</option>'}</select><div class="between"><span>本用途使用：${m?.name||'现有配置'}</span><button class="text-button" data-nav="routing">调整</button></div><p class="helper">同一部署可用于多个任务，切换用途不重复添加模型。</p>`)}
      ${configCard(`模型配置：${m?.name||'现有专用模型'}`,`<div class="cap-line">${UI.pill(m?.mode==='API'?'API · 指定内网':'本机运行','local')}${UI.pill(m?.state||'原有参数与索引','ok')}</div><p class="helper">${m?.family||'人脸／对象继续使用各自识别契约'}</p><div class="cap-line">${m?m.caps.map(c=>UI.pill(CAP_LABELS[c])).join(''):UI.pill('专用识别')}</div><p class="helper">能力按具体部署测试。图片输入与帧序列视频分别验收。</p><div class="row wrap">${UI.button(m?.mode==='API'?'编辑 API 配置':'编辑本机配置',m?.mode==='API'?'api':'local','small quiet','settings')}${UI.button('保存并测试','test-config','small quiet','play')}</div>`)}
      ${configCard('模型目录',`<p class="helper">添加本机模型或连接 API，已配置项可用于多个任务。</p><div class="row wrap">${UI.button('查看目录／添加模型','catalog','small quiet','plus')}${UI.button('本机模型','local','small quiet','download')}${UI.button('API 服务','api','small quiet','cloud')}</div>`)}`,
      {title:'新增用途，保留原表单',notes:[['保留现有四类任务','原有图文检索、人脸、OCR、对象四项不动；新增媒体理解、语言生成、Embedding，按钮允许换行。'],['按能力筛选部署','这三项是用途，不是互斥模型类别。一个多模态模型可同时承担理解与生成，不需要重复配置。'],['组件改动集中','扩展 AiModelSettings 的任务选择与适配器。默认用途、资源和连接配置都在这一个窗口内编辑。']]});
  },
  local(){
    const mage=App.state.localModel==='mage',ready=App.state.localReady;
    const engines=[['onnx','原生 ONNX／GenAI'],['native','原生推理服务'],['python','受管 Python 服务']];
    return UI.settingsWindow(`${configBack('本机模型配置')}
      ${configCard('模型与执行方式',`${UI.field('模型资源包',`<select id="local-model" class="select" aria-label="本机模型资源包"><option value="mage" ${mage?'selected':''}>Mage-VL · 示例适配包</option><option value="qwen" ${!mage?'selected':''}>Qwen-VL · 示例适配包</option></select>`)}<div class="compact-engines">${engines.map(([id,label])=>`<label class="compact-engine ${App.state.localEngine===id?'chosen':''}"><input type="radio" name="engine" value="${id}" ${App.state.localEngine===id?'checked':''} ${mage&&id!=='python'?'disabled':''}><span>${label}</span><small>${mage&&id!=='python'?'当前包未适配':App.state.localEngine===id?'当前选择':'按适配包验收'}</small></label>`).join('')}</div><p class="helper">引擎由检查点、格式和适配包决定；不代表所有 Qwen／Mage 版本均已兼容。</p>`)}
      ${configCard('资源与运行状态',`<div class="compact-resource"><span>隔离运行环境</span><b class="${ready?'success':'warning'}">${ready?'示例已准备':'待准备'}</b></div><div class="compact-resource"><span>模型权重与处理配置</span><b>${ready?'示例已安装':'待下载'}</b></div><div class="compact-resource"><span>设备／显存／实际推理</span><b>待检测</b></div><p class="helper">环境与权重分别下载、校验；不修改系统 Python。</p><button class="text-button" data-action="probe-local">重新检测硬件与加载能力</button>`)}
      ${configCard('模型参数','<div class="setting-parameter"><span>执行设备</span><select class="select" aria-label="生成模型执行设备"><option>自动</option><option>CPU</option><option>GPU</option></select></div><p class="helper">由适配器提供，仅显示该部署支持的参数。</p>')}`,
      {footer:`<span class="helper">下载后仍需测试</span><div class="row">${UI.button(ready?'重新准备':'准备环境并下载','prepare-local','primary','download')}${UI.button('保存并测试','test-config','quiet')}</div>`,title:'沿用下载与设备配置',notes:[['在当前 AI 页编辑','选中本机模型后，显示执行方式、资源、硬件与参数。单列表单向下滚动，底部保留本次操作。'],['环境和模型分别准备','现有 ONNX 资源机制服务已适配模型。其他后端以独立资源包和受管进程扩展，共用下载与进度交互。'],['设备条件单独报告','本机跑不动不等于模型不能用；仍可另建 API 部署，连接已配置的工作站或服务器。']]});
  },
  api(){
    return UI.settingsWindow(`${configBack('API 模型配置')}
      ${configCard('连接配置',`${UI.field('配置名称','<input class="input" value="视觉工作站" aria-label="API 配置名称">')}${UI.field('协议','<select class="select" aria-label="调用协议"><option>兼容对话生成协议</option><option>适配器专用协议</option></select>')}${UI.field('API 地址','<input class="input mono" value="https://vision-workstation.lan/v1" aria-label="示例 API 地址">')}${UI.field('服务端模型 ID','<input class="input" value="vision-model-demo" aria-label="服务端模型 ID">')}${UI.field('API Key（系统凭据存储）','<input type="password" class="input" placeholder="留空保留已保存密钥" autocomplete="off" aria-label="API 密钥">')}<div class="setting-parameter"><span>服务位置</span><select class="select" aria-label="服务执行位置"><option>指定内网</option><option>本机</option><option>外网</option></select></div>`)}
      ${configCard('传输与能力',`${UI.field('视频输入','<select class="select" aria-label="视频传输方式"><option>抽帧＋原视频时间信息</option><option>服务端支持时发送原片</option></select>')}<label class="check"><input type="checkbox" checked><span>允许向此服务发送选定的图片／文字</span></label><label class="check"><input type="checkbox"><span>允许后台批量媒体分析</span></label><div class="test-status" id="api-status">${icon(App.state.apiTested?'check':'info',14)}<span>${App.state.apiTested?'模拟通过：文字、图片、帧序列视频与结构化输出；精确时间定位待验证。':'待测试：按输入类型分别验证能力。'}</span></div>`)}`,
      {footer:`<span class="helper">保存不自动启用</span><div class="row">${UI.button('保存参数','save-api','quiet')}${UI.button('保存并测试','test-api','primary','play')}</div>`,title:'API 是另一种部署方式',notes:[['沿用现有 API 表单','地址、模型 ID、凭据、上传许可保留在配置卡内；新增协议、服务位置和输入能力验证。'],['服务位置独立记录','API 可以连接本机、指定内网或外部服务。上传许可和后台批量许可分别设置，不凭回环地址判断实际推理位置。'],['保留保存语义','保存参数不切换用途。保存并测试先保存再测试；失败不撤销配置，未验证能力暂不可启用。']]});
  },
  routing(){
    return UI.settingsWindow(`${configBack('默认用途')}<p class="helper">选择已配置、可使用的部署。同一个部署可以重复使用。</p>
      ${configCard('媒体理解与语言生成',`${compactRole('图片理解','image','image')}${compactRole('视频理解','video','video_frames','帧序列与原生视频能力分别验收。')}${compactRole('语言生成／基于证据回答','text','text','解析查询、润色说明、读取证据回答；可与理解模型相同。')}`)}
      ${configCard('Embedding 与检索',`${compactRole('分析文本向量','textEmbed','embed_text')}${compactRole('图片／片段向量（可选）','visualEmbed','embed_image','文字找媒体还须具备匹配的查询编码。')}<p class="helper">切换向量模型建立新索引，保留已有分析与人工资料。</p>`)}
      ${configCard('已有识别任务','<p class="helper">图文检索、人脸、OCR、对象继续使用各任务页原有选择与专用参数。</p>')}`,
      {footer:`<span class="helper">新任务使用新绑定</span>${UI.button('保存默认用途','save-routes','primary','check')}`,title:'按用途决定调用对象',notes:[['点击理解时有固定默认','图片走图片理解绑定，视频走视频理解绑定。无需每次在 Mage 与其他多模态模型之间选择。'],['能力重叠可以复用','同一 VLM 可绑定理解与生成。纯文字模型读取既有证据回答；需要看画面时调用对应理解部署。'],['任务保存配置快照','提交时固定部署、版本和方案。修改默认用途只影响新任务；不可用时给出原因和设置入口。']]});
  },
  catalog(){
    return UI.settingsWindow(`${configBack('模型目录 · 同页向下滚动')}<p class="helper">上方仍为模型选择与参数。此处展示原页面下部的目录、下载与导入区域。</p>
      ${configCard('模型目录',MODELS.map(m=>`<div class="compact-catalog-row"><div class="between"><b>${m.name}</b>${UI.pill(m.mode==='API'?'API':m.location,m.status==='ready'?'ok':'warn')}</div><p class="helper">${m.family}</p><div class="cap-line">${m.caps.slice(0,3).map(c=>UI.pill(CAP_LABELS[c])).join('')}</div><div class="between"><span class="helper">${m.state}</span>${UI.button(m.mode==='API'?'编辑连接':m.id==='mage-local'?'准备／下载':'查看配置',m.mode==='API'?'api':'local','small quiet')}</div></div>`).join(''))}
      ${configCard('导入模型清单',`<p class="helper">保留已适配模型的清单、文件哈希与资源来源。</p>${UI.button('导入模型清单','import-manifest','small quiet')}`)}`,
      {footer:`<span class="helper">模型可有多个部署</span><div class="row">${UI.button('添加本机模型','local','quiet','download')}${UI.button('添加 API','api','primary','plus')}</div>`,title:'目录保留在页面下方',notes:[['没有第二套左导航','本机、API 和默认用途是 AI 模型页内部编辑状态，不是新增设置分类。'],['本机与 API 可以并存','Mage 本机、Mage 内网 API、其他生成或向量模型可以同时登记，用途绑定选择当前调用对象。'],['只提供已定义适配包','模型名、ONNX 扩展名、API 模型列表都不能代替能力测试。新架构仍需新增适配器。']]});
  },
};
