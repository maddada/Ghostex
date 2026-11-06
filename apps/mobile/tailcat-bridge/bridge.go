// Package tailcatbridge is the gomobile-bound glue between the Ghostex mobile
// app and the tailcat library. It runs tailcat clients in-process and exposes
// each paired machine as a loopback TCP listener, so the platform SSH stacks
// (SSHJ on Android, libssh2 on iOS) dial 127.0.0.1:<port> exactly as they
// would dial a real host, and PTY bytes never touch the JS bridge.
//
// gomobile restricts exported signatures to basic types, so the API is
// (string, int, error) shaped by design.
//
// Everything this package logs goes through the standard library logger, which
// gomobile routes to logcat (tag "GoLog") on Android and to the unified log on
// iOS. Those lines are the only on-device view of the tunnel, so every line is
// prefixed with "tailcat[<id>]: " and never contains the pairing token, which
// is a secret.
package tailcatbridge

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net"
	"net/netip"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/tailscale/tailcat"
	"tailscale.com/types/key"
)

const (
	// reachabilityTimeout bounds the cold rendezvous a fresh client performs on
	// its first use: resolving the DERP map (a network fetch unless the token
	// embeds the relay details), connecting to the relay, and completing the
	// meow handshake with the peer.
	reachabilityTimeout = 30 * time.Second

	// reuseProbeTimeout bounds the disco ping that checks a tunnel being reused
	// still answers. A healthy tunnel answers in one relay round-trip; magicsock
	// itself gives up on a disco ping after 5s and never reports the timeout,
	// so this context is the only thing that ends the wait.
	reuseProbeTimeout = 4 * time.Second

	// dialTimeout bounds one accepted connection's dial to the peer.
	dialTimeout = 30 * time.Second

	// retiredListenerGrace keeps a replaced loopback listener accepting long
	// enough for a concurrent StartForward caller that was handed its port to
	// dial it. Every caller dials right after StartForward returns.
	retiredListenerGrace = 30 * time.Second
)

type forward struct {
	id         string
	token      string
	remotePort int
	listener   net.Listener
	client     *tailcat.Client
	closed     chan struct{}
	ctx        context.Context
	cancel     context.CancelFunc

	// lastDialErr is the most recent per-connection dial failure, cleared on the
	// next successful dial. It has its own mutex so a dial never waits behind a
	// teardown holding the package lock across the tunnel's Close.
	errMu       sync.Mutex
	lastDialErr string
	// path is what the last disco ping said about the tunnel (see linkPath).
	path linkPath
}

// linkPath is how a tunnel reaches its computer: straight there, or through the
// relay its address names.
type linkPath struct {
	// Path is "direct" or "relay"; empty before the first check.
	Path string `json:"path"`
	// Route is "lan" or "internet" for a direct path.
	Route string `json:"route,omitempty"`
	// Relay is the relay the address meets on (a hostname, or "region N").
	Relay     string `json:"relay"`
	LatencyMs int    `json:"latencyMs,omitempty"`
}

func (f *forward) notePath(p linkPath) {
	f.errMu.Lock()
	defer f.errMu.Unlock()
	f.path = p
}

func (f *forward) currentPath() linkPath {
	f.errMu.Lock()
	defer f.errMu.Unlock()
	return f.path
}

func (f *forward) noteDialErr(message string) {
	f.errMu.Lock()
	defer f.errMu.Unlock()
	f.lastDialErr = message
}

func (f *forward) lastError() string {
	f.errMu.Lock()
	defer f.errMu.Unlock()
	return f.lastDialErr
}

var (
	mu       sync.Mutex
	forwards = map[string]*forward{}

	// clientKey is this phone's Easy Connect identity once UseClientKeyFile
	// loaded it; while zero, every tunnel makes a throwaway key. Guarded by mu.
	clientKey key.NodePrivate
)

