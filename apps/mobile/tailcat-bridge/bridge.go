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
	"errors"
	"fmt"
	"log"
	"net"
	"strings"
	"sync"
	"time"

	"github.com/tailscale/tailcat"
)

const (
	// reachabilityTimeout bounds the cold rendezvous a fresh client performs on
	// its first use: resolving the DERP map (a network fetch unless the token
	// embeds the relay details), connecting to the relay, and completing the
	// meow handshake with the peer.
	reachabilityTimeout = 30 * time.Second

	// dialTimeout bounds one accepted connection's dial to the peer.
	dialTimeout = 30 * time.Second
)

type forward struct {
	id         string
	token      string
	remotePort int
	listener   net.Listener
	client     *tailcat.Client
	closed     chan struct{}

	// lastDialErr is the most recent per-connection dial failure, cleared on the
	// next successful dial. It has its own mutex so a dial never waits behind a
	// teardown holding the package lock across the tunnel's Close.
	errMu       sync.Mutex
	lastDialErr string
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
)

// StartForward ensures a loopback listener that forwards every accepted
// connection to remotePort on the tailcat peer identified by token, and
// returns the listener's local port. id keys the forward (one per machine):
// calling again with the same id, token, and remotePort returns the existing
// listener's port, so reconnects reuse the already-established tunnel.
// A changed token or remotePort for the same id replaces the forward.
//
// Before returning, the peer is pinged so an unreachable machine, a stale
// token, or a failed DERP map fetch surfaces here as a real error instead of
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

	f, port, err := ensureForward(id, token, remotePort)
	if err != nil {
		log.Printf("tailcat[%s]: StartForward could not open a loopback listener: %v", id, err)
		return 0, err
	}

	// Ping outside the lock: a cold rendezvous blocks for seconds, and a warm
	// one is a single relay round-trip, so validating on every call is cheap
	// and keeps "Test connection" honest.
	start := time.Now()
	ctx, cancel := context.WithTimeout(context.Background(), reachabilityTimeout)
	defer cancel()
	if _, err := f.client.Ping(ctx); err != nil {
		log.Printf("tailcat[%s]: peer unreachable after %v: %v", id, elapsed(start), err)
		stopForwardIf(id, f)
		return 0, fmt.Errorf("tailcat: cannot reach paired machine: %w", err)
	}
	log.Printf("tailcat[%s]: peer reachable in %v, forwarding 127.0.0.1:%d -> peer:%d",
		id, elapsed(start), port, remotePort)
	return port, nil
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
	client := tailcat.NewClient(tailcat.ConnBlob(token))
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
// whose token or remote port changed) together with its local port.
func ensureForward(id, token string, remotePort int) (*forward, int, error) {
	mu.Lock()
	defer mu.Unlock()
	if existing, ok := forwards[id]; ok {
		if existing.token == token && existing.remotePort == remotePort {
			return existing, existing.listener.Addr().(*net.TCPAddr).Port, nil
		}
		log.Printf("tailcat[%s]: token or remote port changed, replacing forward", id)
		stopLocked(id)
	}
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return nil, 0, err
	}
	f := &forward{
		id:         id,
		token:      token,
		remotePort: remotePort,
		listener:   listener,
		client: &tailcat.Client{
			Server: tailcat.ConnBlob(token),
			Logf:   forwardLogf(id),
		},
		closed: make(chan struct{}),
	}
	forwards[id] = f
	go acceptLoop(f)
	return f, listener.Addr().(*net.TCPAddr).Port, nil
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
	close(f.closed)
	_ = f.listener.Close()
	_ = f.client.Close()
}

func elapsed(start time.Time) time.Duration {
	return time.Since(start).Round(time.Millisecond)
}

func acceptLoop(f *forward) {
	for {
		conn, err := f.listener.Accept()
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
	ctx, cancel := context.WithTimeout(context.Background(), dialTimeout)
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
