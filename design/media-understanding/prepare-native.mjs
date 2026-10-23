// Build only temporary frontend output, then reuse current Vue templates and real app CSS.
import {spawn} from 'node:child_process';
import {readFile,writeFile,mkdtemp,readdir} from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';

const here=path.dirname(fileURLToPath(import.meta.url));
const repo=path.resolve(here,'../..');
function extractTemplate(source){
  const start=source.indexOf('<template>')+10;
  // Nested <template> blocks are valid in these SFCs; the final close precedes <script>.
  const end=source.lastIndexOf('</template>',source.indexOf('<script'));
  return source.slice(start,end).trim();
}
export async function prepareNative(){
  const output=await mkdtemp(path.join(os.tmpdir(),'lap-native-css-'));
  await new Promise((resolve,reject)=>{
    const child=spawn(process.execPath,[path.join(repo,'src-vite/node_modules/vite/bin/vite.js'),'build','--outDir',output],{cwd:path.join(repo,'src-vite'),windowsHide:true,stdio:['ignore','pipe','pipe']});
    let log='';child.stdout.on('data',chunk=>{log+=chunk;});child.stderr.on('data',chunk=>{log+=chunk;});
    child.on('error',reject);child.on('exit',code=>code===0?resolve():reject(new Error('Frontend CSS build failed:\n'+log.slice(-6000))));
  });
  const assets=path.join(output,'assets');
  const files=(await readdir(assets)).filter(name=>name.endsWith('.css'));
  const css=(await Promise.all(files.map(name=>readFile(path.join(assets,name),'utf8')))).join('\n');
  const names={settings:'views/Settings.vue',ai:'components/AiModelSettings.vue',cuda:'components/CudaRuntimeSettings.vue',title:'components/TitleBar.vue'};
  const source={css,templates:{},locale:JSON.parse(await readFile(path.join(repo,'src-vite/src/locales/zh-CN.json'),'utf8'))};
  for(const [key,name] of Object.entries(names))source.templates[key]=extractTemplate(await readFile(path.join(repo,'src-vite/src',name),'utf8'));
  source.hash=createHash('sha256').update(JSON.stringify(source)).digest('hex');
  await writeFile(path.join(here,'native-source.generated.js'),'// Generated from current frontend source; intentionally gitignored.\nwindow.LapNativeSource='+JSON.stringify(source)+';\n');
  console.log('Native settings CSS and templates prepared from current frontend.');
  return {hash:source.hash,templates:Object.values(names),cssFiles:files.length};
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))await prepareNative();
