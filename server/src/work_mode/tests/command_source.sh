#!/bin/sh
# A command ticket source for the runner's tests. Its credential field `mode`, or a `mode.txt`
# in the folder it runs in when it has no credentials, picks how it answers. It records its argv,
# environment and input in that folder, and adds each call's verb to `calls.txt`.
verb="$1"
mode="${GHOSTEX_CREDENTIAL_MODE:-$(cat mode.txt 2>/dev/null)}"
printf '%s\n' "$@" >argv.txt
printf '%s\n' "$verb" >>calls.txt
env >env.txt
if [ "$verb" != info ] && [ "$mode" != late-stdin ]; then
	cat >stdin.txt
fi

# Its `team` is not read: a key-number ticket's team is its id's key.
good_ticket='"id": "ACM-12", "title": "Spec the contract", "url": "https://tracker.example/ACM-12",
  "state": { "group": "progress", "name": "In progress", "type": "active" },
  "updatedAt": "2026-10-28T09:12:00Z",
  "assignee": { "name": "Leandro", "avatarUrl": "https://tracker.example/a.png", "isMe": true },
  "labels": ["server"], "branchName": "leandro/acm-12-spec",
  "pullRequestUrls": ["https://github.com/acme/app/pull/7"], "team": "ACM", "repo": "acme/app"'

good_detail="{ $good_ticket, \"description\": \"Body\", \"author\": \"Ana\", \"comments\": [ { \"author\": \"Ana\", \"body\": \"Hi\", \"createdAt\": \"2026-10-28T08:00:00Z\" } ], \"secretField\": \"x\" }"

# The credential `a"b\c` as JSON writes it: escaped, and as \u escapes.
escaped='a\"b\\c and \u0061\u0022\u0062\u005c\u0063'

case "$mode" in
good)
	case "$verb" in
	info)
		cat <<'EOF'
{
  "id": "fixture", "name": "Fixture",
  "icon": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path d=\"M2 2h20v20H2z\" fill=\"currentColor\"/></svg>",
  "ids": { "format": "key-number", "keys": ["ACM", "ops"] },
  "credentials": [
    { "name": "mode", "label": "Mode", "secret": false },
    { "name": "token", "label": "API token", "required": true }
  ],
  "canCreate": true
}
EOF
		;;
	list) printf '{ "tickets": [ { %s } ] }\n' "$good_ticket" ;;
	detail) printf '%s\n' "$good_detail" ;;
	create) printf '{ "id": "ACM-41", "url": "https://tracker.example/ACM-41", "branchName": "leandro/acm-41" }\n' ;;
	esac
	;;
info-number)
	printf '{ "id": "fixture", "name": "Fixture", "ids": { "format": "number" }, "credentials": [ { "name": "mode", "label": "Mode", "secret": false } ] }\n'
	;;
info-switched)
	# An id no other test uses, so the id-wide format the cards read is this one's own.
	printf '{ "id": "card-switched-to", "name": "Switched", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "mode", "label": "Mode", "secret": false } ] }\n'
	;;
info-renamed)
	printf '{ "id": "renamed", "name": "Renamed", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [ { "name": "mode", "label": "Mode", "secret": false } ] }\n'
	;;
bad-icon)
	# `fixture` keeps key-number ids in every test that stores it (`check_one_format_per_id`).
	cat <<'EOF'
{ "id": "fixture", "name": "Fixture", "ids": { "format": "key-number", "keys": ["ACM"] }, "credentials": [],
  "icon": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><script>alert(1)</script><path d=\"M2 2h20v20H2z\"/></svg>" }
EOF
	;;
icon-token)
	printf '{ "id": "fixture", "name": "Fixture", "ids": { "format": "number" }, "credentials": [], "icon": "<svg xmlns=\\"http://www.w3.org/2000/svg\\" viewBox=\\"0 0 24 24\\"><path d=\\"M2 2h20v20H2z\\" fill=\\"%s\\"/></svg>" }\n' "$GHOSTEX_CREDENTIAL_TOKEN"
	;;
