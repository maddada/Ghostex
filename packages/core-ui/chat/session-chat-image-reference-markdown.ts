/**
 * CommonMark rejects spaces in bare link destinations, while composer image
 * references deliberately carry literal machine paths. Turn only those image
 * references into link nodes so they keep their exact authored position and
 * reach the chat image-preview renderer even when the path contains spaces.
 */

interface MarkdownNode {
  children?: MarkdownNode[];
  type?: string;
  url?: string;
  value?: unknown;
}

const IMAGE_REFERENCE = /\[Image #(\d+)\]\(([^)\r\n]+)\)/g;

export function remarkSessionChatImageReferences() {
  return (tree: MarkdownNode): void => {
    const visit = (node: MarkdownNode): void => {
      if (!node.children) {
        return;
      }
      const children: MarkdownNode[] = [];
      for (const child of node.children) {
        if (child.type !== 'text' || typeof child.value !== 'string') {
          visit(child);
          children.push(child);
          continue;
        }
        let cursor = 0;
        for (const match of child.value.matchAll(IMAGE_REFERENCE)) {
          const index = match.index ?? 0;
          if (index > cursor) {
            children.push({ type: 'text', value: child.value.slice(cursor, index) });
          }
          children.push({
            children: [{ type: 'text', value: `Image #${match[1]}` }],
            type: 'link',
            url: match[2]?.trim() ?? '',
          });
          cursor = index + match[0].length;
        }
        if (cursor === 0) {
          children.push(child);
        } else if (cursor < child.value.length) {
          children.push({ type: 'text', value: child.value.slice(cursor) });
        }
      }
      node.children = children;
    };
    visit(tree);
  };
}
