interface ThemeSettings {
  id: string;
  name: string;
  backgroundColor?: string;
  colors: Record<string, string>;
  syntaxTokens: Record<string, string>;
  fonts: {
    liveFont?: string;
    sourceFont?: string;
    liveFontWeight?: string;
    sourceFontWeight?: string;
    liveFontSize?: number | null;
    sourceFontSize?: number | null;
    h1FontSize?: number | null;
    h2FontSize?: number | null;
    h3FontSize?: number | null;
    h4FontSize?: number | null;
    h5FontSize?: number | null;
    h6FontSize?: number | null;
    h1FontWeight?: string;
    h2FontWeight?: string;
    h3FontWeight?: string;
    h4FontWeight?: string;
    h5FontWeight?: string;
    h6FontWeight?: string;
    liveLineHeight?: number;
    sourceLineHeight?: number;
  };
}

interface EditorDiagnostic {
  from: number;
  to: number;
  severity: 0 | 1 | 2 | 3;
  message: string;
  source?: string;
  code?: string;
}


interface HeadingInfo {
  text: string;
  level: number;
  from: number;
  to: number;
  lineFrom: number;
  lineTo: number;
  id: string;
}


