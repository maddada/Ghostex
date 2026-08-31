# PR #116: Project-aware board worker dispatch

```gherkin
Feature: Start Project Board work in the intended project

  Scenario: Current behavior before this change
    Given several projects share one global Beads board
    When a user or automation starts work without a project selector
    Then the worker can open in whichever project was updated most recently
    And an explicit project request can still reuse a linked worker from a sibling project

  Scenario: Expected behavior after this change
    Given several projects share one global Beads board
    When the caller selects a project by path or project ID
    Then Ghostex starts or reuses the worker only in that selected project
    When the caller does not select a project and no usable worker is already linked
    Then Ghostex creates the worker in the project that owns the shared Beads directory
    But an existing usable worker may be reused from any project sharing that board
    And explicit project selections never reuse a worker across project boundaries
```
