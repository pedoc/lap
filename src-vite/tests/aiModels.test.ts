import { test } from 'node:test';
import assert from 'node:assert/strict';
import { effectiveParameter, applyAiConfiguration, supportsNonLatin, requiresIndexRebuild } from '../src/common/aiModels.ts';
test('adapter/model/user parameter priority preserves zero values', () => {
  const spec={key:'blur_threshold',default:200};
  assert.equal(effectiveParameter({defaults:{}},{parameters:{}},spec),200);
  assert.equal(effectiveParameter({defaults:{blur_threshold:80}},{parameters:{}},spec),80);
  assert.equal(effectiveParameter({defaults:{blur_threshold:80}},{parameters:{blur_threshold:0}},spec),0);
});
test('configuration synchronization uses the selected instance, not the first model', () => {
  const config:any={settings:{}};
  const view={libraryId:'library-a',bindings:{semantic:'custom',face:'face'},models:[{definition:{id:'english',languages:['en']}},{definition:{id:'multi',languages:['*']}}],instances:[{instance:{id:'default',modelId:'english'},values:{semantic_threshold:.26}},{instance:{id:'custom',modelId:'multi'},values:{semantic_threshold:.4}},{instance:{id:'face',modelId:'face'},values:{blur_threshold:0}}]};
  applyAiConfiguration(config,view);
  assert.equal(config.settings.ai.semanticParameters.semantic_threshold,.4);
  assert.equal(config.settings.ai.faceParameters.blur_threshold,0);
  assert.equal(config.settings.ai.libraryId,'library-a');
  assert.deepEqual(config.settings.ai.semanticLanguages,['*']);
});
test('language restrictions are declared rather than inferred from a model integer',()=>{
  assert.equal(supportsNonLatin(['en']),false);assert.equal(supportsNonLatin(['*']),true);assert.equal(supportsNonLatin([]),true);
});

test('rebuild warnings distinguish detection settings from execution/query settings',()=>{
 const definition={defaults:{detection_threshold:.65}};const saved={modelId:'m',parameters:{},endpoint:'',remoteModel:'',remoteRevision:''};
 const schema=[{key:'threads',default:2,affectsIndex:false},{key:'detection_threshold',default:.65,affectsIndex:true}];
 assert.equal(requiresIndexRebuild(definition,saved,{...saved,parameters:{threads:8}},schema),false);
 assert.equal(requiresIndexRebuild(definition,saved,{...saved,parameters:{detection_threshold:.8}},schema),true);
 assert.equal(requiresIndexRebuild(definition,saved,{...saved,remoteRevision:'new'},schema),true);
});
