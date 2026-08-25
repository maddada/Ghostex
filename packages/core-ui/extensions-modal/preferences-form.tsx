import { Input } from '@/packages/components/ui/input';
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from '@/packages/components/ui/field';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/packages/components/ui/select';
import { Switch } from '@/packages/components/ui/switch';
import type { GhostexExtensionPreference, GhostexExtensionPreferenceValue } from '@/packages/shared/ghostex-extensions';

export function missingRequiredPreferences(
  definitions: readonly GhostexExtensionPreference[],
  values: Record<string, GhostexExtensionPreferenceValue>
): string[] {
  return definitions
    .filter((definition) => definition.required)
    .filter((definition) => {
      const value = values[definition.name];
      return definition.type === 'checkbox' ? value !== true : typeof value !== 'string' || !value.trim();
    })
    .map((definition) => definition.name);
}

export function PreferencesForm({
  definitions,
  onChange,
  values,
}: {
  definitions: readonly GhostexExtensionPreference[];
  onChange: (values: Record<string, GhostexExtensionPreferenceValue>) => void;
  values: Record<string, GhostexExtensionPreferenceValue>;
}) {
  const missing = new Set(missingRequiredPreferences(definitions, values));
  const update = (name: string, value: GhostexExtensionPreferenceValue) => {
    onChange({ ...values, [name]: value });
  };

  return (
    <FieldGroup className='gap-5'>
      {definitions.map((definition) => {
        const inputId = `extension-preference-${definition.name}`;
        const invalid = missing.has(definition.name);
        const value = values[definition.name] ?? definition.default ?? (definition.type === 'checkbox' ? false : '');
        return (
          <Field data-invalid={invalid || undefined} key={definition.name}>
            <FieldLabel htmlFor={inputId}>
              {definition.title}
              {definition.required ? <span aria-hidden='true'>*</span> : null}
            </FieldLabel>
            {definition.description ? <FieldDescription>{definition.description}</FieldDescription> : null}
            <FieldContent>
              {definition.type === 'checkbox' ? (
                <Switch
                  aria-invalid={invalid || undefined}
                  checked={value === true}
                  id={inputId}
                  onCheckedChange={(checked) => update(definition.name, checked)}
                />
              ) : definition.type === 'dropdown' ? (
                <Select onValueChange={(nextValue) => update(definition.name, nextValue)} value={String(value)}>
                  <SelectTrigger aria-invalid={invalid || undefined} className='w-full' id={inputId}>
                    <SelectValue placeholder={definition.placeholder ?? `Select ${definition.title}`} />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {(definition.data ?? []).map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                          {option.title}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              ) : (
                <Input
                  aria-invalid={invalid || undefined}
                  autoComplete={definition.type === 'password' ? 'new-password' : 'off'}
                  id={inputId}
                  onChange={(event) => update(definition.name, event.currentTarget.value)}
                  placeholder={
                    definition.placeholder ??
                    (definition.type === 'file'
                      ? 'Choose or enter a file path'
                      : definition.type === 'directory'
                        ? 'Choose or enter a directory path'
                        : undefined)
                  }
                  type={definition.type === 'password' ? 'password' : 'text'}
                  value={String(value)}
                />
              )}
              {invalid ? <FieldError>{definition.title} is required.</FieldError> : null}
            </FieldContent>
          </Field>
        );
      })}
    </FieldGroup>
  );
}
