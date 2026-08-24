// @ts-nocheck
import { WidgetType } from '@codemirror/view';
import { createElement, Info, Lightbulb, AlertCircle, AlertTriangle, XCircle } from 'lucide';

export type AlertType = 'NOTE' | 'TIP' | 'IMPORTANT' | 'WARNING' | 'CAUTION';

export const ALERT_TYPES: readonly AlertType[] = ['NOTE', 'TIP', 'IMPORTANT', 'WARNING', 'CAUTION'];

const ALERT_DIRECTIVE_REGEX = />\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]/;

function getAlertDirectiveDetails(
  lineText: string,
  lineFrom: number
): {
  type: AlertType;
  directiveFrom: number;
  directiveTo: number;
  labelFrom: number;
  labelTo: number;
} | null {
  const match = ALERT_DIRECTIVE_REGEX.exec(lineText);
  if (!match) {
    return null;
  }
  const type = match[1] as AlertType;
  if (!ALERT_TYPES.includes(type)) {
    return null;
  }
  const directiveFrom = lineFrom + match.index;
  const directiveTo = directiveFrom + match[0].length;
  const labelOffset = match[0].indexOf(match[1]);
  const labelFrom = directiveFrom + labelOffset;
  return {
    type,
    directiveFrom,
    directiveTo,
    labelFrom,
    labelTo: labelFrom + match[1].length,
  };
}

function getAlertIconElement(type: AlertType): Element {
  const iconProps = { 'aria-hidden': 'true', width: 16, height: 16 };
  switch (type) {
    case 'NOTE':
      return createElement(Info, iconProps);
    case 'TIP':
      return createElement(Lightbulb, iconProps);
    case 'IMPORTANT':
      return createElement(AlertCircle, iconProps);
    case 'WARNING':
      return createElement(AlertTriangle, iconProps);
    case 'CAUTION':
      return createElement(XCircle, iconProps);
    default:
      return createElement(Info, iconProps);
  }
}

export class AlertIconWidget extends WidgetType {
  type: AlertType;

  constructor(type: AlertType) {
    super();
    this.type = type;
  }

  eq(other: WidgetType): boolean {
    return other instanceof AlertIconWidget && other.type === this.type;
  }

  toDOM(): HTMLElement {
    const container = document.createElement('span');
    container.className = 'meo-md-alert-icon';
    container.appendChild(getAlertIconElement(this.type));
    const label = document.createElement('span');
    label.className = 'meo-md-alert-label';
    label.textContent = this.type;
    container.appendChild(label);
    return container;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

export interface AlertBlock {
  type: AlertType;
  from: number;
  to: number;
  lineFrom: number;
  lineTo: number;
  directiveFrom: number;
  directiveTo: number;
  labelFrom: number;
  labelTo: number;
}

export function detectAlertInBlockquote(state: any, node: any): AlertBlock | null {
  const line = state.doc.lineAt(node.from);
  const lineText = state.doc.sliceString(line.from, line.to);
  const directive = getAlertDirectiveDetails(lineText, line.from);
  if (!directive) {
    return null;
  }

  return {
    type: directive.type,
    from: node.from,
    to: node.to,
    lineFrom: line.from,
    lineTo: line.to,
    directiveFrom: directive.directiveFrom,
    directiveTo: directive.directiveTo,
    labelFrom: directive.labelFrom,
    labelTo: directive.labelTo,
  };
}