// UseClientKeyFile loads this phone's Easy Connect client key from path,
// creating it on first use, makes every later tunnel use it, and returns its
// public key ("nodekey:…").
//
// CDXC:RemotePairing 2026-10-25 DECISION:
// User (security audit, Q4 "per-device tokens and a tunnel allow-list"): the
// phone keeps one client key for good and sends its public half when it pairs,
// so the computer can list it and, with "Only paired devices can connect" on,
// refuse every key it has not listed, including a removed phone's. Until then
// each tunnel made a throwaway key, which no allow-list could ever name. The
// key file lives in the app's private storage; a reinstalled app makes a new
// key and must pair again, as it must for its SSH key.
func UseClientKeyFile(path string) (string, error) {
	mu.Lock()
	defer mu.Unlock()
	if !clientKey.IsZero() {
		return clientKey.Public().String(), nil
	}
	loaded, err := readOrCreateClientKey(path)
	if err != nil {
		return "", err
	}
	clientKey = loaded
	log.Printf("tailcat: client key loaded")
	return clientKey.Public().String(), nil
}

func readOrCreateClientKey(path string) (key.NodePrivate, error) {
	var loaded key.NodePrivate
	text, err := os.ReadFile(path)
	if err == nil {
		if err := loaded.UnmarshalText([]byte(strings.TrimSpace(string(text)))); err != nil {
			return key.NodePrivate{}, fmt.Errorf("tailcat client key file %s is unreadable: %w", path, err)
		}
		return loaded, nil
	}
	if !errors.Is(err, os.ErrNotExist) {
		return key.NodePrivate{}, err
	}
	fresh := key.NewNode()
	encoded, err := fresh.MarshalText()
	if err != nil {
		return key.NodePrivate{}, err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return key.NodePrivate{}, err
	}
	partial := path + ".partial"
	if err := os.WriteFile(partial, encoded, 0o600); err != nil {
		return key.NodePrivate{}, err
	}
	if err := os.Rename(partial, path); err != nil {
		return key.NodePrivate{}, err
	}
	return fresh, nil
}

// StartForward ensures a loopback listener that forwards every accepted
// connection to remotePort on the tailcat peer identified by token, and
// returns the listener's local port. id keys the forward (one per machine):
// calling again with the same id, token, and remotePort reuses the existing
// tunnel when it still answers, so reconnects skip the rendezvous.
// A changed token or remotePort for the same id replaces the forward.
//
// Before returning, the peer is checked so an unreachable machine, a stale
// token, a failed DERP map fetch, or a tunnel that died while the app was
// suspended surfaces here as a real error (or a rebuilt tunnel) instead of
// silently resetting the SSH client's TCP connection later.
func StartForward(id, token string, remotePort int) (int, error) {
	token = strings.TrimSpace(token)
	if token == "" {
		return 0, errors.New("tailcat token is empty")
	}
	if remotePort < 1 || remotePort > 65535 {
		return 0, fmt.Errorf("remote port %d is out of range", remotePort)
	}
	log.Printf("tailcat[%s]: StartForward remotePort=%d tokenLen=%d", id, remotePort, len(token))

	for attempt := 0; ; attempt++ {
		f, port, reused, err := ensureForward(id, token, remotePort)
		if err != nil {
			log.Printf("tailcat[%s]: StartForward could not open a loopback listener: %v", id, err)
			return 0, err
		}
		if !reused {
			if err := rendezvous(f); err != nil {
				stopForwardIf(id, f)
				return 0, err
			}
			log.Printf("tailcat[%s]: forwarding 127.0.0.1:%d -> peer:%d", id, port, remotePort)
			go reportLinkPath(f)
			return port, nil
		}
		if err := probeReused(f); err == nil {
			log.Printf("tailcat[%s]: reusing tunnel, forwarding 127.0.0.1:%d -> peer:%d", id, port, remotePort)
			return port, nil
		} else if attempt == 0 {
			log.Printf("tailcat[%s]: rebuilding the tunnel: %v", id, err)
			stopForwardIf(id, f)
			continue
		} else {
			// The forward another caller built while this one was probing is dead too.
			stopForwardIf(id, f)
			return 0, fmt.Errorf("tailcat: cannot reach paired machine: %w", err)
		}
	}
}

