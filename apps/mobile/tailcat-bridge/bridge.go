// Package tailcatbridge is the gomobile-bound glue between the Ghostex mobile
// app and the tailcat library. It runs tailcat clients in-process and exposes
// each paired machine as a loopback TCP listener, so the platform SSH stacks
// (SSHJ on Android, libssh2 on iOS) dial 127.0.0.1:<port> exactly as they
// would dial a real host, and PTY bytes never touch the JS bridge.
//
// gomobile restricts exported signatures to basic types, so the API is
// (string, int, error) shaped by design.
package tailcatbridge

import (
	"context"
	"errors"
	"fmt"
	"net"
	"strings"
	"sync"
	"time"

	"github.com/tailscale/tailcat"
)

type forward struct {
	token      string
	remotePort int
	listener   net.Listener
	client     *tailcat.Client
	closed     chan struct{}
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
func StartForward(id, token string, remotePort int) (int, error) {
	token = strings.TrimSpace(token)
	if token == "" {
		return 0, errors.New("tailcat token is empty")
	}
	if remotePort < 1 || remotePort > 65535 {
		return 0, fmt.Errorf("remote port %d is out of range", remotePort)
	}
	mu.Lock()
	defer mu.Unlock()
	if existing, ok := forwards[id]; ok {
		if existing.token == token && existing.remotePort == remotePort {
			return existing.listener.Addr().(*net.TCPAddr).Port, nil
		}
		stopLocked(id)
	}
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return 0, err
	}
	f := &forward{
		token:      token,
		remotePort: remotePort,
		listener:   listener,
		client:     tailcat.NewClient(tailcat.ConnBlob(token)),
		closed:     make(chan struct{}),
	}
	forwards[id] = f
	go acceptLoop(f)
	return listener.Addr().(*net.TCPAddr).Port, nil
}

// StopForward closes the machine's listener, its tunnel, and every in-flight
// connection.
func StopForward(id string) {
	mu.Lock()
	defer mu.Unlock()
	stopLocked(id)
}

// StopAll tears down every forward. The app calls this when connectivity is
// globally reset.
func StopAll() {
	mu.Lock()
	defer mu.Unlock()
	for id := range forwards {
		stopLocked(id)
	}
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
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	remote, err := f.client.DialTCPPort(ctx, uint16(f.remotePort))
	if err != nil {
		_ = local.Close()
		return
	}
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
