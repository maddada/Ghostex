# PR #114: Hermes profile-aware Chat composer detection

```gherkin
Feature: Send Chat messages to Hermes sessions that use a named profile

  Scenario: Current behavior before this change
    Given a Hermes agent is launched with a named profile
    And its input prompt displays the profile name before the prompt symbol
    When the user sends a message from Chat
    Then Ghostex reports that the Hermes input box is not on screen
    And the visible terminal composer receives nothing

  Scenario: Expected behavior after this change
    Given a Hermes agent uses either the default profile or one named profile
    And its normal framed composer is visible
    When the user sends a message from Chat
    Then Ghostex recognizes the composer as ready
    And delivers the message to Hermes
    But prompt-like prose and numbered option dialogs remain excluded
```
