<template>
  <div ref="root" class="media-chat-markdown min-w-0 max-w-full leading-relaxed select-text" data-chat-markdown @click="followLink" @auxclick="followLink" v-html="html"></div>
</template>

<script setup lang="ts">
import { ref, watch, onBeforeUnmount, nextTick } from 'vue';
import { openExternalUrl } from '@/common/api';
import { renderChatMarkdown, safeChatMarkdownLink } from '@/common/chatMarkdown';

const props = withDefaults(defineProps<{ content: string; streaming?: boolean }>(), { streaming: false });
const emit = defineEmits<{ 'link-error': [message: string]; rendered: [] }>();
const root = ref<HTMLDivElement | null>(null), html = ref('');
let timer: ReturnType<typeof setTimeout> | undefined, disposed = false;
function flush() {
  clearTimeout(timer); timer = undefined;
  if (!disposed) {
    html.value = renderChatMarkdown(props.content);
    void nextTick(() => { if (!disposed) emit('rendered'); });
  }
}
watch([() => props.content, () => props.streaming], (_next, previous) => {
  if (!props.content || !props.streaming || !previous?.[1]) flush();
  else if (!timer) timer = setTimeout(flush, 100);
}, { immediate: true });
async function followLink(event: MouseEvent) {
  const target = event.target instanceof Element ? event.target : (event.target as Node | null)?.parentElement;
  const anchor = target?.closest('a[href]');
  if (!anchor || !root.value?.contains(anchor)) return;
  event.preventDefault();
  if (event.type === 'auxclick' ? event.button !== 1 : event.button !== 0) return;
  const url = safeChatMarkdownLink(anchor.getAttribute('href') || '');
  if (!url) return;
  try { await openExternalUrl(url); }
  catch (reason) { if (!disposed) emit('link-error', String(reason)); }
}
onBeforeUnmount(() => { disposed = true; clearTimeout(timer); timer = undefined; });
</script>

<style scoped>
.media-chat-markdown { overflow-wrap: anywhere; }
.media-chat-markdown :deep(> :first-child) { margin-top: 0; }
.media-chat-markdown :deep(> :last-child) { margin-bottom: 0; }
.media-chat-markdown :deep(p),
.media-chat-markdown :deep(ul),
.media-chat-markdown :deep(ol),
.media-chat-markdown :deep(blockquote),
.media-chat-markdown :deep(pre),
.media-chat-markdown :deep(.chat-markdown-table) { margin: .65em 0; }
.media-chat-markdown :deep(strong) { font-weight: 700; }
.media-chat-markdown :deep(em) { font-style: italic; }
.media-chat-markdown :deep(s) { text-decoration: line-through; }
.media-chat-markdown :deep(h1),
.media-chat-markdown :deep(h2),
.media-chat-markdown :deep(h3),
.media-chat-markdown :deep(h4),
.media-chat-markdown :deep(h5),
.media-chat-markdown :deep(h6) { margin: 1em 0 .45em; font-weight: 650; line-height: 1.45; }
.media-chat-markdown :deep(h1) { font-size: 1.4em; }
.media-chat-markdown :deep(h2) { font-size: 1.25em; }
.media-chat-markdown :deep(h3) { font-size: 1.12em; }
.media-chat-markdown :deep(h4),
.media-chat-markdown :deep(h5),
.media-chat-markdown :deep(h6) { font-size: 1em; }
.media-chat-markdown :deep(ul), .media-chat-markdown :deep(ol) { padding-inline-start: 1.6em; }
.media-chat-markdown :deep(ul) { list-style-type: disc; }
.media-chat-markdown :deep(ol) { list-style-type: decimal; }
.media-chat-markdown :deep(ul ul) { list-style-type: circle; }
.media-chat-markdown :deep(li + li) { margin-top: .25em; }
.media-chat-markdown :deep(li > ul), .media-chat-markdown :deep(li > ol) { margin: .3em 0; }
.media-chat-markdown :deep(blockquote) { border-inline-start: 3px solid color-mix(in srgb, var(--color-base-content) 20%, transparent); padding-inline-start: .85em; color: color-mix(in srgb, var(--color-base-content) 70%, transparent); }
.media-chat-markdown :deep(a) { color: var(--color-primary); text-decoration: underline; text-underline-offset: 2px; }
.media-chat-markdown :deep(a:focus-visible) { outline: 2px solid var(--color-primary); outline-offset: 2px; border-radius: 2px; }
.media-chat-markdown :deep(code) { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: .92em; }
.media-chat-markdown :deep(:not(pre) > code) { padding: .1em .3em; border-radius: .25em; background: color-mix(in srgb, var(--color-base-content) 7%, transparent); }
.media-chat-markdown :deep(pre) { max-width: 100%; overflow-x: auto; padding: .75em 1em; border: 1px solid color-mix(in srgb, var(--color-base-content) 10%, transparent); border-radius: .5em; background: var(--color-base-200); white-space: pre; overflow-wrap: normal; }
.media-chat-markdown :deep(pre code) { padding: 0; background: none; white-space: inherit; }
.media-chat-markdown :deep(.chat-markdown-table) { max-width: 100%; overflow-x: auto; }
.media-chat-markdown :deep(table) { min-width: 100%; border-collapse: collapse; }
.media-chat-markdown :deep(th), .media-chat-markdown :deep(td) { padding: .45em .7em; border: 1px solid color-mix(in srgb, var(--color-base-content) 15%, transparent); text-align: start; vertical-align: top; min-width: 5em; }
.media-chat-markdown :deep(th) { font-weight: 650; background: var(--color-base-200); }
.media-chat-markdown :deep(hr) { margin: 1em 0; border: 0; border-top: 1px solid color-mix(in srgb, var(--color-base-content) 15%, transparent); }
</style>
