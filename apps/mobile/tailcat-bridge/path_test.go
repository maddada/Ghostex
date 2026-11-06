package tailcatbridge

import (
	"testing"

	"github.com/tailscale/tailcat"
	"tailscale.com/tailcfg"
)

func addressOn(host string, regionID tailcfg.DERPRegionID) string {
	priv := tailcat.NewPrivateKey()
	if host != "" {
		priv.Public.Region = []*tailcfg.DERPRegion{{Nodes: []*tailcfg.DERPNode{{HostName: host}}}}
	} else {
		priv.Public.RegionID = regionID
	}
	return string(priv.Public.Addr())
}

func TestRelayNameReadsTheAddress(t *testing.T) {
	if got := relayName(addressOn("relay-eu.ghostex.dev", 0)); got != "relay-eu.ghostex.dev" {
		t.Fatalf("relay host: got %q", got)
	}
	if got := relayName(addressOn("", 903)); got != "region 903" {
		t.Fatalf("public region: got %q", got)
	}
	if got := relayName("tcbroken"); got != "unknown" {
		t.Fatalf("broken address: got %q", got)
	}
}

func TestPathOfKeepsOnlyTheKindOfRoute(t *testing.T) {
	token := addressOn("relay-eu.ghostex.dev", 0)
	if p := pathOf(token, "", 0.04); p.Path != "relay" || p.Relay != "relay-eu.ghostex.dev" || p.LatencyMs != 40 {
		t.Fatalf("relay path: %+v", p)
	}
	if p := pathOf(token, "192.168.1.20:41641", 0.002); p.Path != "direct" || p.Route != "lan" {
		t.Fatalf("lan path: %+v", p)
	}
	if p := pathOf(token, "203.0.113.7:41641", 0.03); p.Path != "direct" || p.Route != "internet" {
		t.Fatalf("internet path: %+v", p)
	}
	if got := ForwardPath("no-such-forward"); got != "" {
		t.Fatalf("missing forward: got %q", got)
	}
}