icon-token-tag)
	printf '{ "id": "fixture", "name": "Fixture", "ids": { "format": "number" }, "credentials": [], "icon": "<%s></g>" }\n' "$GHOSTEX_CREDENTIAL_TOKEN"
	;;
bad-group)
	printf '{ "tickets": [ { %s }, { "id": "ACM-13", "title": "Closed one", "state": { "group": "closed", "name": "Closed" } } ] }\n' "$good_ticket"
	;;
error-json)
	printf '{ "error": { "code": "auth", "message": "The token %s was rejected." } }\n' "$GHOSTEX_CREDENTIAL_TOKEN"
	exit 1
	;;
error-stderr)
	echo "starting" >&2
	echo "can't reach the server with $GHOSTEX_CREDENTIAL_TOKEN" >&2
	echo "" >&2
	exit 3
	;;
error-pin)
	printf '{ "error": { "code": "auth", "message": "PIN %s was rejected" } }\n' "$GHOSTEX_CREDENTIAL_PIN"
	exit 1
	;;
error-long)
	# The token starts 10 characters before the message's 300-character cut.
	printf '{ "error": { "code": "other", "message": "%s%s" } }\n' "$(head -c 290 /dev/zero | tr '\0' x)" "$GHOSTEX_CREDENTIAL_TOKEN"
	exit 1
	;;
stderr-straddle)
	# The overlapping credentials `abcXYZ` and `XYZdefgh` on one line longer than the 8 KiB
	# stderr tail, so the tail starts inside them.
	{
		printf 'abcXYZdefgh'
		head -c 8184 /dev/zero | tr '\0' x
		echo
	} >&2
	exit 5
	;;
escaped-stdout)
	printf '{ "tickets": "%s' "$escaped"
	;;
escaped-shape)
	printf '{ "tickets": "%s" }\n' "$escaped"
	;;
escaped-stderr)
	printf 'rejected %s\n' "$escaped" >&2
	exit 6
	;;
error-padded)
	printf '{ "error": { "code": "auth", "message": "%s" } }\n' "$GHOSTEX_CREDENTIAL_TOKEN"
	exit 1
	;;
error-token)
	# The credential without the spaces it was stored with.
	token=$(printf '%s' "$GHOSTEX_CREDENTIAL_TOKEN" | sed 's/^ *//; s/ *$//')
	printf 'rejected %s\n' "$token" >&2
	printf '{ "error": { "code": "auth", "message": "rejected %s" } }\n' "$token"
	exit 1
	;;
flip-secret)
	# `token`, which the accepted info had as not secret, is declared secret by default now, and
	# its value is in the name.
	printf '{ "id": "fixture", "name": "Fixture %s", "ids": { "format": "number" }, "credentials": [ { "name": "token", "label": "API token" }, { "name": "mode", "label": "Mode", "secret": false } ] }\n' "$GHOSTEX_CREDENTIAL_TOKEN"
	;;
flip-secret-invalid)
	# The same, in an answer the contract refuses: it has no `ids`.
	printf '{ "id": "fixture", "name": "Fixture %s", "credentials": [ { "name": "token", "label": "API token" } ] }\n' "$GHOSTEX_CREDENTIAL_TOKEN"
	;;
flip-secret-bad-label)
	# The same, with a declaration whose label the contract refuses.
	printf '{ "id": "fixture", "name": "Fixture %s", "ids": { "format": "number" }, "credentials": [ { "name": "token", "label": "API\\ntoken" } ] }\n' "$GHOSTEX_CREDENTIAL_TOKEN"
	;;
mode-secret-invalid)
	# `mode`, which the accepted info had as not secret, declared secret in an answer the contract
	# refuses: it has no `ids`.
	printf '{ "id": "fixture", "name": "Fixture", "credentials": [ { "name": "mode", "label": "Mode" } ] }\n'
	;;
blank-stderr)
	printf '   \n\n' >&2
	exit 3
	;;