// rendezvous performs a fresh client's cold start: the DERP map fetch, the
// relay connection, and the meow handshake that registers it with the peer.
// It runs outside the registry lock because it blocks for seconds.
func rendezvous(f *forward) error {
	start := time.Now()
	ctx, cancel := context.WithTimeout(f.ctx, reachabilityTimeout)
	defer cancel()
	if _, err := f.client.Ping(ctx); err != nil {
		log.Printf("tailcat[%s]: peer unreachable after %v: %v", f.id, elapsed(start), err)
		return fmt.Errorf("tailcat: cannot reach paired machine: %w", err)
	}
	log.Printf("tailcat[%s]: peer reachable in %v", f.id, elapsed(start))
	return nil
}

// probeReused checks that a tunnel built earlier still carries traffic to the
// peer, and that the peer still knows this client.
//
// CDXC:RemotePairing 2026-09-22 WHY:
// A reused client must be probed with a disco ping, not Client.Ping: the
// meow handshake completes once per client and every later Ping returns at
// once without touching the network, so the reachability check the first
// version ran here always passed on a dead tunnel. That is how the iPhone
// lost Easy Connect after leaving the app: iOS suspends the process, the
// relay connection and any direct path die, and every reconnect reused the
// same client, whose dials then hung until the app was killed. A disco ping
// needs a live path and a pong from the paired machine, so it also fails when
// the machine's tailcat helper restarted and forgot this client; either way
// the caller rebuilds the tunnel and a fresh client registers again.
func probeReused(f *forward) error {
	start := time.Now()
	ctx, cancel := context.WithTimeout(f.ctx, reuseProbeTimeout)
	defer cancel()
	res, err := f.client.DiscoPing(ctx)
	if err != nil {
		return fmt.Errorf("reused tunnel did not answer within %v: %w", elapsed(start), err)
	}
	p := pathOf(f.token, res.Endpoint, res.LatencySeconds)
	f.notePath(p)
	log.Printf("tailcat[%s]: reused tunnel answered in %v, link %s", f.id, elapsed(start), p.describe())
	return nil
}

// pathOf names the path a disco ping answered over. The endpoint is never kept
// or logged: only whether it is on the LAN or the internet.
func pathOf(token, endpoint string, latencySeconds float64) linkPath {
	p := linkPath{Path: "relay", Relay: relayName(token), LatencyMs: int(latencySeconds * 1000)}
	if endpoint == "" {
		return p
	}
	p.Path = "direct"
	p.Route = "internet"
	if ap, err := netip.ParseAddrPort(endpoint); err == nil {
		addr := ap.Addr()
		if addr.IsPrivate() || addr.IsLinkLocalUnicast() || addr.IsLoopback() {
			p.Route = "lan"
		}
	}
	return p
}

func (p linkPath) describe() string {
	if p.Path == "direct" {
		return fmt.Sprintf("direct (%s), relay %s", p.Route, p.Relay)
	}
	return "via relay " + p.Relay
}

// relayName is the relay an address meets on: its first embedded relay node's
// hostname, or the public region's number.
func relayName(token string) string {
	ci, err := tailcat.ParseAddr(tailcat.Addr(token))
	if err != nil {
		return "unknown"
	}
	for _, region := range ci.Region {
		if region == nil {
			continue
		}
		for _, node := range region.Nodes {
			if node != nil && node.HostName != "" {
				return node.HostName
			}
		}
	}
	if ci.RegionID != 0 {
		return fmt.Sprintf("region %d", ci.RegionID)
	}
	return "unknown"
}

// pathChecks spaces the checks after a fresh rendezvous: a direct path usually
// wins within a few seconds of the first traffic, or never.
var pathChecks = []time.Duration{3 * time.Second, 5 * time.Second, 10 * time.Second}

// reportLinkPath checks a fresh tunnel's path a few times and logs the outcome
// once, so every Easy Connect link leaves one "link direct" or "link stayed on
// the relay" line in the device log.
//
// CDXC:RemotePairing 2026-11-06 DECISION:
// User (Q1491, "build and measure together"): the phone records whether each
// Easy Connect link went direct or stayed relayed, and which relay, beside
// gxserver's own count (server/src/tailcat/link_log.rs), and shows it on the
// connection details as "Direct" or "Via relay". It ships with the next store
// build: the over-the-air bundle only reads it when the app has ForwardPath.
func reportLinkPath(f *forward) {
	var p linkPath
	for _, wait := range pathChecks {
		select {
		case <-f.closed:
			return
		case <-time.After(wait):
		}
		ctx, cancel := context.WithTimeout(f.ctx, reuseProbeTimeout)
		res, err := f.client.DiscoPing(ctx)
		cancel()
		if err != nil {
			continue
		}
		p = pathOf(f.token, res.Endpoint, res.LatencySeconds)
		f.notePath(p)
		if p.Path == "direct" {
			break
		}
	}
	if p.Path == "" {
		log.Printf("tailcat[%s]: link path unknown: no disco ping answered", f.id)
		return
	}
	if p.Path == "direct" {
		log.Printf("tailcat[%s]: link %s", f.id, p.describe())
	} else {
		log.Printf("tailcat[%s]: link stayed on the relay %s", f.id, p.Relay)
	}
}

