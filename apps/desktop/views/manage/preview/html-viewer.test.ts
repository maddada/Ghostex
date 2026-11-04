import { describe, expect, test } from 'vitest';
import {
  MANAGE_PAGE_FEEDBACK_HEADER_SOURCE,
  buildManageAgentationBootstrapScript,
  manageHtmlPageName,
} from './html-viewer';

const renamePageFeedbackHeader = new Function(
  `${MANAGE_PAGE_FEEDBACK_HEADER_SOURCE}\nreturn renamePageFeedbackHeader;`
)() as (text: unknown, pathname: string, pageName: string) => unknown;

describe('manageHtmlPageName', () => {
  test('names a project file by its project-relative path', () => {
    expect(manageHtmlPageName('docs/samples/options-a.html', 'docs/samples/options-a.html')).toBe(
      'docs/samples/options-a.html'
    );
  });

  test('names an outside file by its file name, never an absolute or mount path', () => {
    expect(manageHtmlPageName('/Users/me/notes/report.html', '.ghostex-chat-file/abc123/report.html')).toBe(
      'report.html'
    );
    expect(manageHtmlPageName('C:\\Users\\me\\report.html', 'C:\\Users\\me\\report.html')).toBe('report.html');
    expect(manageHtmlPageName('/Users/me/report.html', '/Users/me/report.html')).toBe('report.html');
  });
});

describe('page feedback header', () => {
  test('swaps the srcdoc location for the page name and keeps the rest', () => {
    const copied = '## Page Feedback: srcdoc\n**Viewport:** 566×902\n\n### 1. b\n**Feedback:** use option 2\n';
    expect(renamePageFeedbackHeader(copied, 'srcdoc', 'docs/options-a.html')).toBe(
      '## Page Feedback: docs/options-a.html\n**Viewport:** 566×902\n\n### 1. b\n**Feedback:** use option 2\n'
    );
  });

  test('keeps a search or hash the page set after its location', () => {
    expect(renamePageFeedbackHeader('## Page Feedback: srcdoc#step-2\nbody', 'srcdoc', 'a.html')).toBe(
      '## Page Feedback: a.html#step-2\nbody'
    );
  });

  test('leaves other clipboard text, another location and a longer name alone', () => {
    expect(renamePageFeedbackHeader('hello', 'srcdoc', 'a.html')).toBe('hello');
    expect(renamePageFeedbackHeader('## Page Feedback: /other\nx', 'srcdoc', 'a.html')).toBe(
      '## Page Feedback: /other\nx'
    );
    expect(renamePageFeedbackHeader('## Page Feedback: srcdocs\nx', 'srcdoc', 'a.html')).toBe(
      '## Page Feedback: srcdocs\nx'
    );
    expect(renamePageFeedbackHeader(undefined, 'srcdoc', 'a.html')).toBeUndefined();
    expect(renamePageFeedbackHeader('## Page Feedback: srcdoc\nx', 'srcdoc', '')).toBe('## Page Feedback: srcdoc\nx');
  });
});

describe('Agentation bootstrap', () => {
  test('carries the page name and stays valid module source', () => {
    const script = buildManageAgentationBootstrapScript('docs/a"b.html');
    expect(script).toContain('const pageName = "docs/a\\"b.html";');
    expect(script).toContain('renamePageFeedbackHeader(text, location.pathname, pageName)');
    expect(() => new Function(script.replace(/^export /gmu, ''))).not.toThrow();
  });

  test('cannot be closed early by a file name holding a script tag', () => {
    const script = buildManageAgentationBootstrapScript('x</script><b>.html');
    expect(script).not.toContain('</script>');
    expect(script).toContain('x\\u003c/script>');
  });

  test('without a page name it renames nothing', () => {
    expect(buildManageAgentationBootstrapScript()).toContain('const pageName = "";');
  });
});
