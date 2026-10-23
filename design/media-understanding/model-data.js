const SCREENS = [
  {id:'baseline',title:'当前设置布局复核',file:'00-current-settings.png',group:'现状对照',caption:'复用当前 Vue 模板与前端样式，在 1437×833 浅色窗口复核截图中的布局；数据为样例。'},
  {id:'models',title:'AI 模型原位扩展',file:'01-model-management.png',group:'模型与服务',caption:'默认仍为 CLIP，原四项任务保留，只追加一个媒体理解标签页。'},
  {id:'understand',title:'媒体理解统一配置',file:'17-media-model.png',group:'模型与服务',caption:'同一页管理多个 OpenAI 兼容服务及其模型，下方分别选择媒体理解、语言生成和 Embedding 默认模型。'},
  {id:'api',title:'服务连接配置',file:'08-api-connection.png',group:'模型与服务',caption:'在媒体理解页展开服务连接，只填写名称、基础地址和密钥；不绑定部署平台。'},
  {id:'model',title:'模型能力标记',file:'16-model-properties.png',group:'模型与服务',caption:'模型归属服务；标记图片、视频、文本生成和向量能力，用于筛选默认模型。'},
  {id:'understand',title:'媒体理解最小窗口适配',file:'19-media-compact.png',group:'窗口适配',query:'size=compact',caption:'同一组件与 CSS 在 600×620 最小窗口自然换行和滚动，不另建紧凑版页面。'},
  {id:'workspace',title:'内容索引工作区',file:'05-library-workspace.png',group:'分析与检索',caption:'分析完成不等于向量就绪；未建立内容索引的媒体不被当作内容不存在。'},
  {id:'selection',title:'现有多选面板中的入口',file:'15-selection-entry.png',group:'分析与检索',caption:'沿用 SelectionPanel 的分组操作，在整理区增加媒体理解，打开共享的批量处理对话框。'},
  {id:'batch',title:'选中媒体建立索引',file:'04-batch-fill.png',group:'分析与检索',caption:'固定范围、分析方案与部署，建立索引并按需填充标题、说明和标签。'},
  {id:'batch',title:'当前图库建立索引',file:'06-library-batch.png',group:'分析与检索',query:'scope=library',caption:'全库指当前图库全部可见媒体，范围、复用量与写入规则在提交前明确。'},
  {id:'viewer',title:'图片分析与资料采用',file:'02-image-viewer.png',group:'查看与问答',caption:'分析记录、正式资料与标签建议分开保存，采用操作才更新用户资料。'},
  {id:'qa',title:'图片问答闭环',file:'10-image-qa.png',group:'查看与问答',caption:'先取当前媒体证据，缺少细节时只复核当前图片，回答不自动写入资料。'},
  {id:'video',title:'视频片段问答',file:'03-video-viewer.png',group:'查看与问答',caption:'从片段索引召回证据，回答引用原片时间；跳转片段不冒充逐帧事件检测。'},
  {id:'search',title:'自然语言搜索',file:'11-natural-search.png',group:'分析与检索',caption:'条件解析、兼容向量编码、范围内召回与融合检索，不逐项重新运行媒体分析。'},
  {id:'task',title:'任务进度与部分失败',file:'12-task-progress.png',group:'分析与检索',caption:'分析、向量和资料写入分别计数；失败阶段重试，复用已经完成的有效产物。'},
  {id:'flow',title:'开发流程与数据边界',file:'13-flow-guide.png',group:'开发蓝图',caption:'离线索引、自然语言搜索和单媒体问答三条链路，共享部署与来源校验。'},
];
const PROVIDERS = [
  {id:'vision-server',name:'自建模型服务',baseUrl:'https://models.example.com/v1',enabled:true},
  {id:'workspace-server',name:'团队模型服务',baseUrl:'https://ai.example.net/v1',enabled:true},
];
const MODELS = [
  {id:'vision-lan',providerId:'vision-server',remoteId:'mage-vl',name:'Mage-VL',mode:'API',status:'ready',state:'示例已验证',caps:['text','image','video_frames','json']},
  {id:'qwen-api',providerId:'vision-server',remoteId:'qwen3-vl-4b-instruct',name:'Qwen3-VL-4B',mode:'API',status:'ready',state:'示例已验证',caps:['text','image','video_frames','json']},
  {id:'cloud-text',providerId:'workspace-server',remoteId:'text-model',name:'语言模型',mode:'API',status:'ready',state:'示例已验证',caps:['text','json']},
  {id:'embed-text-api',providerId:'workspace-server',remoteId:'qwen3-embedding-0.6b',name:'Qwen3-Embedding',mode:'API',status:'ready',state:'示例已验证',caps:['embed_text']},
  {id:'embed-multimodal-api',providerId:'workspace-server',remoteId:'qwen3-vl-embedding',name:'Qwen3-VL-Embedding',mode:'API',status:'untested',state:'多模态接口待验证',caps:['embed_text','embed_multimodal']},
  {id:'clip-local',name:'现有 CLIP',family:'CLIP 图文检索',mode:'本机',location:'本机',status:'ready',state:'既有索引',caps:['clip_query','embed_image'],note:'可继续检索既有索引，与新索引分别管理版本。'},
  {id:'ocr-local',name:'现有文字识别',family:'OCR 识别模型',mode:'本机',location:'本机',status:'ready',state:'既有能力',caps:['ocr'],note:'保留专用识别契约及人工修订。'},
];
const CAP_LABELS={text:'文本生成',image:'图片理解',video_frames:'视频理解',json:'结构化输出',embed_text:'文本 Embedding',embed_multimodal:'多模态 Embedding',embed_image:'图片向量',ocr:'文字识别',clip_query:'图文查询向量'};
const SCOPES={
  selected:{name:'选中的媒体',count:36,images:32,videos:4,analysisReuse:12,vectorReuse:8,titles:28,descriptions:30,tags:21},
  filtered:{name:'当前筛选结果',count:214,images:190,videos:24,analysisReuse:86,vectorReuse:61,titles:42,descriptions:150,tags:124},
  library:{name:'当前图库全部可见媒体',count:1286,images:1192,videos:94,analysisReuse:644,vectorReuse:448,titles:86,descriptions:912,tags:760},
};
const App = {
  screen:'models',params:new URLSearchParams(location.search),
  state:{
    roles:{text:'vision-lan',image:'vision-lan',video:'vision-lan',textEmbed:'embed-text-api'},
    filter:'all',task:'semantic',providerId:'vision-server',providerOverrides:{},dirtyProviders:[],verifiedModels:{},editingModelId:'vision-lan',apiTested:false,scope:'selected',
    fields:{title:true,description:true,tags:true},indexEnabled:true,existingTagsOnly:true,
    title:'',description:'',tags:['旅行'],qaMessages:[],qaInspected:false,qaPending:false,
    searchQuery:'找出 2025 年四星以上有湖边木屋的照片和视频',searchRan:true,
    createdJobs:0,jobStopped:false,jobScope:'selected',qaMode:'indexed',notice:'',
  },
};
function scopeData(){return SCOPES[App.state.scope]||SCOPES.selected;}
function modelName(role){return modelLabel(UI.model(App.state.roles[role]));}
function n(value){return value.toLocaleString('en-US');}
function headerActions(){return UI.button('默认用途','routing','quiet','settings')+UI.button('添加模型','add-model','primary','plus');}
