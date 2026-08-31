# PR #113: Chat draft handoff across user or host hops

```gherkin
Feature: Keep Chat usable for agents launched through another user or host

  Scenario: Current behavior before this change
    Given an agent session is launched through SSH or another user or host hop
    And the user opens Chat while a draft may still be in the terminal
    When Ghostex tries to transfer the terminal draft through the remote agent's editor
    Then the remote terminal can open the wrong editor and become stuck
    And later Chat messages are rejected because the agent input is no longer ready

  Scenario: Expected behavior after this change
    Given an agent session's effective launch command uses a user or host hop
    When the user opens Chat
    Then Ghostex skips the unsupported terminal draft transfer
    And leaves the draft safely in the terminal
    And keeps the agent available for normal Chat messages
    But local agent sessions continue to transfer terminal drafts into Chat
```
