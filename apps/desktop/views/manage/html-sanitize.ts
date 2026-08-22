export function sanitizeManageHref(value: string): string | undefined {
  const trimmed = value.trim();
  if (!trimmed || /^(?:javascript|data|vbscript|file):/iu.test(trimmed)) {
    return undefined;
  }
  return trimmed;
}

export function sanitizeManageImageSrc(value: string): string | undefined {
  const trimmed = value.trim();
  if (!trimmed || /^(?:javascript|vbscript|file):/iu.test(trimmed)) {
    return undefined;
  }
  if (/^data:/iu.test(trimmed) && !/^data:image\//iu.test(trimmed)) {
    return undefined;
  }
  return trimmed;
}

export function sanitizeManageBlockHtml(html: string): string {
  const template = document.createElement("template");
  template.innerHTML = html;
  template.content.querySelectorAll("script, style, iframe, object, embed, link, meta").forEach((element) => {
    element.remove();
  });
  template.content.querySelectorAll("*").forEach((element) => {
    for (const attribute of Array.from(element.attributes)) {
      const name = attribute.name.toLocaleLowerCase();
      if (name.startsWith("on") || name === "style") {
        element.removeAttribute(attribute.name);
        continue;
      }
      if ((name === "href" || name === "src") && !sanitizeManageHref(attribute.value)) {
        element.removeAttribute(attribute.name);
      }
    }
    if (element instanceof HTMLAnchorElement && element.href && !element.href.startsWith("#")) {
      element.target = "_blank";
      element.rel = "noreferrer";
    }
  });
  return template.innerHTML;
}
