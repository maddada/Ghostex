#!/bin/sh
# A command ticket source for the runner's tests. Its credential field `mode`, or a `mode.txt`
# in the folder it runs in when it has no credentials, picks how it answers. It records its argv,
# environment and input in that folder.
verb="$1"
mode="${GHOSTEX_CREDENTIAL_MODE:-$(cat mode.txt 2>/dev/null)}"
printf '%s\n' "$@" >argv.txt
env >env.txt
if [ "$verb" != info ] && [ "$mode" != late-stdin ]; then
	cat >stdin.txt
fi

good_ticket='"id": "ACM-12", "title": "Spec the contract", "url": "https://tracker.example/ACM-12",
  "state": { "group": "progress", "name": "In progress", "type": "active" },
  "updatedAt": "2026-10-28T09:12:00Z",
  "assignee": { "name": "Leandro", "avatarUrl": "https://tracker.example/a.png", "isMe": true },
  "labels": ["server"], "branchName": "leandro/acm-12-spec",
  "pullRequestUrls": ["https://github.com/acme/app/pull/7"], "team": "ACM", "repo": "acme/app"'

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
	detail) printf '{ %s, "description": "Body", "author": "Ana", "comments": [ { "author": "Ana", "body": "Hi", "createdAt": "2026-10-28T08:00:00Z" } ], "secretField": "x" }\n' "$good_ticket" ;;
	create) printf '{ "id": "ACM-41", "url": "https://tracker.example/ACM-41", "branchName": "leandro/acm-41" }\n' ;;
	esac
	;;
bad-icon)
	cat <<'EOF'
{ "id": "fixture", "name": "Fixture", "ids": { "format": "number" }, "credentials": [],
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
