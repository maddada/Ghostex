package tailcatbridge

import (
	"context"
	"errors"
	"net"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/tailscale/tailcat"
	"tailscale.com/ipn/ipnstate"
	"tailscale.com/types/key"
)

// fakeTunnel stands in for a tailcat client: each hello answers with the next
// of helloErrs (nil once they run out), after gate opens when there is one.
type fakeTunnel struct {
	mu         sync.Mutex
	helloErrs  []error
	gate       chan struct{}
	hellos     int
	discoPings int
	closed     bool
}

func (t *fakeTunnel) Ping(ctx context.Context) (tailcat.PingResult, error) {
	t.mu.Lock()
	t.hellos++
	var err error
	if t.hellos <= len(t.helloErrs) {
		err = t.helloErrs[t.hellos-1]
	}
	gate := t.gate
	t.mu.Unlock()
	if gate != nil {
		select {
		case <-gate:
		case <-ctx.Done():
			return tailcat.PingResult{}, ctx.Err()
		}
	}
	return tailcat.PingResult{}, err
}

func (t *fakeTunnel) DiscoPing(context.Context) (*ipnstate.PingResult, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.discoPings++
	return &ipnstate.PingResult{}, nil
}

func (t *fakeTunnel) DialTCPPort(context.Context, uint16) (net.Conn, error) {
	return nil, errors.New("tests do not dial")
}

func (t *fakeTunnel) Close() error {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.closed = true
	return nil
}

func (t *fakeTunnel) counts() (hellos, discoPings int, closed bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.hellos, t.discoPings, t.closed
}

// fakeTunnels makes every new tunnel a fakeTunnel set up by prepare, and
// records them by address.
func fakeTunnels(t *testing.T, prepare func(token string, tun *fakeTunnel)) func(token string) []*fakeTunnel {
	var mu sync.Mutex
	made := map[string][]*fakeTunnel{}
	previous := newTunnel
	newTunnel = func(id, token string, _ key.NodePrivate) tunnel {
		tun := &fakeTunnel{}
		if prepare != nil {
			prepare(token, tun)
		}
		mu.Lock()
		made[token] = append(made[token], tun)
		mu.Unlock()
		return tun
	}
	t.Cleanup(func() {
		StopAll()
		newTunnel = previous
	})
	return func(token string) []*fakeTunnel {
		mu.Lock()
		defer mu.Unlock()
		return append([]*fakeTunnel(nil), made[token]...)
	}
}

func eventually(t *testing.T, what string, done func() bool) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for !done() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(5 * time.Millisecond)
	}
}

type started struct {
	port int
	err  error
}

func startAsync(id, token string) chan started {
	out := make(chan started, 1)
	go func() {
		port, err := StartForward(id, token, 22)
		out <- started{port, err}
	}()
	return out
}

func TestASecondCallerWaitsForTheTunnelStillConnecting(t *testing.T) {
	gate := make(chan struct{})
	made := fakeTunnels(t, func(_ string, tun *fakeTunnel) { tun.gate = gate })

	first := startAsync("m", "tcRelay")
	eventually(t, "the first hello", func() bool { return len(made("tcRelay")) == 1 && hellosOf(made("tcRelay")[0]) == 1 })
	second := startAsync("m", "tcRelay")
	time.Sleep(50 * time.Millisecond)
	tun := made("tcRelay")[0]
	if _, discoPings, closed := tun.counts(); discoPings != 0 || closed {
		t.Fatalf("the second caller probed (%d) or closed (%v) a tunnel still connecting", discoPings, closed)
	}
	close(gate)
	for _, result := range []started{<-first, <-second} {
		if result.err != nil || result.port == 0 {
			t.Fatalf("a caller failed: %+v", result)
		}
	}
	if n := len(made("tcRelay")); n != 1 {
		t.Fatalf("built %d tunnels for one address, want 1", n)
	}
}

func hellosOf(tun *fakeTunnel) int {
	hellos, _, _ := tun.counts()
	return hellos
}

