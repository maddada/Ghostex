import { describe, expect, test } from 'vitest';
import { isRepositoryCloneBranchNameInputValid, parseRepositoryCloneInput } from './repository-clone';

describe('parseRepositoryCloneInput', () => {
  test.each([
    ['https://github.com/yyopc/yyork.git', 'https://github.com/yyopc/yyork.git', 'yyork'],
    ['yyopc/yyork', 'https://github.com/yyopc/yyork.git', 'yyork'],
    ['git@github.com:yyopc/yyork.git', 'git@github.com:yyopc/yyork.git', 'yyork'],
    ['gh repo clone yyopc/yyork', 'https://github.com/yyopc/yyork.git', 'yyork'],
    ['github.com/yyopc/yyork', 'https://github.com/yyopc/yyork.git', 'yyork'],
    ['git clone https://github.com/yyopc/yyork', 'https://github.com/yyopc/yyork.git', 'yyork'],
    ['ssh://git@github.com/yyopc/yyork.git', 'ssh://git@github.com/yyopc/yyork.git', 'yyork'],
    [
      'https://codeberg.org/JohnWalkerx/nixConfigs.git',
      'https://codeberg.org/JohnWalkerx/nixConfigs.git',
      'nixConfigs',
    ],
    ['codeberg.org/JohnWalkerx/nixConfigs.git', 'https://codeberg.org/JohnWalkerx/nixConfigs.git', 'nixConfigs'],
  ])('normalizes %s', (input, cloneUrl, repositoryName) => {
    expect(parseRepositoryCloneInput(input)).toEqual({ cloneUrl, repositoryName });
  });

  test('ignores surrounding command text and browser path suffixes', () => {
    expect(parseRepositoryCloneInput('please clone https://github.com/maddada/zehn/tree/main')).toEqual({
      cloneUrl: 'https://github.com/maddada/zehn.git',
      repositoryName: 'zehn',
    });
  });
});

describe('isRepositoryCloneBranchNameInputValid', () => {
  test.each(['', 'main', 'master', 'feature/branch-picker', 'release/v4.0.0-beta.3'])('accepts %s', (branchName) => {
    expect(isRepositoryCloneBranchNameInputValid(branchName)).toBe(true);
  });

  test.each([' feature branch ', '-main', 'feature..branch', 'feature@{1}', '.hidden', 'release.lock'])(
    'rejects %s',
    (branchName) => {
      expect(isRepositoryCloneBranchNameInputValid(branchName)).toBe(false);
    }
  );
});
