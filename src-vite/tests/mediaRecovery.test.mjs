import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const read=path=>readFileSync(new URL(path,import.meta.url),'utf8');
test('backup and restore disclose that original and recovery media are not in a database archive',()=>{
 for(const component of ['BackupDialog.vue','RestoreDialog.vue'])assert.match(read('../src/components/'+component),/media_recovery.backup_scope/);
 const en=JSON.parse(read('../src/locales/en.json'));const zh=JSON.parse(read('../src/locales/zh-CN.json'));
 assert.match(en.media_recovery.backup_scope,/not original/);assert.match(zh.media_recovery.backup_scope,/不包含原图/);
});
test('backup and restore surface preflight warnings rather than silently reporting full media restoration',()=>{
 for(const component of ['BackupDialog.vue','RestoreDialog.vue'])assert.match(read('../src/components/'+component),/result.warnings/);
 assert.match(read('../src/components/MediaUtilityView.vue'),/result.recovery\?\.issues/);
});