func TestAFailedDialKeepsTheTunnelInUseForTheFallback(t *testing.T) {
	made := fakeTunnels(t, func(token string, tun *fakeTunnel) {
		if token == "tcRelay" {
			tun.helloErrs = []error{errors.New("relay refused")}
		}
	})
	if _, err := StartForward("m", "tcPublic", 22); err != nil {
		t.Fatalf("public dial: %v", err)
	}
	if _, err := StartForward("m", "tcRelay", 22); err == nil {
		t.Fatal("relay dial succeeded, want its error")
	}
	public := made("tcPublic")[0]
	if _, _, closed := public.counts(); closed {
		t.Fatal("dialing the relay closed the public tunnel in use")
	}
	if _, err := StartForward("m", "tcPublic", 22); err != nil {
		t.Fatalf("fallback to the public tunnel: %v", err)
	}
	if n := len(made("tcPublic")); n != 1 {
		t.Fatalf("the fallback built %d public tunnels, want it to reuse the one in use", n)
	}
	if _, discoPings, _ := public.counts(); discoPings != 1 {
		t.Fatalf("the reused tunnel was probed %d times, want 1", discoPings)
	}
}

func TestConnectingAnotherAddressClosesTheTunnelUsedBefore(t *testing.T) {
	made := fakeTunnels(t, nil)
	if _, err := StartForward("m", "tcPublic", 22); err != nil {
		t.Fatalf("public dial: %v", err)
	}
	if _, err := StartForward("m", "tcRelay", 22); err != nil {
		t.Fatalf("relay dial: %v", err)
	}
	// Teardown runs off the registry lock, so the close lands a moment later.
	eventually(t, "the public tunnel to close", func() bool {
		_, _, closed := made("tcPublic")[0].counts()
		return closed
	})
	mu.Lock()
	inUse := current["m"]
	mu.Unlock()
	if inUse == nil || inUse.token != "tcRelay" {
		t.Fatalf("tunnel in use: %+v, want the relay one", inUse)
	}
}

func TestASlowHelloGetsASecondWindow(t *testing.T) {
	previous := helloRetryAfter
	helloRetryAfter = 0
	t.Cleanup(func() { helloRetryAfter = previous })
	made := fakeTunnels(t, func(_ string, tun *fakeTunnel) {
		tun.helloErrs = []error{context.DeadlineExceeded}
	})
	if _, err := StartForward("m", "tcRelay", 22); err != nil {
		t.Fatalf("a hello answered the second time still failed: %v", err)
	}
	if hellos := hellosOf(made("tcRelay")[0]); hellos != 2 {
		t.Fatalf("said hello %d times, want 2", hellos)
	}
}

func TestAHelloThatRunsOutAtOnceIsNotRepeated(t *testing.T) {
	made := fakeTunnels(t, func(_ string, tun *fakeTunnel) {
		tun.helloErrs = []error{context.DeadlineExceeded, context.DeadlineExceeded}
	})
	if _, err := StartForward("m", "tcRelay", 22); err == nil {
		t.Fatal("want the hello's error")
	}
	if hellos := hellosOf(made("tcRelay")[0]); hellos != 1 {
		t.Fatalf("said hello %d times, want 1", hellos)
	}
}

func TestStoppingATunnelWhileItConnectsSaysSo(t *testing.T) {
	gate := make(chan struct{})
	t.Cleanup(func() { close(gate) })
	made := fakeTunnels(t, func(_ string, tun *fakeTunnel) { tun.gate = gate })
	result := startAsync("m", "tcRelay")
	eventually(t, "the hello", func() bool { return len(made("tcRelay")) == 1 && hellosOf(made("tcRelay")[0]) == 1 })
	StopForward("m")
	got := <-result
	if got.err == nil || !strings.Contains(got.err.Error(), "stopped while it was connecting") {
		t.Fatalf("got %+v, want the stopped-while-connecting error", got)
	}
}
