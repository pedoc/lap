// Current Settings/AiModelSettings templates are rendered in an isolated, themed Vue fixture.
function nativeButton(label,action,primary=false){return `<button class="btn btn-sm ${primary?'btn-primary':''}" data-action="${action}">${label}</button>`;}
function nativeTask(task,baseline=false){
  App.currentNativeTask={task,baseline};
  const names={semantic:'图文语义检索',media:'媒体理解',text:'语言生成',embedding:'Embedding'};
  return UI.settingsWindow('',{baseline,title:baseline?'当前设置界面复核':'在原 AI 模型页扩展'+(names[task]||''),notes:[['沿用当前组件','使用当前 Settings、TitleBar、AiModelSettings 和 CUDA 组件模板，以及当前前端构建出的 CSS；只用样例状态替换后端。'],['保留现有入口','默认仍打开图文语义检索与 CLIP 配置，原四个任务按钮和单列滚动表单保持原位。新增用途追加到任务栏。'],['共享模型服务','地址、协议、密钥由模型服务管理；任务模型选择仍放在原来的选择器中。管理入口位于任务栏右侧。']]});
}
const NativeSettingsViews={
  baseline(){return nativeTask('semantic',true);},
  models(){return nativeTask(App.state.task||'semantic');},
  understand(){return nativeTask(App.state.task==='semantic'?'media':App.state.task);},
  services(){return ServiceViews.models();},
};
