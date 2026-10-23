const SCREENS = [
  {id:'baseline',title:'当前设置布局复核',file:'00-current-settings.png',group:'现状对照',caption:'按当前源码重建：独立 600×620 设置窗口、原左侧分类与单列 AI 表单；不是原生运行截图。'},
  {id:'models',title:'AI 模型任务与配置',file:'01-model-management.png',group:'模型与部署',caption:'在原独立设置窗口中增加用途按钮，保留模型选择、配置、测试和下方目录。'},
  {id:'local',title:'本机运行与下载',file:'07-local-runtime.png',group:'模型与部署',caption:'分别准备运行环境与模型包，本机可用性不等同于模型是否受支持。'},
  {id:'api',title:'API 部署配置',file:'08-api-connection.png',group:'模型与部署',caption:'本机、内网和外部 API 使用明确的连接配置，能力按部署分别验证。'},
  {id:'routing',title:'默认用途与路由',file:'09-task-routing.png',group:'模型与部署',caption:'同一部署可以承担多个用途，理解、生成、Embedding 各有明确默认绑定。'},
  {id:'catalog',title:'同页模型目录与添加',file:'14-model-catalog.png',group:'模型与部署',caption:'沿用 AI 模型页下方的目录、下载和导入区域；此图展示滚动到目录的状态。'},
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
const MODELS = [
  {id:'vision-lan',name:'视觉工作站',family:'Qwen-VL · 示例检查点',mode:'API',location:'指定内网',status:'ready',state:'示例已验证',caps:['text','image','video_frames','json'],note:'图片与帧序列视频理解，提供文本生成。'},
  {id:'embed-local',name:'本机检索模型',family:'多模态 Embedding · 示例检查点',mode:'本机',location:'本机',status:'ready',state:'示例已验证',caps:['embed_text','embed_image','embed_video'],note:'文本、图片与视频向量；同一契约下使用。'},
  {id:'mage-local',name:'Mage · 本机',family:'Mage-VL · 示例检查点',mode:'本机',location:'本机',status:'runtime_missing',state:'运行环境待准备',caps:['text','image','video_frames'],note:'模型定义保留，需准备并验证对应运行环境。'},
  {id:'cloud-text',name:'线上语言服务',family:'服务端模型 ID · 待配置',mode:'API',location:'外网',status:'untested',state:'未验证／当前策略未启用',caps:['text','json'],note:'仅作为连接配置示例，不自动接收图库内容。'},
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
    roles:{text:'vision-lan',image:'vision-lan',video:'vision-lan',textEmbed:'embed-local',visualEmbed:'embed-local'},
    filter:'all',task:'media',localModel:'mage',localEngine:'python',localReady:false,apiTested:false,scope:'selected',
    fields:{title:true,description:true,tags:true},indexEnabled:true,existingTagsOnly:true,
    title:'',description:'',tags:['旅行'],qaMessages:[],qaInspected:false,qaPending:false,
    searchQuery:'找出 2025 年四星以上有湖边木屋的照片和视频',searchRan:true,
    createdJobs:0,jobStopped:false,jobScope:'selected',qaMode:'indexed',notice:'',
  },
};
function scopeData(){return SCOPES[App.state.scope]||SCOPES.selected;}
function modelName(role){return UI.model(App.state.roles[role]).name;}
function n(value){return value.toLocaleString('en-US');}
function headerActions(){return UI.button('默认用途','routing','quiet','settings')+UI.button('添加模型','add-model','primary','plus');}