info-padded)
	printf '{ "id": "%s", "name": "X", "ids": { "format": "number" }, "credentials": [] }\n' "$GHOSTEX_CREDENTIAL_TOKEN"
	;;
not-utf8)
	printf '\377\376{'
	;;
stderr-exact)
	# Exactly the 8 KiB stderr tail.
	head -c 8191 /dev/zero | tr '\0' x >&2
	echo >&2
	exit 7
	;;
error-silent)
	exit 4
	;;
invalid-json)
	echo "tickets: none"
	;;
oversized)
	head -c 70000 /dev/zero | tr '\0' ' '
	echo '{}'
	;;
hang)
	sleep 600
	;;
slow)
	# A good answer, a second late.
	sleep 1
	printf '{ "tickets": [ { %s } ] }\n' "$good_ticket"
	;;
not-found)
	echo '{ "tickets": [], "notFound": ["acm-12"] }'
	;;
hourly\ *)
	# `hourly <id> <variant>`: the `good` info as a test pins it under <id>, or a variant of it.
	set -- $mode
	id=$2
	name=Fixture format=key-number keys='"ACM"' extra=''
	case "$3" in
	renamed) name="Fixture Two" ;;
	number) format=number keys='' ;;
	wait)
		# The same answer, once a `release` file sits in the folder it runs in.
		while [ ! -e release ]; do sleep 0.02; done
		;;
	keys-zzq) keys='"ACM", "ZZQ"' ;;
	pin-secret) extra=', { "name": "pin", "label": "PIN" }' ;;
	fail)
		echo '{ "error": { "code": "other", "message": "the tracker is down" } }'
		exit 1
		;;
	esac
	case "$verb" in
	info)
		printf '{ "id": "%s", "name": "%s", "icon": "<svg xmlns=\\"http://www.w3.org/2000/svg\\" viewBox=\\"0 0 24 24\\"><path d=\\"M2 2h20v20H2z\\" fill=\\"currentColor\\"/></svg>", "ids": { "format": "%s", "keys": [%s] }, "credentials": [ { "name": "mode", "label": "Mode", "secret": false }, { "name": "token", "label": "API token", "required": true }%s ], "canCreate": true }\n' "$id" "$name" "$format" "$keys" "$extra"
		;;
	*) printf '{ "tickets": [ { %s } ] }\n' "$good_ticket" ;;
	esac
	;;
wait-release)
	# A good answer once a `release` file sits in the folder it runs in.
	while [ ! -e release ]; do sleep 0.02; done
	case "$verb" in
	detail) printf '%s\n' "$good_detail" ;;
	*) printf '{ "tickets": [ { %s } ] }\n' "$good_ticket" ;;
	esac
	;;
fail-after-release)
	# A failure once a `release` file sits in the folder it runs in.
	while [ ! -e release ]; do sleep 0.02; done
	echo '{ "error": { "code": "other", "message": "the tracker is down" } }'
	exit 1
	;;
detail-token)
	# Details whose description and comment hold the token.
	printf '{ "id": "ACM-12", "title": "Spec the contract", "state": { "group": "progress", "name": "In progress" }, "description": "Use %s here", "comments": [ { "author": "Ana", "body": "token %s", "createdAt": "2026-10-28T08:00:00Z" } ] }\n' "$GHOSTEX_CREDENTIAL_TOKEN" "$GHOSTEX_CREDENTIAL_TOKEN"
	;;
detail-other)
	# Details of another ticket than the one asked for.
	printf '%s\n' "$(printf '%s' "$good_detail" | sed 's/ACM-12/ACM-13/')"
	;;
start-fallback)
	# A ticket with no branch name of its own.
	printf '{ "id": "ACM-12", "title": "Fix the login", "state": { "group": "todo", "name": "To do" } }\n'
	;;
start-untitled)
	# A ticket whose title slugs to nothing.
	printf '{ "id": "ACM-12", "title": "!!!", "state": { "group": "todo", "name": "To do" } }\n'
	;;
