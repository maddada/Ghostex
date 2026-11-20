#!/bin/bash
# The CLI half of the release smoke test (smoke-test.md). The agent running the
# smoke does only the GUI presses; this script does everything the ghostex CLI
# can: the crash baseline, the sessions for 2.1, 2.2, 3.1, 3.2 and 4.1, the
# messages, the waits for replies, the 5.3 page, putting Claude's and Codex's
# saved defaults back after 6.1 and 6.2, and the clean-up. Every ref goes into
# one file, $W/refs.json, which cleanup reads, so nothing is left behind.
#
#   smoke.sh run [--start-coordinators]   baseline, models, create and send
#   smoke.sh check [step...]              crash 2.1 2.2 3.1 3.2 4.1 (default), or 6.1, 6.2;
#                                         a coordinator's 120 s run from `check 3.x`, so run
#                                         it right after pressing Start (bare `check` skips
#                                         coordinators nobody started)
#   smoke.sh page [2.1]                   5.3: publish the fixture, have that chat echo its card
#   smoke.sh add <kind> <value> [<value>] record what a GUI step made: session <ref>,
#                                         page <id>, tab <devtools-port> <target-id>, project <id>
#   smoke.sh restore-defaults [claude|codex]
#   smoke.sh show                         print $W and refs.json
#   smoke.sh cleanup                      close, kill, retire and restore everything recorded
#
# Settings (environment):
#   SMOKE_RELEASE      release point (default: git rev-parse HEAD of this checkout;
#                      set it to the release worktree's HEAD when the build came from there)
#   SMOKE_RUN          run number (default: the next free one for that release)
#   SMOKE_ROOT         where run folders go (default ~/.local/share/agent-work/Ghostex/smoke)
#   SMOKE_W            the run folder for check/page/add/cleanup (default: the one the
#                      last `run` wrote to $SMOKE_ROOT/current)
#   SMOKE_PROJECT_ID   project for the sessions (default: `ghostex agents whoami --json`)
#   SMOKE_STATE_DIR    Ghostex state folder for the crash baseline (default
#                      $GHOSTEX_HOME/state on a test server, else ~/.local/state/ghostex)
#   SMOKE_CLAUDE_SETTINGS, SMOKE_CODEX_CONFIG   the defaults files (default
#                      ~/.claude/settings.json and ~/.codex/config.toml)
#   SMOKE_REPLY_TIMEOUT  seconds a reply may take (default 120)
#
# Exit codes: 0 every check passed, 1 a check failed or timed out, 2 usage or setup error.
# Needs bash 3.2 (macOS /bin/bash), jq, and the ghostex CLI of the server under test first
# on PATH. To prove it on a throwaway test server (`cargo xtask test-server`), run it from a
# terminal session on that server: agents send refuses a shell with no session identity.

set -u

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
SMOKE_ROOT="${SMOKE_ROOT:-$HOME/.local/share/agent-work/Ghostex/smoke}"
REPLY_TIMEOUT="${SMOKE_REPLY_TIMEOUT:-120}"
if [ -n "${SMOKE_STATE_DIR:-}" ]; then
  STATE_DIR="$SMOKE_STATE_DIR"
elif [ -n "${GHOSTEX_HOME:-}" ]; then
  STATE_DIR="$GHOSTEX_HOME/state"
else
  STATE_DIR="$HOME/.local/state/ghostex"
fi
LOGS="$STATE_DIR/logs"
CLAUDE_SETTINGS="${SMOKE_CLAUDE_SETTINGS:-$HOME/.claude/settings.json}"
CODEX_CONFIG="${SMOKE_CODEX_CONFIG:-$HOME/.codex/config.toml}"

say() { printf '%s\n' "$*"; }
die() { printf 'smoke.sh: %s\n' "$*" >&2; exit 2; }
now() { date +%s; }
stamp() { date '+%Y-%m-%dT%H:%M:%S%z'; }

command -v jq >/dev/null || die "jq is not on PATH"
command -v ghostex >/dev/null || die "ghostex is not on PATH"

# ---------------------------------------------------------------- refs.json

W=""
REFS=""
use_w() {
  W="${SMOKE_W:-}"
  if [ -z "$W" ] && [ -f "$SMOKE_ROOT/current" ]; then W="$(cat "$SMOKE_ROOT/current")"; fi
  [ -n "$W" ] || die "no run folder: run \`smoke.sh run\` first or set SMOKE_W"
  REFS="$W/refs.json"
  [ -f "$REFS" ] || die "no $REFS"
}

# jqset '<jq filter>' [jq args...]: rewrite refs.json through a filter.
jqset() {
  local filter="$1"; shift
  jq "$@" "$filter" "$REFS" > "$REFS.new" || die "could not update $REFS"
  mv "$REFS.new" "$REFS"
}
jqget() { jq -r "$@" "$REFS"; }

