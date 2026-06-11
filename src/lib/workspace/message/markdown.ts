/**
 * Markdown → safe HTML for assistant turns. Same escape + sanitize policy as
 * the legacy AssistantMessage (escape inline HTML, parse with marked, strip
 * dangerous tags/attrs/protocols), extracted so the new message stream and any
 * future surface share one implementation.
 */
import { marked } from "marked";

function escapeInlineHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

const BLOCKED_TAGS = new Set(["script", "iframe", "object", "embed", "link", "meta", "style"]);
const ALLOWED_URL_PROTOCOLS = new Set(["http:", "https:", "mailto:", "data:", "blob:"]);

function sanitize(html: string): string {
  if (typeof window === "undefined") return html;
  const template = document.createElement("template");
  template.innerHTML = html;

  const walk = (node: Node) => {
    if (node.nodeType !== Node.ELEMENT_NODE) {
      for (const child of Array.from(node.childNodes)) walk(child);
      return;
    }
    const el = node as HTMLElement;
    if (BLOCKED_TAGS.has(el.tagName.toLowerCase())) {
      el.remove();
      return;
    }
    for (const attr of Array.from(el.attributes)) {
      const name = attr.name.toLowerCase();
      if (name.startsWith("on")) {
        el.removeAttribute(attr.name);
        continue;
      }
      if (name === "href" || name === "src") {
        try {
          const url = new URL(attr.value.trim(), window.location.href);
          if (!ALLOWED_URL_PROTOCOLS.has(url.protocol)) el.removeAttribute(attr.name);
        } catch {
          el.removeAttribute(attr.name);
        }
      }
    }
    for (const child of Array.from(el.childNodes)) walk(child);
  };
  walk(template.content);
  return template.innerHTML;
}

export function renderMarkdown(text: string): string {
  const rendered = marked.parse(escapeInlineHtml(text), { async: false, breaks: true }) as string;
  return sanitize(rendered);
}
