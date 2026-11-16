-- CDXC:AgentProviders 2026-11-13 DECISION:
-- User: "drop Gemini CLI fully fully". Google refuses its sign-in for individuals now and
-- Antigravity CLI covers Google's models, so Gemini CLI is no longer an agent Ghostex knows. A
-- project's saved agent rows (`customAgents`) and agent order keep the edits someone made to the
-- built-in Gemini agent; without this they would come back as a custom agent that still launches
-- `gemini`. Only the built-in id `gemini` is dropped: custom agents have `custom-…` ids, and the
-- Gemini usage account (accounts/usage_only/gemini.rs, Antigravity's quotas) is not stored here.
UPDATE projects
SET customAgentsJson = (
  SELECT json_group_array(json(value))
  FROM json_each(projects.customAgentsJson)
  WHERE type = 'object'
    AND lower(trim(coalesce(json_extract(value, '$.agentId'), ''))) <> 'gemini'
)
WHERE json_valid(customAgentsJson)
  AND json_type(customAgentsJson) = 'array'
  AND EXISTS (
    SELECT 1
    FROM json_each(projects.customAgentsJson)
    WHERE type = 'object'
      AND lower(trim(coalesce(json_extract(value, '$.agentId'), ''))) = 'gemini'
  );

UPDATE projects
SET customAgentOrderJson = (
  SELECT json_group_array(value)
  FROM json_each(projects.customAgentOrderJson)
  WHERE NOT (type = 'text' AND lower(trim(value)) = 'gemini')
)
WHERE json_valid(customAgentOrderJson)
  AND json_type(customAgentOrderJson) = 'array'
  AND EXISTS (
    SELECT 1
    FROM json_each(projects.customAgentOrderJson)
    WHERE type = 'text' AND lower(trim(value)) = 'gemini'
  );

PRAGMA user_version = 78;
