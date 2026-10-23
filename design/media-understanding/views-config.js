// Extend the current Settings.vue / AiModelSettings.vue layout.
const TASKS=[['semantic','图文语义检索'],['face','人脸识别'],['ocr','文字识别'],['detection','对象检测'],['media','媒体理解'],['text','语言生成'],['embedding','Embedding']];
function taskButtons(baseline=false){
  return `<div class="settings-task-buttons" aria-label="AI 任务">${TASKS.slice(0,baseline?4:7).map(([id,label])=>`<button class="button ${id===(baseline?'semantic':App.state.task)?'primary':'quiet'}" ${baseline?'data-action="baseline-control"':`data-task="${id}"`}>${label}</button>`).join('')}</div>`;
}
function configIntro(){return serviceTabs('local');}
function configBack(label){const local=App.screen==='local';return `<div class="settings-subhead"><button class="text-button" data-nav="${local?'models':'services'}">‹ 返回${local?'AI 模型':'模型服务'}</button><span>${label}</span></div>`;}
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
  local(){
    const mage=App.state.localModel==='mage',ready=App.state.localReady;
    const engines=[['onnx','原生 ONNX／GenAI'],['native','原生推理服务'],['python','受管 Python 服务']];
    return UI.settingsWindow(`${configIntro()}${configBack('本机模型配置')}
      ${configCard('模型与执行方式',`${UI.field('模型资源包',`<select id="local-model" class="select" aria-label="本机模型资源包"><option value="mage" ${mage?'selected':''}>Mage-VL · 示例适配包</option><option value="qwen" ${!mage?'selected':''}>Qwen-VL · 示例适配包</option></select>`)}<div class="compact-engines">${engines.map(([id,label])=>`<label class="compact-engine ${App.state.localEngine===id?'chosen':''}"><input type="radio" name="engine" value="${id}" ${App.state.localEngine===id?'checked':''} ${mage&&id!=='python'?'disabled':''}><span>${label}</span><small>${mage&&id!=='python'?'当前包未适配':App.state.localEngine===id?'当前选择':'按适配包验收'}</small></label>`).join('')}</div><p class="helper">引擎由检查点、格式和适配包决定；不代表所有 Qwen／Mage 版本均已兼容。</p>`)}
      ${configCard('资源与运行状态',`<div class="compact-resource"><span>隔离运行环境</span><b class="${ready?'success':'warning'}">${ready?'示例已准备':'待准备'}</b></div><div class="compact-resource"><span>模型权重与处理配置</span><b>${ready?'示例已安装':'待下载'}</b></div><div class="compact-resource"><span>设备／显存／实际推理</span><b>待检测</b></div><p class="helper">环境与权重分别下载、校验；不修改系统 Python。</p><button class="text-button" data-action="probe-local">重新检测硬件与加载能力</button>`)}
      ${configCard('模型参数','<div class="setting-parameter"><span>执行设备</span><select class="select" aria-label="生成模型执行设备"><option>自动</option><option>CPU</option><option>GPU</option></select></div><p class="helper">由适配器提供，仅显示该部署支持的参数。</p>')}`,
      {footer:`<span class="helper">下载后仍需测试</span><div class="row">${UI.button(ready?'重新准备':'准备环境并下载','prepare-local','primary','download')}${UI.button('保存并测试','test-config','quiet')}</div>`,title:'沿用下载与设备配置',notes:[['在当前 AI 页编辑','选中本机模型后，显示执行方式、资源、硬件与参数。单列表单向下滚动，底部保留本次操作。'],['环境和模型分别准备','现有 ONNX 资源机制服务已适配模型。其他后端以独立资源包和受管进程扩展，共用下载与进度交互。'],['设备条件单独报告','本机跑不动不等于模型不能用；仍可另建 API 部署，连接已配置的工作站或服务器。']]});
  },
};