result() { # result <step> <PASS|WARN|FAIL|STOP|NOT RUN|PENDING> <detail>
  jqset '.results[$s] = {status: $st, detail: $d, at: $at}' \
    --arg s "$1" --arg st "$2" --arg d "$3" --arg at "$(stamp)"
  printf '%-4s %s  %s\n' "$1" "$2" "$3"
}

# ---------------------------------------------------------------- waiting for a reply

# Ghostex's own wait (`ghostex agents wait-reply`, which rides the chat's long
# poll) once the CLI has it; a polling loop over read-session-chat until then.
# wait-reply waits for the *next* reply (`--after now` by default), so a reply
# that came before the call would be missed: check the transcript first, and
# call it in slices of at most 20 s with a transcript check between, so a reply
# landing between two calls is seen too. Exit 0 when an assistant row contains
# the text, 1 on timeout.
have_wait_reply() { ghostex agents --help 2>/dev/null | grep -q 'wait-reply'; }
wait_reply() { # wait_reply <ref> <text> <seconds>
  local ref="$1" text="$2" secs="$3" deadline left slice builtin=0
  deadline=$(( $(now) + secs ))
  have_wait_reply && builtin=1
  while :; do
    if assistant_said "$ref" "$text"; then return 0; fi
    left=$(( deadline - $(now) ))
    [ $left -le 0 ] && return 1
    if [ $builtin = 1 ]; then
      slice=$left; [ $slice -gt 20 ] && slice=20
      local t0; t0=$(now)
      ghostex agents wait-reply "$ref" --contains "$text" --timeout "$slice" >/dev/null 2>&1 && return 0
      # A refusal comes back at once (not an agent, a closed session): don't spin on it.
      [ $(( $(now) - t0 )) -lt 2 ] && sleep 3
    else
      sleep 3
    fi
  done
}
assistant_said() { # assistant_said <ref> <text>
  ghostex read-session-chat "$1" --all --last 8 --role assistant --format text 2>/dev/null \
    | grep -qF -- "$2"
}

# ---------------------------------------------------------------- sending

# agents send, coping with what the runs met: Codex's "Update available" picker
# (Escape picks Skip, then resend with the same request id) and a shell that is
# not in a Ghostex session (a throwaway test server, where agents send cannot name the caller).
# Codex can open on its own "Update available" picker (11.11.0 and 11.12.0 runs).
# Escape picks Skip; never "Update now". Returns 0 when it pressed Escape.
skip_codex_update() { # skip_codex_update <ref>
  if ghostex read-text "$1" --lines 30 2>/dev/null | grep -q 'Update available'; then
    if ghostex read-text "$1" --lines 30 2>/dev/null | grep -q -i 'esc skip'; then
      ghostex send-key "$1" escape >/dev/null 2>&1
      say "  $1: skipped Codex's update picker (Escape)"
      return 0
    fi
  fi
  return 1
}

SEND_ERROR=""
SEND_ROUTE=agents   # becomes "chat" once agents send cannot name this shell's session
send_once() { # send_once <ref> <text> [<request-id>]
  if [ "$SEND_ROUTE" = chat ]; then
    ghostex send-session-chat-message "$1" "$2" 2>&1
  elif [ -n "${3:-}" ]; then
    ghostex agents send "$1" "$2" --request-id "$3" 2>&1
  else
    ghostex agents send "$1" "$2" 2>&1
  fi
}
send_msg() { # send_msg <ref> <text>
  local ref="$1" text="$2" out rid="" tries=0
  while [ $tries -lt 8 ]; do
    tries=$((tries + 1))
    if out="$(send_once "$ref" "$text" "$rid")"; then
      say "  sent to $ref$([ "$SEND_ROUTE" = chat ] && echo ' (send-session-chat-message: this shell is no agent session)')"
      return 0
    fi
    rid="$(printf '%s\n' "$out" | grep -oE -- '--request-id [^ ]+' | head -1 | awk '{print $2}')"
    if [ "$SEND_ROUTE" = agents ] && printf '%s' "$out" | grep -q 'Cannot identify'; then
      # A shell that is no agent session (a throwaway test server's terminal): type it into the chat.
      SEND_ROUTE=chat; continue
    fi
    if printf '%s' "$out" | grep -q 'Update Codex'; then
      # Codex's "Update available" picker: Escape picks Skip (never "Update now"), then resend.
      say "  $ref shows Codex's update picker: Escape (Skip), then resend"
      ghostex send-key "$ref" escape >/dev/null 2>&1
      sleep 2; continue
    fi
    if printf '%s' "$out" | grep -q -i -E 'starting|not ready|not running yet'; then
      sleep 4; continue
    fi
    break
  done
  SEND_ERROR="$out"
  say "  send to $ref failed: $out"
  return 1
}

# ---------------------------------------------------------------- crash baseline

