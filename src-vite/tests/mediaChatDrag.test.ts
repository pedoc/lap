import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import * as Vue from 'vue';
import { createMediaChatPointerDrag, isMediaChatDragSurface } from '../src/common/mediaChatDrag.ts';

function fixture() {
  const listeners = new Map<string, Set<any>>(), windowListeners = new Map<string, Set<any>>(), feedback = new Set<string>(), calls: any[] = [];
  let source: any = { libraryId: 'library-a', fileId: 7, path: '/fixture/photo.jpg', version: '1' }, overChat = false;
  const target: any = { classList: { add: (...values: string[]) => values.forEach(value => feedback.add(value)), remove: (...values: string[]) => values.forEach(value => feedback.delete(value)) }, dispatchEvent: (event: any) => calls.push(['drop', event.detail]) };
  const events = (map: Map<string, Set<any>>) => ({ addEventListener: (name: string, callback: any) => { if (!map.has(name)) map.set(name, new Set()); map.get(name)!.add(callback); }, removeEventListener: (name: string, callback: any) => map.get(name)?.delete(callback) });
  const dom: any = { ...events(listeners), documentElement: events(listeners), elementFromPoint: () => overChat ? { closest: () => target } : null };
  const controller = createMediaChatPointerDrag({ source: () => source, snapshot: () => ({ scale: 2, normX: 0.4, normY: 0.3 }), cancelInteraction: () => calls.push(['cancel-pan']), restore: snapshot => calls.push(['restore', snapshot]) }, { document: dom, window: events(windowListeners) as any });
  const pointer = (extra: any = {}) => ({ type: 'pointermove', pointerId: 3, button: 0, isPrimary: true, pointerType: 'mouse', buttons: 1, clientX: 100, clientY: 100, preventDefault: () => assert.fail('Normal media pointer events must remain available'), stopPropagation: () => assert.fail('Original Image cleanup must still receive pointerup'), ...extra } as any);
  const dispatch = (name: string, event = pointer({ type: name })) => { for (const callback of [...listeners.get(name) || [], ...windowListeners.get(name) || []]) callback(event); };
  return { controller, calls, feedback, pointer, dispatch, listeners, windowListeners,
    over: (value = true) => { overChat = value; }, source: () => source, setSource: (value: any) => { source = value; },
    listenerCount: () => [...listeners.values(), ...windowListeners.values()].reduce((count, callbacks) => count + callbacks.size, 0),
  };
}

test('media drags observe normal pan gestures without restoring or attaching outside the chat target', () => {
  const h = fixture();
  h.controller.start(h.pointer()); h.dispatch('pointermove', h.pointer({ clientX: 150 })); h.dispatch('pointerup', h.pointer({ type: 'pointerup', clientX: 150 }));
  assert.deepEqual(h.calls, []); assert.equal(h.listenerCount(), 0);
  for (const extra of [{ button: 2 }, { pointerType: 'touch' }, { isPrimary: false }]) { h.controller.start(h.pointer(extra)); assert.equal(h.listenerCount(), 0); }
});

test('only a completed chat drop clears pan, restores its original viewport and dispatches the real file reference', () => {
  const h = fixture(); h.controller.start(h.pointer()); h.over();
  h.dispatch('pointermove', h.pointer({ clientX: 103 })); assert.equal(h.feedback.size, 0);
  h.dispatch('pointermove', h.pointer({ clientX: 130 })); assert.ok(h.feedback.has('ring-primary'));
  h.dispatch('pointerup', h.pointer({ type: 'pointerup', clientX: 130 }));
  assert.deepEqual(h.calls, [['cancel-pan'], ['restore', { scale: 2, normX: 0.4, normY: 0.3 }], ['drop', { libraryId: 'library-a', fileIds: [7] }]]);
  assert.equal(h.feedback.size, 0); assert.equal(h.listenerCount(), 0);
});

