// Current Settings/AiModelSettings templates are rendered in an isolated, themed Vue fixture.
function nativeButton(label,action,primary=false){return `<button class="btn btn-sm ${primary?'btn-primary':''}" data-action="${action}">${label}</button>`;}
function nativeTask(task,baseline=false){
  App.currentNativeTask={task,baseline};
  return UI.settingsWindow('',{baseline,title:baseline?'当前设置界面复核':'在原 AI 模型页追加媒体理解',notes:[['沿用当前组件','使用当前 Settings、TitleBar、AiModelSettings 和 CUDA 组件模板及当前前端 CSS，样例状态替换后端。'],['只加一个标签页','默认仍为图文语义检索与 CLIP，原四个任务保持原位；只追加媒体理解，不新增语言生成与 Embedding 标签页。'],['新能力统一配置','媒体理解页上方管理 API 服务及模型，下方分别选择三种用途的默认模型；首版不管理新的本机生成引擎。']]});
}
const NativeSettingsViews={
  baseline(){return nativeTask('semantic',true);},
  models(){return App.state.task==='media'?mediaSettings():nativeTask(App.state.task||'semantic');},
  understand(){return mediaSettings();},
};
