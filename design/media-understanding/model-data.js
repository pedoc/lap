const SCREENS = [
  {id:'baseline',title:'当前设置布局复核',file:'00-current-settings.png',group:'现状对照',caption:'复用当前 Vue 模板与前端样式，在 1437×833 浅色窗口复核截图中的布局；数据为样例。'},
  {id:'models',title:'AI 模型原位扩展',file:'01-model-management.png',group:'模型与部署',caption:'默认仍为 CLIP，原四类任务保持原位；新增用途与模型服务管理入口无感融入。'},
  {id:'understand',title:'媒体理解模型配置',file:'17-media-model.png',group:'模型与部署',caption:'沿用同一个模型选择器和参数表单，媒体理解追加到原任务栏，连接继承模型服务。'},
  {id:'services',title:'模型服务管理',file:'18-model-services.png',group:'模型与部署',caption:'从原任务栏右侧进入共享服务管理，不替换 AI 模型默认入口。'},
  {id:'local',title:'本机运行与下载',file:'07-local-runtime.png',group:'模型与部署',caption:'分别准备运行环境与模型包，本机可用性不等同于模型是否受支持。'},
  {id:'api',title:'服务连接与协议',file:'08-api-connection.png',group:'模型与部署',caption:'服务统一管理协议、地址与密钥；Chat Completions、Responses、Messages 各用相应适配器。'},
  {id:'routing',title:'默认模型',file:'09-task-routing.png',group:'模型与部署',caption:'媒体理解、语言生成、Embedding 只引用服务中的模型，不重复连接配置。'},
  {id:'catalog',title:'服务下的模型列表',file:'14-model-catalog.png',group:'模型与部署',caption:'同步模型或手填 ID，能力图标、可用状态与模型参数分别管理。'},
  {id:'model',title:'模型能力与接口',file:'16-model-properties.png',group:'模型与部署',caption:'模型设置只管理 ID、能力和覆盖参数；连接信息继承服务，不再按业务重复填写。'},
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
  {id:'autodl',name:'AutoDL · Mage',protocol:'chat',baseUrl:'http://127.0.0.1:8000/v1',location:'远程 GPU · SSH 隧道',enabled:true,mark:'M'},
  {id:'qwen-server',name:'AutoDL · Qwen',protocol:'chat',baseUrl:'http://127.0.0.1:8001/v1',location:'远程 GPU · SSH 隧道',enabled:true,mark:'Q'},
  {id:'embedding',name:'私有向量服务',protocol:'embeddings',baseUrl:'http://127.0.0.1:8002/v1',location:'远程 GPU · SSH 隧道',enabled:true,mark:'E'},
  {id:'cloud',name:'自定义线上服务',protocol:'responses',baseUrl:'https://api.example.com/v1',location:'外部服务',enabled:false,mark:'S'},
  {id:'claude',name:'Messages 兼容服务',protocol:'messages',baseUrl:'https://messages.example.com/v1',location:'外部服务',enabled:false,mark:'A'},
];
const PROTOCOLS={chat:{name:'OpenAI Chat Completions',path:'chat/completions'},responses:{name:'OpenAI Responses',path:'responses'},messages:{name:'Anthropic Messages',path:'messages'},embeddings:{name:'OpenAI Embeddings（文本）',path:'embeddings'}};
const MODELS = [
  {id:'vision-lan',providerId:'autodl',remoteId:'mage-vl',name:'Mage-VL',family:'microsoft/Mage-VL · SGLang 专用分支',mode:'API',location:'远程 GPU',status:'ready',state:'示例已验证',caps:['text','image','video_frames','json'],note:'通用 API 路径使用图片与带时间说明的抽样帧。'},
  {id:'qwen-api',providerId:'qwen-server',remoteId:'qwen3-vl-4b-instruct',name:'Qwen3-VL-4B',family:'Qwen3-VL-4B-Instruct · vLLM',mode:'API',location:'远程 GPU',status:'ready',state:'示例已验证',caps:['text','image','video_frames','json'],note:'与 Mage 使用同类 API；服务可以独立启停。'},
  {id:'embed-text-api',providerId:'embedding',remoteId:'text-embedding-model',name:'描述向量模型',family:'标准文本 Embeddings · 示例服务',mode:'API',location:'远程 GPU',status:'ready',state:'示例已验证',caps:['embed_text'],note:'独立向量服务示例，不把 Mage 或 Qwen 生成模型作为向量模型。'},
  {id:'embed-local',name:'本机检索模型',family:'多模态 Embedding · 示例检查点',mode:'本机',location:'本机',status:'ready',state:'示例已验证',caps:['embed_text','embed_image','embed_video'],note:'文本、图片与视频向量；同一契约下使用。'},
  {id:'mage-local',name:'Mage · 本机',family:'Mage-VL · 示例检查点',mode:'本机',location:'本机',status:'runtime_missing',state:'运行环境待准备',caps:['text','image','video_frames'],note:'模型定义保留，需准备并验证对应运行环境。'},
  {id:'cloud-text',providerId:'cloud',remoteId:'model-id-example',name:'线上语言模型',family:'服务端模型 ID · 待配置',mode:'API',location:'外部服务',status:'untested',state:'未验证／服务未启用',caps:['text','image','json'],note:'服务字段共用，模型条目只补充 ID 与能力。'},
  {id:'claude-example',providerId:'claude',remoteId:'model-id-example',name:'Messages 模型',family:'Messages 兼容模型 · 待配置',mode:'API',location:'外部服务',status:'untested',state:'未验证／服务未启用',caps:['text','image'],note:'按 Messages 协议发送图文，不靠替换 URL 模拟其他协议。'},
  {id:'clip-local',name:'现有 CLIP',family:'CLIP 图文检索',mode:'本机',location:'本机',status:'ready',state:'既有索引',caps:['clip_query','embed_image'],note:'可继续检索既有索引，与新索引分别管理版本。'},
  {id:'ocr-local',name:'现有文字识别',family:'OCR 识别模型',mode:'本机',location:'本机',status:'ready',state:'既有能力',caps:['ocr'],note:'保留专用识别契约及人工修订。'},
];
const CAP_LABELS={text:'文字生成',image:'图片理解',video_frames:'视频（帧序列）',json:'结构化输出',embed_text:'文本向量',embed_image:'图片向量',embed_video:'视频向量',ocr:'文字识别',clip_query:'图文查询向量'};
const SCOPES={
  selected:{name:'选中的媒体',count:36,images:32,videos:4,analysisReuse:12,vectorReuse:8,titles:28,descriptions:30,tags:21},
  filtered:{name:'当前筛选结果',count:214,images:190,videos:24,analysisReuse:86,vectorReuse:61,titles:42,descriptions:150,tags:124},
  library:{name:'当前图库全部可见媒体',count:1286,images:1192,videos:94,analysisReuse:644,vectorReuse:448,titles:86,descriptions:912,tags:760},
};
const App = {
  screen:'models',params:new URLSearchParams(location.search),
  state:{
    roles:{text:'vision-lan',image:'vision-lan',video:'vision-lan',textEmbed:'embed-text-api',visualEmbed:'embed-local'},
    filter:'all',task:'semantic',providerId:'autodl',providerOverrides:{},dirtyProviders:[],verifiedModels:{},editingModelId:'vision-lan',localModel:'mage',localEngine:'python',localReady:false,apiTested:false,scope:'selected',
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