crash_snapshot() { # crash_snapshot <suffix>: before|now
  local sfx="$1"
  ls "$LOGS/crashes" > "$W/crashes-$sfx.txt" 2>&1
  if [ -f "$LOGS/gpui-crash-reports.log" ]; then
    wc -l < "$LOGS/gpui-crash-reports.log" | tr -d ' ' > "$W/panic-$sfx.txt"
  else
    echo "absent" > "$W/panic-$sfx.txt"
  fi
  ls "$HOME/Library/Logs/DiagnosticReports" 2>/dev/null | grep -i ghostex > "$W/diag-$sfx.txt"
  ls -la "$LOGS/crash-reports/" 2>&1 | grep -v '^total' > "$W/crashreports-$sfx.txt"
}
check_crash() {
  crash_snapshot now
  local bad=""
  cmp -s "$W/crashes-before.txt" "$W/crashes-now.txt" || bad="$bad crashes/ changed;"
  cmp -s "$W/panic-before.txt" "$W/panic-now.txt" \
    || bad="$bad panic log $(cat "$W/panic-before.txt") -> $(cat "$W/panic-now.txt") lines;"
  cmp -s "$W/diag-before.txt" "$W/diag-now.txt" || bad="$bad new DiagnosticReport;"
  cmp -s "$W/crashreports-before.txt" "$W/crashreports-now.txt" || bad="$bad crash-reports/ changed;"
  local t0 t1; t0=$(now)
  if ! ghostex sessions --json >/dev/null 2>&1; then bad="$bad ghostex sessions failed;"; fi
  t1=$(now)
  [ $(( t1 - t0 )) -gt 15 ] && bad="$bad ghostex sessions took $(( t1 - t0 )) s;"
  if [ -z "$bad" ]; then
    result crash PASS "baseline unchanged (state $STATE_DIR; panic log: $(cat "$W/panic-now.txt")), ghostex sessions $(( t1 - t0 )) s"
    return 0
  fi
  result crash STOP "${bad# }"
  return 1
}

# ---------------------------------------------------------------- models and defaults

latest_model() { # latest_model <agentId>: first non-alias model in coordinator options
  ghostex coordinator options --json 2>/dev/null | jq -r --arg a "$1" '
    [.agents[] | select(.agentId == $a) | .models[]?.value
     | select(test("^[a-z]+-[a-z0-9.-]*[0-9]") and (test("\\[") | not))] | first // empty'
}

backup_defaults() {
  if [ -f "$CLAUDE_SETTINGS" ]; then cp "$CLAUDE_SETTINGS" "$W/claude-settings.before.json"; fi
  if [ -f "$CODEX_CONFIG" ]; then cp "$CODEX_CONFIG" "$W/codex-config.before.toml"; fi
  jqset '.defaults = {
      claude: {file: $cf, backup: (if $cb then $cw else null end), restored: false},
      codex:  {file: $xf, backup: (if $xb then $xw else null end), restored: false}}' \
    --arg cf "$CLAUDE_SETTINGS" --arg xf "$CODEX_CONFIG" \
    --arg cw "$W/claude-settings.before.json" --arg xw "$W/codex-config.before.toml" \
    --argjson cb "$([ -f "$CLAUDE_SETTINGS" ] && echo true || echo false)" \
    --argjson xb "$([ -f "$CODEX_CONFIG" ] && echo true || echo false)"
}

