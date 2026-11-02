---
name: ghostex-secrets
description: >-
  Use this skill whenever a task needs a secret only the user has: a password,
  a sudo password, an API key, an access token, a webhook signing secret, a
  database password, or a CLI login token. Inside a Ghostex session, never ask
  for the secret in chat, never have the user paste it into the chat, and never
  hand the command back to the user or suggest passwordless sudo just because
  it needs a password: run `ghostex secret request`, which shows the user a
  private card and gives you a private file holding the value, without the
  value entering the chat or your context.
---

# ghostex-secrets

Ghostex shows the user a private card above the chat box; what they type goes
to an owner-only file that your commands can read. The value never appears in
the chat, the transcript or your context, as long as you never print it.

## Ask

```bash
ghostex secret request \
  --label "Cloudflare API token" \
  --reason "wrangler needs it to deploy the worker. Create one at dash.cloudflare.com > My Profile > API Tokens with the Edit Workers template." \
  --placeholder "Paste the token"
```

- `--label`: what you need, the card's title.
- `--reason`: one or two sentences on what it is for and where the user gets
  it. Links are fine.
- `--timeout-minutes N`: how long to wait (default 10, at most 60).

The command waits for the answer and prints JSON:

- `{"status":"saved","file":"<path>","next":"…"}`: the value is in that file
  (owner-only, deleted after 24 hours).
- `{"status":"declined"|"timedOut"|"cancelled",…}` with exit code 1: the user
  chose not to, did not answer in time, or the request ended. After
  `declined`, do not ask again unless the user says so.

Ask once per secret and reuse the file for the rest of the task; ask again only
after the file is gone or the secret was wrong.

## Use the file, never the value

Read the file only inside the command that needs it:

```bash
CLOUDFLARE_API_TOKEN="$(cat '<file>')" wrangler deploy
gh auth login --with-token < '<file>'
PGPASSWORD="$(cat '<file>')" psql -h db.example.com -U app
```

Never `cat`, `echo`, `printf` or log the value on its own, never put it in a
command's arguments where it would be printed, never write it into the
project, a commit, a config file you did not mean to create, or the chat, and
never repeat it in your reply. If a tool prints its credentials back, pipe its
output through something that drops them.

## A sudo password

1. Settle the full command first: add its own `-y` (or similar) if it would
   ask to confirm, and give it any input it would read from stdin as a file
   argument, because the password replaces stdin.
2. Run `sudo -n <command>`. If it works, no password is needed: you are done.
3. If it fails for lack of a password, ask with
   `ghostex secret request --label "sudo password" --reason "To run <command> on this computer."`
4. Run exactly that command as `sudo -k -S -p '' <command> < '<file>'`.

Do not tell the user to run it themselves or to set up passwordless sudo
instead of asking. The command runs as root and could read the file anyway;
the recipe keeps the password out of the command line and the chat.

## When `ghostex secret` is unknown

Ghostex before this feature has no `ghostex secret` verb. Then tell the user
the task needs a secret and ask them to update Ghostex, or to put the value in
a file themselves and give you only its path. Never ask for the value in chat.

## 1Password

The user may answer with a 1Password reference (`op://vault/item/field`) when
a 1Password account is set up in Settings > Secrets; Ghostex reads it for them
and you get the same file. `ghostex guide` explains the Settings page.
