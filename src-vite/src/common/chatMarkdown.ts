import MarkdownIt from 'markdown-it';

export function safeChatMarkdownLink(value: string): string | null {
  value = value.trim();
  if (!/^https?:\/\//i.test(value)) return null;
  try {
    const url = new URL(value);
    return url.protocol === 'https:' || url.protocol === 'http:' ? url.href : null;
  } catch { return null; }
}

const markdown = new MarkdownIt({ html: false, breaks: true, linkify: true, typographer: false });
markdown.linkify.set({ fuzzyLink: false, fuzzyEmail: false });
markdown.validateLink = value => safeChatMarkdownLink(value) !== null;
markdown.renderer.rules.link_open = (tokens, index, options, _env, renderer) => {
  tokens[index].attrSet('rel', 'noopener noreferrer');
  return renderer.renderToken(tokens, index, options);
};
// Model-supplied images must not cause network requests or access local assets.
markdown.renderer.rules.image = (tokens, index, options, env, renderer) =>
  markdown.utils.escapeHtml(renderer.renderInlineAsText(tokens[index].children || [], options, env));
markdown.renderer.rules.table_open = (tokens, index, options, _env, renderer) =>
  `<div class="chat-markdown-table">${renderer.renderToken(tokens, index, options)}`;
markdown.renderer.rules.table_close = (tokens, index, options, _env, renderer) =>
  `${renderer.renderToken(tokens, index, options)}</div>`;

export function renderChatMarkdown(source: string): string {
  return markdown.render(source, {});
}
