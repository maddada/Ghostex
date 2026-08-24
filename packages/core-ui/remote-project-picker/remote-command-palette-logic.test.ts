import { describe, expect, test } from 'vitest';
import { buildBrowseGroups, filterBrowseEntries } from './remote-command-palette-logic';

describe('remote project picker command logic', () => {
  const entries = [
    { name: '.config', fullPath: '/home/madda/.config' },
    { name: 'ghostex', fullPath: '/home/madda/ghostex' },
    { name: 'ghostty', fullPath: '/home/madda/ghostty' },
    { name: 'notes', fullPath: '/home/madda/notes' },
  ];

  test('filters hidden directories only when the typed prefix starts with dot', () => {
    expect(
      filterBrowseEntries({
        browseEntries: entries,
        browseFilterQuery: 'g',
        highlightedItemValue: 'browse:/home/madda/ghostex',
      })
    ).toMatchObject({
      exactEntry: null,
      filteredEntries: [
        { name: 'ghostex', fullPath: '/home/madda/ghostex' },
        { name: 'ghostty', fullPath: '/home/madda/ghostty' },
      ],
      highlightedEntry: { name: 'ghostex', fullPath: '/home/madda/ghostex' },
    });

    expect(
      filterBrowseEntries({
        browseEntries: entries,
        browseFilterQuery: '.',
        highlightedItemValue: null,
      }).filteredEntries
    ).toEqual([{ name: '.config', fullPath: '/home/madda/.config' }]);
  });

  test('builds browse rows with up navigation first', async () => {
    const calls: string[] = [];
    const [group] = buildBrowseGroups({
      browseEntries: entries.slice(1, 3),
      browseQuery: '~/g',
      browseTo: (name) => calls.push(`to:${name}`),
      browseUp: () => calls.push('up'),
      canBrowseUp: true,
      directoryIcon: 'dir',
      upIcon: 'up',
    });

    expect(group.items.map((item) => item.value)).toEqual([
      'browse:up',
      'browse:/home/madda/ghostex',
      'browse:/home/madda/ghostty',
    ]);
    await group.items[0].run();
    await group.items[1].run();
    expect(calls).toEqual(['up', 'to:ghostex']);
  });
});