// ForwardPath returns the machine's tunnel path as JSON ({"path":"direct"|"relay",
// "route":"lan"|"internet","relay":"<host>","latencyMs":N}), or the empty string
// when no forward exists or it has not been checked yet.
func ForwardPath(id string) string {
	mu.Lock()
	f := forwards[id]
	mu.Unlock()
	if f == nil {
		return ""
	}
	p := f.currentPath()
	if p.Path == "" {
		return ""
	}
	encoded, err := json.Marshal(p)
	if err != nil {
		return ""
	}
	return string(encoded)
}

// StopForward closes the machine's listener, its tunnel, and every in-flight
// connection.
func StopForward(id string) {
	mu.Lock()
	defer mu.Unlock()
	if _, ok := forwards[id]; ok {
		log.Printf("tailcat[%s]: StopForward", id)
	}
	stopLocked(id)
}

// StopAll tears down every forward. The app calls this when connectivity is
// globally reset.
func StopAll() {
	mu.Lock()
	defer mu.Unlock()
	log.Printf("tailcat: StopAll (%d forwards)", len(forwards))
	for id := range forwards {
		stopLocked(id)
	}
}

// LastError returns the most recent dial failure recorded for the machine's
// forward, or the empty string when the last dial succeeded, no connection has
// been dialed yet, or no forward exists for id. The platform SSH layers read it
// after a transport failure so the user sees the tunnel's own error instead of
// the bare TCP reset the closed loopback connection produces.
func LastError(id string) string {
	mu.Lock()
	f := forwards[id]
	mu.Unlock()
	if f == nil {
		return ""
	}
	return f.lastError()
}

// Ping establishes (or reuses) a tunnel to the peer and returns the relay
// round-trip latency in milliseconds. It is the "Test connection" primitive:
// it proves the token is valid and the peer is reachable without needing SSH.
func Ping(token string, timeoutMs int) (int, error) {
	token = strings.TrimSpace(token)
	if token == "" {
		return 0, errors.New("tailcat token is empty")
	}
	if timeoutMs <= 0 {
		timeoutMs = 15000
	}
	mu.Lock()
	client := &tailcat.Client{Server: tailcat.Addr(token), Key: clientKey}
	mu.Unlock()
	defer client.Close()
	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(timeoutMs)*time.Millisecond)
	defer cancel()
	result, err := client.Ping(ctx)
	if err != nil {
		return 0, err
	}
	return int(result.Latency / time.Millisecond), nil
}