# Put back only the keys a model pick writes, so anything else the user changed
# in those files meanwhile stays: Claude's `model` in settings.json, Codex's
# top-level `model` and `model_reasoning_effort` in config.toml.
restore_claude() {
  local backup; backup="$(jqget '.defaults.claude.backup // empty')"
  [ -n "$backup" ] && [ -f "$backup" ] || { say "  Claude: no backup (the file did not exist), nothing to restore"; return 0; }
  [ -f "$CLAUDE_SETTINGS" ] || { cp "$backup" "$CLAUDE_SETTINGS"; return 0; }
  local before; before="$(jq -c '.model // null' "$backup")"
  jq --argjson m "$before" 'if $m == null then del(.model) else .model = $m end' \
    "$CLAUDE_SETTINGS" > "$W/claude-settings.restoring.json" || return 1
  cat "$W/claude-settings.restoring.json" > "$CLAUDE_SETTINGS"
  if [ "$(jq -c '.model // null' "$CLAUDE_SETTINGS")" = "$before" ]; then
    jqset '.defaults.claude.restored = true'
    say "  Claude default model back to $before ($(diff -q "$backup" "$CLAUDE_SETTINGS" >/dev/null && echo 'file identical to the backup' || echo 'other keys left as they are now'))"
  else
    say "  Claude default model NOT restored"; return 1
  fi
}
toml_top_value() { # toml_top_value <file> <key>: the line for a top-level key (before any [table])
  awk -v k="$2" '/^\[/{exit} $0 ~ "^"k"[ \t]*=" {print; exit}' "$1"
}
restore_codex() {
  local backup; backup="$(jqget '.defaults.codex.backup // empty')"
  [ -n "$backup" ] && [ -f "$backup" ] || { say "  Codex: no backup (the file did not exist), nothing to restore"; return 0; }
  [ -f "$CODEX_CONFIG" ] || { cp "$backup" "$CODEX_CONFIG"; return 0; }
  local key line tmp="$W/codex-config.restoring.toml"
  cp "$CODEX_CONFIG" "$tmp"
  for key in model model_reasoning_effort; do
    line="$(toml_top_value "$backup" "$key")"
    awk -v k="$key" -v line="$line" '
      BEGIN { done = 0; top = 1 }
      /^\[/ { if (!done && line != "") { print line; done = 1 } top = 0 }
      top && $0 ~ "^"k"[ \t]*=" { if (line != "" && !done) { print line; done = 1 } next }
      { print }
      END { if (!done && line != "") print line }' "$tmp" > "$tmp.2" && mv "$tmp.2" "$tmp"
  done
  cat "$tmp" > "$CODEX_CONFIG"
  local ok=1
  for key in model model_reasoning_effort; do
    [ "$(toml_top_value "$CODEX_CONFIG" "$key")" = "$(toml_top_value "$backup" "$key")" ] || ok=0
  done
  if [ $ok = 1 ]; then
    jqset '.defaults.codex.restored = true'
    say "  Codex model and effort back to: $(toml_top_value "$backup" model | tr -d '\n') $(toml_top_value "$backup" model_reasoning_effort) ($(diff -q "$backup" "$CODEX_CONFIG" >/dev/null && echo 'file identical to the backup' || echo 'other keys left as they are now'))"
  else
    say "  Codex defaults NOT restored"; return 1
  fi
}
restore_defaults() {
  local which="${1:-both}" rc=0
  case "$which" in
    claude) restore_claude || rc=1 ;;
    codex) restore_codex || rc=1 ;;
    both) restore_claude || rc=1; restore_codex || rc=1 ;;
    *) die "restore-defaults takes claude or codex" ;;
  esac
  return $rc
}

# ---------------------------------------------------------------- run

