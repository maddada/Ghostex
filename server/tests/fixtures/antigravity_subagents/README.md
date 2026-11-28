# Antigravity sub-agent run (agy 1.5.19 ACP server, CLI 1.5.18)

One real `agy acp --stdio --cli_dir_name ghostex-acp` run, recorded on 2026-11-28 with the
default agent on Gemini 3.8 Flash (Low). The prompt asked the lead to start one built-in
`research` sub-agent that read a three-file sandbox project and reported back.

- `acp.jsonl`: every JSON-RPC frame both ways, one `{"t", "dir", "msg"}` per line (`t` is seconds
  from the start, `dir` is `out` for client to agy and `in` for agy to client).
- `brain/<lead>/.system_generated/logs/transcript_full.jsonl`: the lead's step log.
- `brain/<lead>/.system_generated/subagents/<child>.json`: the link agy writes when the lead starts
  a child.
- `brain/<child>/.system_generated/logs/transcript_full.jsonl`: the child's own step log.

What the run shows:

- The lead's `invoke_subagent` call (`TypeName`, `Role`, `Prompt`) has no child id in the ACP
  stream. Its step-log result does (`Created the following subagent: {"conversationId": …}`).
- The child is a conversation of its own. Its first step is a `SYSTEM` message from the lead
  (`[Message] … sender=<lead> … content=<task>`), and it reports back with `send_message`, which
  reaches the lead as a `SYSTEM` message with `sender=<child>`.
- The child's calls that need an approval arrive on the lead's ACP session with `<child>:` before
  the call id. Its other calls are in its step log only.
- The lead's `session/prompt` stays open until the child has reported and the lead has answered.
- `state` in the link file stays `SUBAGENT_STATE_ALIVE` after the child's last reply.

Scrubbed before commit: home-folder paths (now `/home/user`), the sandbox path (now
`/home/user/tinycalc`), the contents of a personal notes file the agent read (now a one-line
stub), the user's own skills in `available_commands_update` (only agy's built-ins kept), the
account tier in `session/new`, local time-zone offsets (now UTC) and the client name. Not
included: agy's encrypted `conversations/<id>.db`, its server log (it names the signed-in
account), and the shared `conversation_summaries.db`.

Read by: `server/src/session_chat_antigravity_fleet.rs`, `server/src/session_chat_antigravity_mirror.rs`.
