// Keep the existing four AI tasks; the new configuration has one media-understanding tab.
const TASKS=[['semantic','图文语义检索'],['face','人脸识别'],['ocr','文字识别'],['detection','对象检测'],['media','媒体理解']];
function taskButtons(){
  return `<div class="flex flex-wrap gap-2" aria-label="AI 任务">${TASKS.map(([id,label])=>`<button class="btn btn-sm ${id===App.state.task?'btn-primary':'btn-ghost'}" data-task="${id}">${label}</button>`).join('')}</div>`;
}
function configCard(title,body){return `<section class="setting-card"><h3>${title}</h3>${body}</section>`;}
function compactRole(label,role,cap,help=''){
  const follow=role==='text'?`<option value="@media" ${App.state.followMediaForText?'selected':''} ${roleReady('image','text')?'':'disabled'}>与媒体理解相同</option>`:'';
  return `<div class="compact-role"><label for="route-${role}">${label}</label><select id="route-${role}" class="select" data-role="${role}">${follow}${UI.options(cap,role==='text'&&App.state.followMediaForText?'@media':App.state.roles[role])}</select>${help?`<p class="helper">${help}</p>`:''}</div>`;
}