test('library, file, path or version changes cannot add the previous media or restore a viewport into its replacement', () => {
  for (const change of [{ libraryId: 'library-b' }, { fileId: 8 }, { path: '/different.jpg' }, { version: '2' }, null]) {
    const h = fixture(); h.controller.start(h.pointer()); h.over(); h.dispatch('pointermove', h.pointer({ clientX: 130 }));
    h.setSource(change === null ? null : { ...h.source(), ...change });
    h.dispatch('pointerup', h.pointer({ type: 'pointerup', clientX: 130 }));
    assert.deepEqual(h.calls, []); assert.equal(h.feedback.size, 0); assert.equal(h.listenerCount(), 0);
  }
});

test('wrong pointers, cancellation, blur, Escape and disposal never attach and release hover feedback and listeners', () => {
  for (const action of ['pointercancel', 'blur', 'keydown', 'dispose']) {
    const h = fixture(); h.controller.start(h.pointer()); h.over(); h.dispatch('pointermove', h.pointer({ clientX: 130 }));
    h.dispatch('pointerup', h.pointer({ type: 'pointerup', pointerId: 4, clientX: 130 })); assert.ok(h.listenerCount() > 0);
    if (action === 'dispose') h.controller.dispose();
    else h.dispatch(action, h.pointer({ type: action, key: 'Escape', clientX: 130 }));
    assert.equal(h.calls.some(call => call[0] === 'drop' || call[0] === 'restore'), false);
    assert.equal(h.feedback.size, 0); assert.equal(h.listenerCount(), 0);
  }
});

test('real media root div and canvas hit areas can start while controls, annotation SVG and editors remain excluded', () => {
  const source = {};
  const surface = (blocked: boolean) => ({ closest: (selector: string) => selector === '[data-media-chat-source]' ? source : blocked ? {} : null }) as any;
  assert.equal(isMediaChatDragSurface(surface(false)), true);
  assert.equal(isMediaChatDragSurface(surface(true)), false);
  assert.equal(isMediaChatDragSurface({ closest: () => null } as any), false);
  const image = readFileSync(new URL('../src/components/Image.vue', import.meta.url), 'utf8');
  const viewer = readFileSync(new URL('../src/components/MediaViewer.vue', import.meta.url), 'utf8');
  assert.match(image, /pointerEvents: 'auto'/);
  assert.equal((viewer.match(/data-media-chat-source/g) || []).length, 3);
  assert.match(viewer, /@pointerdown\.capture="beginChatMediaDrag"/);
});

