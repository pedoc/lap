function flowNode(title,body,view='',kind='') {
  return `<button class="flow-node ${kind}" ${view?`data-nav="${view}"`:'data-action="demo"'}><b>${title}</b><span>${body}</span></button>`;
}
const FlowViews={
  flow() {
    return UI.shell(`${UI.heading('开发流程与数据边界','准备一个生成服务和一个文本向量服务，再完成理解、入库、查询与问答。',UI.button('验证业务链路','verification','quiet','check')+UI.button('查看界面目录','gallery','quiet','grid'))}
      <div class="blueprint"><section class="flow-lane"><div class="lane-label"><span class="step-badge">A</span><div><h3>后台建立索引</h3><p>重计算发生在明确提交的任务中</p></div></div><div class="flow-row">${flowNode('服务与任务预检','Mage :8000 或 Qwen :8001；文本向量 :8002；固定媒体范围','verification')}${flowNode('结构化媒体分析','图片描述／视频片段／标题与标签建议','viewer')}${flowNode('保存分析记录','AnalysisRecord＋覆盖范围＋证据来源','workspace')}${flowNode('文本 Embedding','对已保存的分析文字编码，绑定索引身份','embedding')}${flowNode('可检索记录','描述向量保存后，匹配查询编码方可检索','search')}</div><div class="flow-foot">可选分支：从已保存分析提议填充标题、说明、标签；只按显式写入规则保存，不由问答自动触发。</div></section>
      <section class="flow-lane"><div class="lane-label"><span class="step-badge">B</span><div><h3>自然语言搜索</h3><p>查询索引，不逐项重新分析媒体</p></div></div><div class="flow-row">${flowNode('用户问题','文字或自然语言搜索入口','search')}${flowNode('可选：条件解析','生成受控 QueryPlan，应用校验','search')}${flowNode('匹配的查询编码','相同向量空间与处理契约','routing')}${flowNode('范围内召回与融合','字段、全文与描述向量','search')}${flowNode('返回媒体与片段','fileId＋segmentId＋原片时间','search')}</div></section>
      <section class="flow-lane"><div class="lane-label"><span class="step-badge">C</span><div><h3>单媒体问答</h3><p>固定当前图片／视频／时间范围</p></div></div><div class="flow-row">${flowNode('问题与会话范围','新问题需要生成回答','qa')}${flowNode('读取有效证据','分析、片段、OCR、已确认标注','qa')}${flowNode('证据足够？','足够则直接依据记录回答','qa')}${flowNode('按需视觉复核','只检查当前媒体或候选片段','video')}${flowNode('回答与来源引用','不足时明确无法确定；可跳转证据','qa')}</div></section>
      <div class="contract-grid"><div><b>AnalysisRecord</b><span>源版本、生成部署、方案、覆盖信息、结构化内容</span></div><div><b>VectorRecord</b><span>分析／媒体引用、Embedding 契约、向量版本、可检索状态</span></div><div><b>QASession / Answer</b><span>范围、历史、回答／复核部署、证据引用、是否读取原媒体</span></div><div><b>正式媒体资料</b><span>人工标题、说明、标签独立保存；采用时再次做冲突校验</span></div></div></div>`,{kind:'blueprint',active:'flow',title:'业务流程',status:'开发蓝图 · 图中节点可跳转到对应设计页'});
  },
};