// ensureForward returns the live forward for id (creating it, or replacing one
// whose token or remote port changed) together with its local port. reused
// reports whether the forward existed before this call, so the caller knows
// whether it needs the cold rendezvous or the reuse probe.
//
// CDXC:RemotePairing 2026-10-20 WHY:
// A reused forward gets a fresh loopback listener on every call, and the replaced one keeps accepting for retiredListenerGrace so a concurrent caller handed its port can still dial it. iOS reclaims a suspended app's idle listening sockets (developer.apple.com/forums/thread/85038: clients then get ECONNREFUSED) while the tunnel comes back and answers the reuse probe, so handing out the old port made every reconnect after a long stay in the background fail with "SSH refused the connection" until the app was killed. A reclaimed listener cannot be detected instead: Go keeps it non-blocking, and XNU answers accept on a defunct non-blocking listener with EWOULDBLOCK and never fires its kqueue read filter, so the accept loop stays parked (and Close still returns at once). Only the tunnel's rendezvous is worth reusing.
func ensureForward(id, token string, remotePort int) (f *forward, port int, reused bool, err error) {
	mu.Lock()
	defer mu.Unlock()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return nil, 0, false, err
	}
	port = listener.Addr().(*net.TCPAddr).Port
	if existing, ok := forwards[id]; ok {
		if existing.token == token && existing.remotePort == remotePort {
			old := existing.listener
			existing.listener = listener
			go acceptLoop(existing, listener)
			time.AfterFunc(retiredListenerGrace, func() { _ = old.Close() })
			return existing, port, true, nil
		}
		log.Printf("tailcat[%s]: token or remote port changed, replacing forward", id)
		stopLocked(id)
	}
	ctx, cancel := context.WithCancel(context.Background())
	f = &forward{
		id:         id,
		token:      token,
		remotePort: remotePort,
		listener:   listener,
		// CDXC:RemotePairing 2026-09-15 WHY:
		// The tailcat library must be newer than v0.5.0: gxserver installs the tailcat v0.6.0 helper, which serves addresses carrying a WireGuard pre-shared key, and an older client still passes the reachability ping but never completes the handshake, so every dial hangs and the phone reports "Reached the computer, but Ghostex there did not answer".
		// SEE-ALSO: server/src/tailcat/install.rs (TAILCAT_MODULE) and the go.mod pin next to this file.
		client: &tailcat.Client{
			Server: tailcat.Addr(token),
			Key:    clientKey,
			Logf:   forwardLogf(id),
		},
		closed: make(chan struct{}),
		ctx:    ctx,
		cancel: cancel,
	}
	forwards[id] = f
	go acceptLoop(f, listener)
	return f, port, false, nil
}

// forwardLogf attributes the tailcat library's own log lines to one machine.
func forwardLogf(id string) func(format string, args ...any) {
	prefix := "tailcat[" + id + "]: "
	return func(format string, args ...any) {
		log.Printf(prefix+format, args...)
	}
}

// stopForwardIf tears down id's forward only when it is still f, so a failed
// validation cannot close a forward another caller replaced in the meantime.
func stopForwardIf(id string, f *forward) {
	mu.Lock()
	defer mu.Unlock()
	if forwards[id] == f {
		stopLocked(id)
	}
}

func stopLocked(id string) {
	f, ok := forwards[id]
	if !ok {
		return
	}
	delete(forwards, id)
	f.cancel()
	close(f.closed)
	_ = f.listener.Close()
	// CDXC:RemotePairing 2026-09-14 WHY:
	// A dead tunnel's WireGuard teardown must not hold the registry lock and block reconnects to every computer.
	go f.client.Close()
}

func elapsed(start time.Time) time.Duration {
	return time.Since(start).Round(time.Millisecond)
}

func acceptLoop(f *forward, listener net.Listener) {
	for {
		conn, err := listener.Accept()
		if err != nil {
			return
		}
		go serveConn(f, conn)
	}
}

func serveConn(f *forward, local net.Conn) {
	// The first dial on a fresh client performs DERP rendezvous and can take
	// several seconds; later dials reuse the established tunnel and are fast.
	start := time.Now()
	ctx, cancel := context.WithTimeout(f.ctx, dialTimeout)
	defer cancel()
	remote, err := f.client.DialTCPPort(ctx, uint16(f.remotePort))
	if err != nil {
		// Closing the accepted connection is all the SSH client can observe, so
		// the real cause must be logged and kept for LastError; otherwise the
		// user only ever sees "Connection reset".
		detail := fmt.Sprintf("dial peer port %d failed after %v: %v", f.remotePort, elapsed(start), err)
		log.Printf("tailcat[%s]: %s", f.id, detail)
		f.noteDialErr(detail)
		_ = local.Close()
		return
	}
	log.Printf("tailcat[%s]: dialed peer port %d in %v", f.id, f.remotePort, elapsed(start))
	f.noteDialErr("")
	// Tear both ends down when the forward stops so no pump outlives StopForward.
	done := make(chan struct{})
	defer close(done)
	go func() {
		select {
		case <-f.closed:
			_ = local.Close()
			_ = remote.Close()
		case <-done:
		}
	}()
	tailcat.ProxyConns(local, remote)
}