test('video mouse and pen chat drags cannot start touch zoom or leave a mouseup pan flush', () => {
  const video = readFileSync(new URL('../src/components/Video.vue', import.meta.url), 'utf8');
  assert.doesNotMatch(video, /(?:addEventListener\s*\(\s*['"]mouse(?:down|move|up)|@mouse(?:down|move|up))/);
  const pointers = new Map([[9, { x: 1, y: 2 }]]);
  const context = { activeTouchPointers: pointers, pinchStartDistance: 20, pinchStartScale: 3, isPinching: true,
    scale: Vue.ref(2), isFit: Vue.ref(true), noTransition: Vue.ref(false),
    updateTransform: () => assert.fail('A mouse/pen chat drag cannot transform the video'),
    requestAnimationFrame: () => assert.fail('A mouse/pen chat drag cannot schedule a later viewport update'),
  };
  for (const name of ['handlePinchPointerDown', 'handlePinchPointerMove', 'handlePinchPointerEnd']) {
    const source = video.match(new RegExp(`function ${name}\\([\\s\\S]*?\\r?\\n}`))![0];
    const handler: any = runInNewContext(`${stripTypeScriptTypes(source)};${name}`, context);
    for (const pointerType of ['mouse', 'pen']) handler({ pointerType, pointerId: 9, clientX: 300, clientY: 200,
      preventDefault: () => assert.fail('Video handlers must leave mouse/pen pointer events available for the chat drop'),
    });
  }
  assert.deepEqual([...pointers], [[9, { x: 1, y: 2 }]]);
  assert.equal(context.scale.value, 2); assert.equal(context.isFit.value, true); assert.equal(context.noTransition.value, false);
});

test('gallery and filmstrip pointers remain available for chat while other input handlers still block the grid', () => {
  const grid = readFileSync(new URL('../src/components/GridView.vue', import.meta.url), 'utf8');
  const condition = grid.match(/'pointer-events-none':\s*([^\r\n]+),/)![1];
  for (const [inputStack, blocked] of [[[], false], [['MediaChat'], false], [['MessageBox'], true], [['MediaChat', 'FaceNameEditor:7'], true], [['ObjectEdit', 'MediaChat'], true]] as const) {
    assert.equal(runInNewContext(condition, { uiStore: { inputStack } }), blocked);
  }
  assert.match(grid, /@dragstart\.capture\.prevent/);
});

test('the viewer source guard binds the loaded active media, library and UI version while excluding editing and inactive panes', () => {
  const viewer = readFileSync(new URL('../src/components/MediaViewer.vue', import.meta.url), 'utf8');
  const code = viewer.match(/function chatDragSource\([\s\S]*?\r?\n}/)![0];
  let version = 1, editing = false;
  const ui = { inputStack: ['MediaChat'], getFileVersion: () => version };
  const context: any = { props: { file: { id: 7, file_path: '/fixture.jpg', modified_at: 1, size: 50, rotate: 0 }, canInteract: true, toolbarOnly: false, isSlideShow: false }, libConfig: { _libraryId: 'library-a' }, mediaChat: { open: true, libraryId: 'library-a' }, useUIStore: () => ui, mediaRef: { value: { viewReady: true } }, objectEditActive: () => editing };
  const read: any = runInNewContext(`${stripTypeScriptTypes(code)};chatDragSource`, context);
  const initial = read(); assert.equal(initial.fileId, 7); assert.equal(initial.libraryId, 'library-a');
  version++; assert.notEqual(read().version, initial.version);
  context.mediaRef.value.viewReady = false; assert.equal(read(), null); context.mediaRef.value.viewReady = true;
  context.props.canInteract = false; assert.equal(read(), null); context.props.canInteract = true;
  ui.inputStack.push('FaceNameEditor:7'); assert.equal(read(), null); ui.inputStack.pop();
  editing = true; assert.equal(read(), null); editing = false;
  context.mediaChat.open = false; assert.equal(read(), null); context.mediaChat.open = true;
  context.libConfig._libraryId = 'library-b'; assert.equal(read(), null);
});

test('a chat target at the window edge takes precedence over native OS image and gallery file dragging', () => {
  const image = readFileSync(new URL('../src/components/Image.vue', import.meta.url), 'utf8');
  const functionSource = image.match(/function trackImageDragOut\([\s\S]*?\r?\n}/)![0];
  const calls: string[] = []; let overChat = true;
  const track: any = runInNewContext(`${stripTypeScriptTypes(functionSource)};trackImageDragOut`, {
    dragOutStart: { x: 10, y: 10, path: '/fixture.jpg', preview: [] }, mediaChatDropTarget: () => overChat ? {} : null, isWindowDragEdge: () => true,
    mouseDragNavDeltaX: Vue.ref(0), mouseDragNavDeltaY: Vue.ref(0), finishImageMouseDrag: () => calls.push('finish-pan'), startNativeFileDrag: async () => { calls.push('native'); },
  });
  track({ buttons: 1, clientX: 999, clientY: 10 }); assert.deepEqual(calls, []);
  overChat = false; track({ buttons: 1, clientX: 999, clientY: 10 }); assert.deepEqual(calls, ['finish-pan', 'native']);
  const content = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
  const updateSource = content.match(/function updateContentDragPosition\([\s\S]*?\r?\n}/)![0];
  let handoffs = 0; overChat = true;
  const update: any = runInNewContext(`${stripTypeScriptTypes(updateSource)};updateContentDragPosition`, { document: { elementFromPoint: () => ({ closest: () => overChat ? {} : null }) }, isContentInternalDrag: Vue.ref(true), isWindowDragEdge: () => true, handOffNativeDrag: () => { handoffs++; }, dragGhost: null });
  update({ buttons: 1, clientX: 999, clientY: 10 }); assert.equal(handoffs, 0);
  overChat = false; update({ buttons: 1, clientX: 999, clientY: 10 }); assert.equal(handoffs, 1);
});
