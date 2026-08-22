export function ManageTextEditor({
  content,
  language,
  onChange,
}: {
  content: string;
  language: string;
  onChange: (content: string) => void;
}) {
  return (
    <textarea
      aria-label={`${language} editor`}
      className="manage-text-editor"
      onChange={(event) => onChange(event.currentTarget.value)}
      spellCheck={false}
      value={content}
    />
  );
}