cmd_run() {
  local start_coords=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --start-coordinators) start_coords=1 ;;
      *) die "run: unknown flag $1" ;;
    esac
    shift
  done

  local release run
  release="${SMOKE_RELEASE:-$(git -C "$REPO_DIR" rev-parse HEAD)}"
  local short; short="$(printf '%s' "$release" | cut -c1-9)"
  if [ -n "${SMOKE_RUN:-}" ]; then run="$SMOKE_RUN"; else
    run=1; while [ -e "$SMOKE_ROOT/$short-run$run" ]; do run=$((run + 1)); done
  fi
  W="$SMOKE_ROOT/$short-run$run"
  [ -e "$W/refs.json" ] && die "$W/refs.json exists: pick another SMOKE_RUN"
  mkdir -p "$W" || die "cannot make $W"
  REFS="$W/refs.json"
  printf '%s\n' "$W" > "$SMOKE_ROOT/current"

  local P="${SMOKE_PROJECT_ID:-}"
  if [ -z "$P" ]; then P="$(ghostex agents whoami --json 2>/dev/null | jq -r '.projectId // .project.id // empty')"; fi
  [ -n "$P" ] || die "no project id: set SMOKE_PROJECT_ID (no Ghostex session in this shell?)"

  local mc mx
  mc="$(latest_model claude)"; mx="$(latest_model codex)"
  [ -n "$mc" ] && [ -n "$mx" ] || die "could not read the latest models from \`ghostex coordinator options --json\`"

  jq -n --arg w "$W" --arg rel "$release" --argjson run "$run" --arg p "$P" \
    --arg srv "${GHOSTEX_GXSERVER_BASE_URL:-default}" --arg cli "$(command -v ghostex)" \
    --arg st "$STATE_DIR" --arg at "$(stamp)" --arg mc "$mc" --arg mx "$mx" '
    {w: $w, release: $rel, run: $run, projectId: $p, server: $srv, cli: $cli, stateDir: $st,
     startedAt: $at, models: {claude: $mc, codex: $mx}, sessions: {}, extra: [], results: {},
     cleanedUp: []}' > "$REFS"

  say "Smoke run $run for $short in $W"
  say "Server: ${GHOSTEX_GXSERVER_BASE_URL:-the default (real) gxserver}  CLI: $(command -v ghostex)  Project: $P"
  say "Models: Claude $mc, Codex $mx"

  crash_snapshot before
  say "Crash baseline saved (crashes-, panic-, diag-, crashreports-before.txt; state $STATE_DIR)"
  backup_defaults
  say "Defaults saved: $CLAUDE_SETTINGS, $CODEX_CONFIG"

  local rc=0 step agent model out ref sid
  # 2.1 and 2.2: threads on the latest model.
  for step in 2.1 2.2; do
    if [ "$step" = 2.1 ]; then agent=claude; model="$mc"; else agent=codex; model="$mx"; fi
    out="$(ghostex create-agent "$agent" --project-id "$P" --model "$model" --effort low 2>&1)"
    ref="$(printf '%s' "$out" | jq -r '.session.globalRef // empty' 2>/dev/null)"
    sid="$(printf '%s' "$out" | jq -r '.session.sessionId // empty' 2>/dev/null)"
    printf '%s\n' "$out" > "$W/c${step/./}.json"
    if [ -z "$ref" ]; then say "$step create failed: $out"; rc=1; continue; fi
    jqset '.sessions[$s] = {ref: $r, sessionId: $i, kind: "agent", agent: $a, model: $m,
                            expect: ("smoke-ok-" + $s), createdAt: $at}' \
      --arg s "$step" --arg r "$ref" --arg i "$sid" --arg a "$agent" --arg m "$model" --arg at "$(stamp)"
    ghostex rename-session "$sid" "smoke-$step-run$run" >/dev/null 2>&1
    say "$step $agent $model: $ref"
  done
  # 3.1 and 3.2: coordinators. From 11.10.0 the first message waits on a card
  # in the chat box; the GUI presses its Start (--start-coordinators sends it now).
  for step in 3.1 3.2; do
    if [ "$step" = 3.1 ]; then agent=claude; else agent=codex; fi
    if [ $start_coords = 1 ]; then
      out="$(ghostex coordinator create --project-id "$P" --agent "$agent" --title "smoke-$step-run$run" \
        --task "Reply with exactly: smoke-ok-$step" --start --json 2>&1)"
    else
      out="$(ghostex coordinator create --project-id "$P" --agent "$agent" --title "smoke-$step-run$run" \
        --task "Reply with exactly: smoke-ok-$step" --json 2>&1)"
    fi
    printf '%s\n' "$out" > "$W/c${step/./}.json"
    ref="$(printf '%s' "$out" | jq -r '.globalRef // .session.globalRef // empty' 2>/dev/null)"
    if [ -z "$ref" ]; then say "$step create failed: $out"; rc=1; continue; fi
    local started=null startedEpoch=null
    if [ $start_coords = 1 ]; then started="\"$(stamp)\""; startedEpoch="$(now)"; fi
    jqset '.sessions[$s] = {ref: $r, sessionId: ($r | split(":") | last), kind: "coordinator",
                            agent: $a, expect: ("smoke-ok-" + $s), createdAt: $at,
                            startedAt: $started, startedEpoch: $se, firstMessage: $fm}' \
      --arg s "$step" --arg r "$ref" --arg a "$agent" --arg at "$(stamp)" \
      --argjson started "$started" --argjson se "$startedEpoch" --arg fm "$(printf '%s' "$out" | jq -r '.firstMessage // .taskStatus // ""')"
    say "$step coordinator $agent: $ref ($( [ $start_coords = 1 ] && echo started || echo 'First message card waits: press Start, then smoke.sh check '"$step" ))"
  done
  # 4.1: a terminal, a command sent from the CLI.
  out="$(ghostex create-session "smoke-4.1-run$run" --start --project-id "$P" --json 2>&1)"
  printf '%s\n' "$out" > "$W/c41.json"
  ref="$(printf '%s' "$out" | jq -r '.session.globalRef // empty' 2>/dev/null)"
  if [ -n "$ref" ]; then
    jqset '.sessions["4.1"] = {ref: $r, sessionId: ($r | split(":") | last), kind: "terminal", createdAt: $at}' \
      --arg r "$ref" --arg at "$(stamp)"
    sleep 1
    ghostex send-text "$ref" 'echo smoke-ok-4.1' >/dev/null 2>&1 && ghostex send-enter "$ref" >/dev/null 2>&1 \
      || { say "4.1 send-text failed"; rc=1; }
    say "4.1 terminal: $ref (sent echo smoke-ok-4.1; the GUI types echo smoke-keys-4.1 and pastes echo smoke-paste-4.1)"
  else
    say "4.1 create failed: $out"; rc=1
  fi

  # Codex sessions may sit on the update picker; give them a few seconds to draw it.
  sleep 6
  for ref in $(jqget '.sessions[] | select(.agent == "codex") | .ref'); do skip_codex_update "$ref"; done
  # The messages for 2.1 and 2.2, once all are created (agents send waits for a starting agent).
  for step in 2.1 2.2; do
    ref="$(jqget --arg s "$step" '.sessions[$s].ref // empty')"
    [ -n "$ref" ] || continue
    if send_msg "$ref" "Reply with exactly: smoke-ok-$step"; then
      jqset '.sessions[$s].sentAt = $at | .sessions[$s].sentEpoch = $e' \
        --arg s "$step" --arg at "$(stamp)" --argjson e "$(now)"
    else
      jqset '.sessions[$s].sendError = $e' --arg s "$step" --arg e "$SEND_ERROR"
      rc=1
    fi
  done
  say "Refs: $REFS"
  return $rc
}

# ---------------------------------------------------------------- check

