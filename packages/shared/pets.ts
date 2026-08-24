export type PetId = 'boo' | 'codex' | 'dewey' | 'fireball' | 'rocky' | 'seedy' | 'stacky' | 'bsod' | 'null-signal';

export type PetOption = {
  description: string;
  displayName: string;
  id: PetId;
};

/*
 * CDXC:PetControlsVisibility 2026-07-21:
 * Keep the pet implementation available for a possible return, but hide its
 * sidebar-menu and Settings controls while the feature is out of the product UI.
 */
export const PET_CONTROLS_VISIBLE = false;

/*
CDXC:PetSelection 2026-05-21-10:23:
Boo is bundled from the verified CodexPetHub pet asset and is the first-install
default pet. Keep the pet metadata static in-app data; the package contributes
only a spritesheet and descriptive fields, not executable behavior.
*/
export const DEFAULT_PET_ID: PetId = 'boo';

export const PET_OPTIONS: ReadonlyArray<PetOption> = [
  {
    description: 'A friendly ghost for quiet workspace focus.',
    displayName: 'Boo',
    id: 'boo',
  },
  {
    description: 'The original Codex companion.',
    displayName: 'Codex',
    id: 'codex',
  },
  {
    description: 'A tidy duck for calm workspace days.',
    displayName: 'Dewey',
    id: 'dewey',
  },
  {
    description: 'Hot path energy for fast iteration.',
    displayName: 'Fireball',
    id: 'fireball',
  },
  {
    description: 'A steady rock when the diff gets large.',
    displayName: 'Rocky',
    id: 'rocky',
  },
  {
    description: 'Small green shoots for new ideas.',
    displayName: 'Seedy',
    id: 'seedy',
  },
  {
    description: 'A balanced stack for deep work.',
    displayName: 'Stacky',
    id: 'stacky',
  },
  {
    description: 'A tiny blue-screen companion.',
    displayName: 'BSOD',
    id: 'bsod',
  },
  {
    description: 'Quiet signal from the void.',
    displayName: 'Null Signal',
    id: 'null-signal',
  },
];

const PET_IDS = new Set(PET_OPTIONS.map((option) => option.id));

export function normalizePetId(value: string | undefined): PetId {
  return value && PET_IDS.has(value as PetId) ? (value as PetId) : DEFAULT_PET_ID;
}

export function getPetOption(id: PetId): PetOption {
  return PET_OPTIONS.find((option) => option.id === id) ?? PET_OPTIONS[0]!;
}
