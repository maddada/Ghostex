package tailcatbridge

// Network interface enumeration for hosts whose OS denies it to Go.
//
// Every tailcat client builds a tailscale netmon first, and netmon's very first
// act is to snapshot the machine's interfaces through the standard library's
// net.Interfaces(). On Linux that is an RTM_GETLINK netlink request, and
// Android 11 (API 30) forbids apps from making it: the call fails with
// "route ip+net: netlinkrib: permission denied", netmon.New returns that error,
// and StartForward reports every paired machine as unreachable.
//
// java.net.NetworkInterface reads the same data through bionic and the
// framework rather than netlink, so it keeps working. The host app therefore
// enumerates the interfaces itself and installs the result through
// SetInterfaceLister, exactly like the Tailscale Android app does with
// netmon.RegisterInterfaceGetter. iOS has no such restriction and never calls
// SetInterfaceLister, so it keeps the standard library path.

import (
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net"
	"net/netip"
	"slices"
	"strings"
	"sync"

	"tailscale.com/net/netmon"
)

// InterfaceLister is implemented by the host app (in Java/Kotlin, through
// gomobile's reverse bindings) to enumerate the device's network interfaces.
type InterfaceLister interface {
	// InterfacesAsJson returns a JSON array describing every interface on the
	// device:
	//
	//	[{"name":"wlan0","index":12,"mtu":1500,"up":true,"broadcast":true,
	//	  "loopback":false,"pointToPoint":false,"multicast":true,
	//	  "addrs":[{"ip":"10.0.2.16","prefixLen":24}]}]
	//
	// "ip" is an unbracketed textual address as produced by
	// InetAddress.getHostAddress(); an IPv6 scope stays on it as a "%zone"
	// suffix. "prefixLen" is InterfaceAddress.getNetworkPrefixLength().
	// Returning an error, or a payload that does not parse, fails the lookup:
	// a partial interface list would silently mis-describe the network.
	InterfacesAsJson() (string, error)
}

// SetInterfaceLister makes l the source of truth for the device's network
// interfaces, for this process, from now on.
//
// It must run before any tailcat client is created, because a client snapshots
// the interfaces while it is being built. The Android module calls this from
// its Expo OnCreate hook, i.e. while the native module registry is still being
// assembled — every tailcat entry point is an async function on that same
// module, so JavaScript cannot have asked for a connection yet and the
// ordering holds by construction. That is why there is no ordering check here.
func SetInterfaceLister(l InterfaceLister) {
	if l == nil {
		log.Printf("tailcat: SetInterfaceLister got no lister; keeping the standard library interface lookup")
		return
	}
	netmon.RegisterInterfaceGetter(func() ([]netmon.Interface, error) {
		return interfacesFromLister(l)
	})
	log.Printf("tailcat: host interface lister registered")
}

// listerAddr is one address of one interface, as the host app reports it.
type listerAddr struct {
	IP        string `json:"ip"`
	PrefixLen int    `json:"prefixLen"`
}

// listerInterface is one interface, as the host app reports it. The booleans
// are the net.Flags bits that netmon reads.
type listerInterface struct {
	Name         string       `json:"name"`
	Index        int          `json:"index"`
	MTU          int          `json:"mtu"`
	Up           bool         `json:"up"`
	Broadcast    bool         `json:"broadcast"`
	Loopback     bool         `json:"loopback"`
	PointToPoint bool         `json:"pointToPoint"`
	Multicast    bool         `json:"multicast"`
	Addrs        []listerAddr `json:"addrs"`
}

// loggedFirstListing keeps the netmon poll loop, which re-reads the interfaces
// every few seconds, from repeating the same summary line forever.
var loggedFirstListing sync.Once

func interfacesFromLister(l InterfaceLister) ([]netmon.Interface, error) {
	payload, err := l.InterfacesAsJson()
	if err != nil {
		log.Printf("tailcat: host interface lister failed: %v", err)
		return nil, fmt.Errorf("host interface lister failed: %w", err)
	}
	payload = strings.TrimSpace(payload)
	if payload == "" {
		log.Printf("tailcat: host interface lister returned an empty payload")
		return nil, errors.New("host interface lister returned an empty payload")
	}

	var raw []listerInterface
	if err := json.Unmarshal([]byte(payload), &raw); err != nil {
		log.Printf("tailcat: host interface lister payload (%d bytes) is not a JSON interface array: %v", len(payload), err)
		return nil, fmt.Errorf("host interface lister payload is malformed: %w", err)
	}

	out := make([]netmon.Interface, 0, len(raw))
	addrCount := 0
	for i, in := range raw {
		if in.Name == "" {
			log.Printf("tailcat: host interface lister entry %d has no name", i)
			return nil, fmt.Errorf("host interface lister entry %d has no name", i)
		}
		iface := netmon.Interface{
			Interface: &net.Interface{Name: in.Name, Index: in.Index, MTU: in.MTU},
			// AltAddrs has to stay non-nil even when the interface has no
			// addresses: netmon.Interface.Addrs falls through to
			// net.Interface.Addrs when it is nil, which is the netlink call
			// this whole detour exists to avoid.
			AltAddrs: []net.Addr{},
		}
		if in.Up {
			iface.Flags |= net.FlagUp
		}
		if in.Broadcast {
			iface.Flags |= net.FlagBroadcast
		}
		if in.Loopback {
			iface.Flags |= net.FlagLoopback
		}
		if in.PointToPoint {
			iface.Flags |= net.FlagPointToPoint
		}
		if in.Multicast {
			iface.Flags |= net.FlagMulticast
		}
		for _, a := range in.Addrs {
			addr, err := a.netAddr()
			if err != nil {
				log.Printf("tailcat: host interface lister gave %s a malformed address: %v", in.Name, err)
				return nil, fmt.Errorf("host interface lister entry %s: %w", in.Name, err)
			}
			iface.AltAddrs = append(iface.AltAddrs, addr)
		}
		addrCount += len(iface.AltAddrs)
		out = append(out, iface)
	}
	loggedFirstListing.Do(func() {
		log.Printf("tailcat: host interface lister reported %d interfaces with %d addresses", len(out), addrCount)
	})
	return out, nil
}

// netAddr converts one reported address into the net.Addr shapes netmon
// understands: *net.IPNet for a plain address with its prefix, and *net.IPAddr
// for a zoned one, because *net.IPNet cannot carry an IPv6 zone.
func (a listerAddr) netAddr() (net.Addr, error) {
	ip, err := netip.ParseAddr(a.IP)
	if err != nil {
		return nil, fmt.Errorf("address %q: %w", a.IP, err)
	}
	raw := net.IP(slices.Clone(ip.AsSlice()))
	if zone := ip.Zone(); zone != "" {
		return &net.IPAddr{IP: raw, Zone: zone}, nil
	}
	bits := ip.BitLen()
	if a.PrefixLen < 0 || a.PrefixLen > bits {
		return nil, fmt.Errorf("address %s has prefix length %d, outside /0../%d", a.IP, a.PrefixLen, bits)
	}
	return &net.IPNet{IP: raw, Mask: net.CIDRMask(a.PrefixLen, bits)}, nil
}