# Reply checks run side by side (each waits in the background into $W/wait-<step>),
# so every step's time is its own, not the time the step before it finished.
reply_prepare() { # reply_prepare <step> [explicit]: prints "<ref> <since-epoch> <seconds-left>" or fails
  local step="$1" explicit="${2:-}" ref since err
  ref="$(jqget --arg s "$step" '.sessions[$s].ref // empty')"
  [ -n "$ref" ] || { result "$step" "NOT RUN" "no session (see run's output)" >&2; return 1; }
  if [ "${step%%.*}" = 3 ]; then
    since="$(jqget --arg s "$step" '.sessions[$s].startedEpoch // empty')"
    if [ -z "$since" ] && [ -z "$explicit" ]; then
      result "$step" PENDING "not started: press its First message card's Start, then \`smoke.sh check $step\`" >&2
      return 1
    fi
    if [ -z "$since" ]; then
      # Named on the command line: its Start was pressed in the GUI just now. Time from here.
      since="$(now)"
      jqset '.sessions[$s].startedAt = $at | .sessions[$s].startedEpoch = $e' \
        --arg s "$step" --arg at "$(stamp)" --argjson e "$since"
    fi
  else
    since="$(jqget --arg s "$step" '.sessions[$s].sentEpoch // empty')"
    if [ -z "$since" ]; then
      err="$(jqget --arg s "$step" '.sessions[$s].sendError // "no record of the send"')"
      result "$step" WARN "its message was never sent: $err ($ref; check \`ghostex accounts\`)" >&2
      return 1
    fi
  fi
  [ "$(jqget --arg s "$step" '.sessions[$s].agent // ""')" = codex ] && skip_codex_update "$ref" >&2
  local left=$(( REPLY_TIMEOUT - ($(now) - since) ))
  [ $left -lt 10 ] && left=10
  printf '%s %s %s\n' "$ref" "$since" "$left"
}
reply_finish() { # reply_finish <step> <ref> <since>: reads $W/wait-<step> ("<rc> <epoch>")
  local step="$1" ref="$2" since="$3" rc at expect="smoke-ok-$1"
  read -r rc at < "$W/wait-$step" || { rc=1; at="$(now)"; }
  if [ "$rc" = 0 ]; then
    local extra=""
    if [ "${step%%.*}" = 3 ]; then
      if ghostex coordinator list --project-id "$(jqget .projectId)" --json 2>/dev/null | grep -qF "$ref"; then
        extra="; listed in coordinator list"
      else
        result "$step" WARN "$expect arrived but $ref is not in coordinator list"; return 1
      fi
    fi
    local took=$(( at - since )) late=""
    [ $took -gt "$REPLY_TIMEOUT" ] && late=" (already there when checked; it may have come earlier)"
    result "$step" PASS "$expect $took s after the $([ "${step%%.*}" = 3 ] && echo Start || echo send)$late$extra ($ref)"
    return 0
  fi
  local why="no $expect from the assistant $(( at - since )) s after the $([ "${step%%.*}" = 3 ] && echo Start || echo send)"
  if [ "${step%%.*}" = 3 ] && ghostex read-session-chat "$ref" --all --last 4 --format text 2>/dev/null | grep -q -i 'first message'; then
    why="$why (its First message card may still wait for Start)"
  fi
  result "$step" WARN "$why ($ref; check \`ghostex accounts\`)"
  return 1
}
check_replies() { # check_replies <explicit|""> <step>...
  local explicit="$1" step line ref since left rc=0 started=""; shift
  for step in "$@"; do
    line="$(reply_prepare "$step" "$explicit")" || { rc=1; continue; }
    set -- $line; ref="$1"; since="$2"; left="$3"
    ( wait_reply "$ref" "smoke-ok-$step" "$left"; printf '%s %s\n' "$?" "$(date +%s)" > "$W/wait-$step" ) &
    started="$started $step=$ref=$since"
  done
  wait
  for line in $started; do
    step="${line%%=*}"; ref="$(printf '%s' "$line" | cut -d= -f2)"; since="${line##*=}"
    reply_finish "$step" "$ref" "$since" || rc=1
  done
  return $rc
}

check_terminal() {
  local ref text missing=""
  ref="$(jqget '.sessions["4.1"].ref // empty')"
  [ -n "$ref" ] || { result 4.1 "NOT RUN" "no terminal"; return 1; }
  text="$(ghostex read-text "$ref" --lines 40 2>/dev/null)"
  printf '%s\n' "$text" > "$W/t41.txt"
  local want
  for want in smoke-ok-4.1 smoke-keys-4.1 smoke-paste-4.1; do
    # The echoed output line, not only the typed command line.
    printf '%s\n' "$text" | grep -qx "[[:space:]]*$want[[:space:]]*" || missing="$missing $want"
  done
  if printf '%s' "$text" | grep -qi "can't be delivered"; then
    result 4.1 STOP "\"can't be delivered\" in the terminal ($ref)"; return 1
  fi
  if [ -z "$missing" ]; then
    result 4.1 PASS "CLI text, typed keys and paste all echoed ($ref)"; return 0
  fi
  if [ "$missing" = " smoke-keys-4.1 smoke-paste-4.1" ]; then
    result 4.1 PENDING "CLI text landed; typed keys and paste not seen yet (GUI part; rerun check 4.1 after it)"
    return 1
  fi
  result 4.1 STOP "missing:$missing ($ref; text in $W/t41.txt)"
  return 1
}

