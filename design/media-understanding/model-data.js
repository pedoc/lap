const DEPLOYMENT=window.LapNativeSource.deploymentProfiles.profiles;
const PROVIDERS=Object.entries(DEPLOYMENT).map(([key,p],index)=>({id:'service-'+(index+1),name:'模型服务 '+(index+1),
  baseUrl:`http://127.0.0.1:${p.port}/v1`,enabled:true,deploymentProfile:key}));
const MODELS = [
  ...[['vision-lan','mage','Mage',['text','image','video_frames','json']],['qwen-api','qwen','Qwen',['text','image','video_frames','json']],['embed-text-api','embedding','Qwen',['embed_text']]].map(([id,key,group,caps],index)=>{
    const p=DEPLOYMENT[key];return {id,providerId:'service-'+(index+1),group,remoteId:p.servedModel,name:p.model.split('/').pop(),
      mode:'API',location:'API',status:'ready',state:'示例已验证',caps,api:p.task==='embedding'?'embeddings':'chat',modelRevision:p.modelRevision,
      ...(p.task==='embedding'?{embedding:{format:'openai-text',dimensions:p.dimensions,documentEncoding:p.documentEncoding,queryInstruction:p.queryInstruction}}:{})};
  }),
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
    roles:{text:'vision-lan',image:'vision-lan',video:'vision-lan',textEmbed:'embed-text-api'},followMediaForText:true,indexBuilt:true,
    filter:'all',task:'semantic',providerId:'service-1',providerSearch:'',providerOverrides:{},dirtyProviders:[],verifiedModels:{},editingModelId:'vision-lan',apiTested:false,scope:'selected',
    fields:{title:true,description:true,tags:true},indexEnabled:true,existingTagsOnly:true,
    title:'',description:'',tags:['旅行'],qaMessages:[],qaInspected:false,qaPending:false,
    searchQuery:'找出 2025 年四星以上有湖边木屋的照片和视频',searchRan:true,
    createdJobs:0,jobStopped:false,jobScope:'selected',qaMode:'indexed',notice:'',
  },
};
function scopeData(){return SCOPES[App.state.scope]||SCOPES.selected;}
function roleModel(role){return MODELS.find(m=>m.id===App.state.roles[role]);}
function roleReady(role,cap){const m=roleModel(role);return !!m&&modelReady(m)&&(!cap||m.caps.includes(cap));}
function modelName(role){const m=roleModel(role);return m?modelLabel(m):'未配置';}
function n(value){return value.toLocaleString('en-US');}
function headerActions(){return UI.button('默认用途','routing','quiet','settings')+UI.button('添加模型','add-model','primary','plus');}
