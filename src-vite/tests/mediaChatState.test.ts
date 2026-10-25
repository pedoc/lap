import test from 'node:test';
import assert from 'node:assert/strict';
import { closeMediaChat, mediaChat, openMediaChat, validMediaChatIds } from '../src/common/mediaChat.ts';
import { mediaAiBatch, openMediaAiBatch } from '../src/common/mediaAi.ts';

test('chat entry snapshots valid library media without sharing the caller selection or creating a remote session', () => {
  const ids = [4, 2, 4, 0, -1, NaN, 1.5, Infinity];
  const revision = mediaChat.revision;
  openMediaChat('library-a', ids);
  assert.equal(mediaChat.open, true);
  assert.equal(mediaChat.libraryId, 'library-a');
  assert.deepEqual([...mediaChat.fileIds], [4, 2]);
  assert.equal(mediaChat.revision, revision + 1);
  ids.push(9);
  assert.deepEqual([...mediaChat.fileIds], [4, 2]);
  closeMediaChat();
  assert.equal(mediaChat.open, false);
  assert.equal(mediaChat.libraryId, 'library-a');
  assert.deepEqual([...mediaChat.fileIds], [4, 2]);
});

test('empty entry context cannot change the active chat and large selections are not silently truncated', () => {
  openMediaChat('library-b', [6]);
  const before = JSON.stringify(mediaChat);
  openMediaChat('', [8]);
  assert.equal(JSON.stringify(mediaChat), before);
  const ids = Array.from({ length: 600 }, (_, index) => index + 1);
  openMediaChat('library-b', ids);
  assert.equal(mediaChat.fileIds.length, 600);
  assert.deepEqual(validMediaChatIds([Number.MAX_SAFE_INTEGER + 1, 7, 7]), [7]);
  closeMediaChat();
});

test('batch tasks carry a session model override without leaking it into later default tasks', () => {
  const ids = [1, 2];
  openMediaAiBatch('library-a', ids, 'session-model');
  assert.equal(mediaAiBatch.modelId, 'session-model');
  ids.push(3);
  assert.deepEqual([...mediaAiBatch.fileIds!], [1, 2]);
  openMediaAiBatch('library-a', null);
  assert.equal(mediaAiBatch.modelId, null);
  assert.equal(mediaAiBatch.fileIds, null);
  mediaAiBatch.open = false;
});