check_model_step() { # 6.1 on the 2.1 chat (Claude), 6.2 on the 2.2 chat (Codex)
  local step="$1" base agent ref
  if [ "$step" = 6.1 ]; then base=2.1; agent=claude; else base=2.2; agent=codex; fi
  ref="$(jqget --arg s "$base" '.sessions[$s].ref // empty')"
  [ -n "$ref" ] || { result "$step" "NOT RUN" "no $base session"; return 1; }
  # The pick saved itself as the agent's default; put it back first, whatever the check says.
  restore_defaults "$agent"
  send_msg "$ref" "Reply with exactly: smoke-ok-$step" || { result "$step" WARN "send failed ($ref)"; return 1; }
  local t0; t0=$(now)
  if wait_reply "$ref" "smoke-ok-$step" "$REPLY_TIMEOUT"; then
    local model
    model="$(ghostex read-session-chat "$ref" --limit 4 --json 2>/dev/null \
      | jq -r '[.. | objects | .model? // empty | strings] | last // empty' 2>/dev/null)"
    result "$step" PASS "smoke-ok-$step in $(( $(now) - t0 )) s, reply model ${model:-unknown} (check the picker label and the empty box in the GUI); $agent default restored"
    return 0
  fi
  result "$step" WARN "no smoke-ok-$step after $REPLY_TIMEOUT s ($ref); $agent default restored"
  return 1
}

cmd_check() {
  use_w
  local steps="$*" rc=0 s explicit=1
  [ -n "$steps" ] || { steps="crash 2.1 2.2 3.1 3.2 4.1"; explicit=""; }
  local replies=""
  for s in $steps; do
    case "$s" in
      crash|4.1|6.1|6.2) ;;
      2.1|2.2|3.1|3.2) replies="$replies $s" ;;
      *) die "check: unknown step $s (crash 2.1 2.2 3.1 3.2 4.1 6.1 6.2)" ;;
    esac
  done
  for s in $steps; do
    case "$s" in
      crash) check_crash || rc=1 ;;
      4.1) check_terminal || rc=1 ;;
      6.1|6.2) check_model_step "$s" || rc=1 ;;
    esac
  done
  if [ -n "$replies" ]; then check_replies "$explicit" $replies || rc=1; fi
  return $rc
}

# ---------------------------------------------------------------- 5.3 page

cmd_page() {
  use_w
  local run short ref page out url
  run="$(jqget .run)"; short="$(jqget .release | cut -c1-9)"
  local on="${1:-2.1}"
  ref="$(jqget --arg s "$on" '.sessions[$s].ref // empty')"
  [ -n "$ref" ] || die "page: no $on session to echo the card"
  page="$W/smoke-page.html"
  cat > "$page" <<EOF
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="ghostex-page" content="v2">
<title>Smoke test 5.3 fixture $short run$run</title>
</head>
<body>
<gx-questions submit="Send to the agent">
  <header>
    <span class="gx-eyebrow">Smoke test</span>
    <h1>Typing fixture (5.3)</h1>
    <p class="gx-lede">A throwaway test page for the release smoke test. Please don't answer it.</p>
  </header>
  <gx-question tag="Q9531" title="Type something" type="text"></gx-question>
  <gx-question tag="Q9532" title="Pick one">
    <gx-choice>Alpha</gx-choice>
    <gx-choice>Beta</gx-choice>
  </gx-question>
</gx-questions>
</body>
</html>
EOF
  out="$(ghostex show "$page" --no-check --json 2>&1)"
  printf '%s\n' "$out" > "$W/page.json"
  local id block
  id="$(printf '%s' "$out" | jq -r '.id // empty' 2>/dev/null)"
  block="$(printf '%s' "$out" | jq -r '.block // empty' 2>/dev/null)"
  [ -n "$id" ] || die "page: ghostex show failed: $out"
  jqset '.extra += [{kind: "page", id: $id}]' --arg id "$id"
  { printf 'Reply with exactly the following block and nothing else:\n\n%s\n' "$block"; } > "$W/echo-block.txt"
  send_msg "$ref" "$(cat "$W/echo-block.txt")" || die "page: could not send the echo request to $ref"
  say "5.3 page $id published; asked $ref to echo its card"
  if wait_reply "$ref" "$id" "$REPLY_TIMEOUT"; then
    say "5.3 the card is in $ref's chat: open that chat and press the card's \`Open page …\` button (by its token)"
  else
    say "5.3 no echoed card after $REPLY_TIMEOUT s in $ref"; return 1
  fi
}