start-number)
	# A number source's ticket with no branch name.
	printf '{ "id": "218", "title": "Arabic plan cards", "state": { "group": "todo", "name": "To do" } }\n'
	;;
start-token-branch)
	# A branch name holding the token.
	printf '{ "id": "ACM-12", "title": "Fix the login", "state": { "group": "todo", "name": "To do" }, "branchName": "acm-12-%s" }\n' "$GHOSTEX_CREDENTIAL_TOKEN"
	;;
start-stored)
	# A branch name and a title holding this mode's own value, which the workspace stores.
	printf '{ "id": "ACM-12", "title": "Fix start-stored login", "state": { "group": "todo", "name": "To do" }, "branchName": "acm-12-start-stored" }\n'
	;;
start-flaky)
	# Fails until a `recovered` file sits in the folder it runs in, then answers.
	if [ ! -e recovered ]; then
		echo '{ "error": { "code": "other", "message": "the tracker is down" } }'
		exit 1
	fi
	printf '%s\n' "$good_detail"
	;;
start-not-found)
	echo '{ "error": { "code": "notFound", "message": "no such ticket" } }'
	exit 1
	;;
create-ok)
	# A create answering its id, link and branch.
	printf '{ "id": "ACM-41", "url": "https://tracker.example/ACM-41", "branchName": "dev/acm-41" }\n'
	;;
create-wait)
	# The `create-ok` create, once a `release` file sits in the folder it runs in.
	while [ ! -e release ]; do sleep 0.02; done
	printf '{ "id": "ACM-41", "url": "https://tracker.example/ACM-41", "branchName": "dev/acm-41" }\n'
	;;
create-slow)
	# A create that answers after its time limit, 60 s or a test's shorter one.
	sleep 70
	printf '{ "id": "ACM-41" }\n'
	;;
ACM-4)
	# A mode whose own value, which isn't secret, the id it creates holds.
	printf '{ "id": "ACM-41" }\n'
	;;
spec)
	# A mode whose own value, which isn't secret, the branch it creates holds.
	printf '{ "id": "ACM-41", "branchName": "dev/acm-41-spec" }\n'
	;;
create-unreadable)
	# A create that ran and answered something that isn't its answer.
	echo 'created, probably'
	;;
create-too-large)
	# A create that ran and wrote more than a create's 64 KiB cap.
	head -c 70000 /dev/zero | tr '\0' 'a'
	;;
create-exited)
	# A create that ran and exited without an error object.
	echo 'lost the connection after posting' >&2
	exit 3
	;;
create-reported)
	# A create that answered an error object: it says nothing was made.
	echo '{ "error": { "code": "other", "message": "the project is archived" } }'
	exit 1
	;;
create-token)
	# A create whose link and branch hold the token.
	printf '{ "id": "ACM-41", "url": "https://tracker.example/%s", "branchName": "acm-41-%s" }\n' "$GHOSTEX_CREDENTIAL_TOKEN" "$GHOSTEX_CREDENTIAL_TOKEN"
	;;
pr-only)
	# A ticket that names its PR, but neither a project nor a repo.
	printf '{ "tickets": [ { "id": "ACM-12", "title": "Spec the contract", "state": { "group": "progress", "name": "In progress" }, "pullRequestUrls": ["https://github.com/acme/app/pull/7"] } ] }\n'
	;;
title-kept)
	# A title holding a value this source was never given.
	printf '{ "tickets": [ { "id": "ACM-12", "title": "Renewal for customer-secret", "state": { "group": "todo", "name": "To do customer-secret" } } ] }\n'
	;;
answer-held-open)
	# A whole answer, then it exits while a child it left behind keeps stdout open.
	echo '{ "tickets": [], "notFound": ["acm-9"] }'
	sleep 5 &
	;;
late-stdin)
	# More than a pipe holds, before it reads a byte of its input.
	head -c 300000 /dev/zero | tr '\0' ' '
	cat >stdin.txt
	echo '{ "tickets": [], "notFound": ["acm-9"] }'
	;;
esac
