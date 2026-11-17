-- The model and effort an automation's new sessions start on (GitHub #662). NULL keeps the agent's own.
ALTER TABLE automations ADD COLUMN model TEXT;
ALTER TABLE automations ADD COLUMN effort TEXT;

PRAGMA user_version = 81;