# ---------------------------------------------------------------- add, show

cmd_add() {
  use_w
  local kind="${1:-}"; shift || true
  case "$kind" in
    session) [ $# -ge 1 ] || die "add session <ref>"; jqset '.extra += [{kind: "session", ref: $v}]' --arg v "$1" ;;
    page) [ $# -ge 1 ] || die "add page <id>"; jqset '.extra += [{kind: "page", id: $v}]' --arg v "$1" ;;
    tab) [ $# -ge 2 ] || die "add tab <devtools-port> <target-id>"
      jqset '.extra += [{kind: "tab", port: $p, id: $v}]' --arg p "$1" --arg v "$2" ;;
    project) [ $# -ge 1 ] || die "add project <id>"; jqset '.extra += [{kind: "project", id: $v}]' --arg v "$1" ;;
    *) die "add: session <ref> | page <id> | tab <port> <id> | project <id>" ;;
  esac
  say "recorded $kind $*"
}

cmd_show() { use_w; say "W=$W"; jq . "$REFS"; }

# ---------------------------------------------------------------- cleanup

cmd_cleanup() {
  use_w
  local rc=0 item ref kind id port gone
  cleaned() { jqset '.cleanedUp += [$c]' --arg c "$1"; say "  $1"; }
  say "Cleaning up $W"
  # Sessions: threads and coordinators close (coordinators then list as stopped), terminals are killed.
  for item in $(jqget '.sessions | to_entries[] | "\(.key)=\(.value.kind)=\(.value.ref)"'); do
    kind="$(printf '%s' "$item" | cut -d= -f2)"; ref="$(printf '%s' "$item" | cut -d= -f3)"
    if [ "$kind" = terminal ]; then
      if ghostex kill "$ref" >/dev/null 2>&1; then cleaned "killed terminal $ref"; else say "  kill $ref failed"; rc=1; fi
    else
      if ghostex agents close "$ref" >/dev/null 2>&1; then cleaned "closed $kind $ref"; else say "  close $ref failed"; rc=1; fi
    fi
  done
  for item in $(jqget '.extra[] | "\(.kind)=\(.ref // .id)=\(.port // "")"'); do
    kind="${item%%=*}"; id="$(printf '%s' "$item" | cut -d= -f2)"; port="$(printf '%s' "$item" | cut -d= -f3)"
    case "$kind" in
      session) if ghostex agents close "$id" >/dev/null 2>&1 || ghostex kill "$id" >/dev/null 2>&1; then
          cleaned "closed session $id"; else say "  close $id failed"; rc=1; fi ;;
      page) if ghostex page retire "$id" >/dev/null 2>&1; then cleaned "retired page $id (close its window in the GUI if open)"
          else say "  retire $id failed"; rc=1; fi ;;
      tab) if curl -fsS "http://127.0.0.1:$port/json/close/$id" >/dev/null 2>&1; then cleaned "closed browser tab $id"
          else say "  closing tab $id on $port failed"; rc=1; fi ;;
      project) if ghostex remove-project --project-id "$id" >/dev/null 2>&1; then cleaned "removed project $id"
          else say "  remove-project $id failed"; rc=1; fi ;;
    esac
  done
  # Defaults: only those not put back yet (check 6.1 / 6.2 does it straight after the pick).
  [ "$(jqget '.defaults.claude.restored')" = true ] || { restore_claude && cleaned "Claude default restored"; } || rc=1
  [ "$(jqget '.defaults.codex.restored')" = true ] || { restore_codex && cleaned "Codex defaults restored"; } || rc=1
  # Prove the sessions are gone.
  sleep 2
  local live; live="$(ghostex sessions --json 2>/dev/null)"
  for ref in $(jqget '.sessions[].ref, (.extra[] | select(.kind == "session") | .ref)'); do
    gone="$(printf '%s' "$live" | jq -r --arg r "$ref" '[.. | objects | select(.globalRef? == $r and ((.lifecycleState? // "") | test("running|starting")))] | length' 2>/dev/null)"
    if [ "${gone:-0}" != 0 ]; then say "  STILL RUNNING: $ref"; rc=1; fi
  done
  jqset '.cleanedAt = $at' --arg at "$(stamp)"
  [ $rc = 0 ] && say "Clean-up done." || say "Clean-up left something (above)."
  return $rc
}

# ---------------------------------------------------------------- main

sub="${1:-}"; shift || true
case "$sub" in
  run) cmd_run "$@" ;;
  check) cmd_check "$@" ;;
  page) cmd_page "$@" ;;
  add) cmd_add "$@" ;;
  restore-defaults) use_w; restore_defaults "${1:-both}" ;;
  show) cmd_show ;;
  cleanup) cmd_cleanup ;;
  -h|--help|help|"") awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0" ;;
  *) die "unknown command $sub (run, check, page, add, restore-defaults, show, cleanup)" ;;
esac
