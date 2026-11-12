use std::{
    cell::RefCell,
    net::{IpAddr, SocketAddr},
    path::Path,
};

use serde_json::{json, Value};

use super::*;

const GXSERVER_PID: u32 = 4000;
const SSHD_PID: u32 = 9792;
const SERVER_PORT: u16 = 58744;
const CLIENT_PORT: u16 = 55996;
const OWN_USER: i64 = 358822019404285;
const OWN_IP: &str = "100.77.81.4";
const PHONE_IP: &str = "100.71.245.28";

/// A computer whose answers each test bends one way.
struct FakeProbe {
    on_port: Result<String, String>,
    files: Result<String, String>,
    root_owned: bool,
    parent_uid: Result<u32, String>,
    status: Value,
    whois: Result<Value, String>,
    asked_whois: RefCell<Vec<IpAddr>>,
}

fn client() -> SocketAddr {
    format!("127.0.0.1:{CLIENT_PORT}").parse().unwrap()
}

/// The lsof answer for the request's client port: sshd holds the client end, gxserver the other.
fn on_port(command: &str, pid: u32) -> String {
    format!(
        "p{pid}\nc{command}\nf12\nn127.0.0.1:{CLIENT_PORT}->127.0.0.1:{SERVER_PORT}\n\
         p{GXSERVER_PID}\ncgxserver\nf44\nn127.0.0.1:{SERVER_PORT}->127.0.0.1:{CLIENT_PORT}\n"
    )
}

/// The SSH login's executable and its sockets: the login from `source`, and its forwards.
fn session_files(source: &str) -> String {
    let source: IpAddr = source.parse().unwrap();
    let ssh = match source {
        IpAddr::V4(_) => format!("{OWN_IP}:22->{source}:52090"),
        IpAddr::V6(_) => format!("[fd7a:115c:a1e0::a601:51ab]:22->[{source}]:52090"),
    };
    format!(
        "p{SSHD_PID}\ncsshd-session\nftxt\nn/usr/libexec/sshd-session\n\
         f7\nn{ssh}\nf8\nn{ssh}\nf12\nn127.0.0.1:{CLIENT_PORT}->127.0.0.1:{SERVER_PORT}\n"
    )
}

fn status() -> Value {
    json!({
        "BackendState": "Running",
        "Self": {
            "ID": "nz9Qd3csZX11CNTRL",
            "UserID": OWN_USER,
            "Tags": null,
            "TailscaleIPs": [OWN_IP, "fd7a:115c:a1e0::a601:51ab"],
        },
    })
}

fn whois_of(ip: &str, user: i64) -> Value {
    json!({
        "Node": {
            "ID": 1234,
            "StableID": "nPhone1CNTRL",
            "Name": "madda-phone.tail1234.ts.net.",
            "ComputedName": "madda-phone",
            "User": user,
            "Sharer": null,
            "Tags": null,
            "Addresses": [format!("{ip}/32"), "fd7a:115c:a1e0::1/128"],
        },
        "UserProfile": { "ID": user, "LoginName": "owner@example.com" },
        "CapMap": null,
    })
}

impl FakeProbe {
    /// The owner's phone, signed in over sshd from its own tailnet address.
    fn own_phone() -> Self {
        Self {
            on_port: Ok(on_port("sshd-session", SSHD_PID)),
            files: Ok(session_files(PHONE_IP)),
            root_owned: true,
            parent_uid: Ok(0),
            status: status(),
            whois: Ok(whois_of(PHONE_IP, OWN_USER)),
            asked_whois: RefCell::new(Vec::new()),
        }
    }
}

impl TailnetProbe for FakeProbe {
    fn processes_on_port(&self, port: u16) -> Result<String, String> {
        assert_eq!(port, CLIENT_PORT);
        self.on_port.clone()
    }
    fn process_files(&self, pid: u32) -> Result<String, String> {
        assert_eq!(pid, SSHD_PID);
        self.files.clone()
    }
    fn root_owned(&self, path: &Path) -> bool {
        assert_eq!(path, Path::new("/usr/libexec/sshd-session"));
        self.root_owned
    }
    fn parent_uid(&self, pid: u32) -> Result<u32, String> {
        assert_eq!(pid, SSHD_PID);
        self.parent_uid.clone()
    }
    fn tailscale(&self, peer_ip: IpAddr) -> Result<(Value, Value), String> {
        self.asked_whois.borrow_mut().push(peer_ip);
        self.whois.clone().map(|whois| (self.status.clone(), whois))
    }
}

fn run(probe: &FakeProbe) -> TailnetOwnerCheck {
    check_with(probe, Some(client()), SERVER_PORT, GXSERVER_PID)
}

fn assert_asks(check: TailnetOwnerCheck, why: &str) {
    match check {
        TailnetOwnerCheck::Ask { reason } => {
            assert!(
                reason.contains(why),
                "asked because {reason:?}, expected {why:?}"
            )
        }
        owned => panic!("expected a click ({why}), got {owned:?}"),
    }
}

#[test]
fn the_owners_own_tailnet_phone_needs_no_click() {
    let probe = FakeProbe::own_phone();
    assert_eq!(
        run(&probe),
        TailnetOwnerCheck::Owned {
            peer_ip: PHONE_IP.parse().unwrap(),
            node_name: "madda-phone".to_string(),
        }
    );
    // whois is asked about the address the SSH login came from, not anything the request said.
    assert_eq!(
        *probe.asked_whois.borrow(),
        vec![PHONE_IP.parse::<IpAddr>().unwrap()]
    );
}

#[test]
fn an_owners_phone_on_the_tailnets_ipv6_address_needs_no_click() {
    let phone = "fd7a:115c:a1e0::1";
    let probe = FakeProbe {
        files: Ok(session_files(phone)),
        whois: Ok(whois_of(PHONE_IP, OWN_USER)),
        ..FakeProbe::own_phone()
    };
    assert!(
        matches!(run(&probe), TailnetOwnerCheck::Owned { peer_ip, .. } if peer_ip == phone.parse::<IpAddr>().unwrap())
    );
}

#[test]
fn a_login_from_this_computers_own_tailscale_address_asks() {
    // An agent here running `ssh <own tailnet ip>` and pairing through that login.
    let probe = FakeProbe {
        files: Ok(session_files(OWN_IP)),
        whois: Ok(whois_of(OWN_IP, OWN_USER)),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "this computer's own Tailscale address");
}

#[test]
fn a_login_whose_whois_is_this_computer_asks() {
    let mut whois = whois_of(PHONE_IP, OWN_USER);
    whois["Node"]["StableID"] = json!("nz9Qd3csZX11CNTRL");
    let probe = FakeProbe {
        whois: Ok(whois),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "this computer itself");
}

#[test]
fn a_login_from_loopback_asks_without_asking_tailscale() {
    // `ssh localhost`: the SSH login's socket is a loopback one, never a tailnet address.
    let probe = FakeProbe {
        files: Ok(format!(
            "p{SSHD_PID}\ncsshd-session\nftxt\nn/usr/libexec/sshd-session\n\
             f7\nn127.0.0.1:22->127.0.0.1:50000\nf12\nn127.0.0.1:{CLIENT_PORT}->127.0.0.1:{SERVER_PORT}\n"
        )),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "did not come from a tailnet address");
    assert!(probe.asked_whois.borrow().is_empty());
}

#[test]
fn a_login_from_the_local_network_asks() {
    let probe = FakeProbe {
        files: Ok(session_files("192.168.1.20")),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "did not come from a tailnet address");
}

#[test]
fn a_node_shared_in_from_another_tailnet_asks() {
    let mut whois = whois_of(PHONE_IP, OWN_USER);
    whois["Node"]["Sharer"] = json!(777);
    let probe = FakeProbe {
        whois: Ok(whois),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "shared in");
}

#[test]
fn a_node_of_another_user_asks() {
    let probe = FakeProbe {
        whois: Ok(whois_of(PHONE_IP, 1111)),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "another Tailscale user");
}

#[test]
fn a_whois_whose_profile_names_another_user_asks() {
    let mut whois = whois_of(PHONE_IP, OWN_USER);
    whois["UserProfile"]["ID"] = json!(1111);
    let probe = FakeProbe {
        whois: Ok(whois),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "another user");
}

#[test]
fn a_tagged_node_asks() {
    let mut whois = whois_of(PHONE_IP, OWN_USER);
    whois["Node"]["Tags"] = json!(["tag:server"]);
    let probe = FakeProbe {
        whois: Ok(whois),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "tagged Tailscale device");
}

#[test]
fn a_tagged_computer_asks() {
    let mut status = status();
    status["Self"]["Tags"] = json!(["tag:server"]);
    let probe = FakeProbe {
        status,
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "this computer is a tagged");
}

#[test]
fn a_whois_failure_asks() {
    let probe = FakeProbe {
        whois: Err("no root-owned Tailscale CLI with a running backend".to_string()),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "Tailscale could not tell");
}

#[test]
fn a_whois_for_another_address_asks() {
    let probe = FakeProbe {
        whois: Ok(whois_of("100.100.1.1", OWN_USER)),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "another address");
}

#[test]
fn tailscale_stopped_on_this_computer_asks() {
    let mut status = status();
    status["BackendState"] = json!("Stopped");
    let probe = FakeProbe {
        status,
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "not running");
}

#[test]
fn a_request_from_another_process_than_sshd_asks() {
    // An agent's own `curl` with the token file, claiming nothing: the socket's holder decides.
    let probe = FakeProbe {
        on_port: Ok(on_port("curl", SSHD_PID)),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "not an SSH login");
}

#[test]
fn an_sshd_the_user_started_asks() {
    // `sshd` run by the user (not the system's): its parent is no root process.
    let probe = FakeProbe {
        parent_uid: Ok(501),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "not started by the system");
}

#[test]
fn an_sshd_binary_not_owned_by_root_asks() {
    let probe = FakeProbe {
        root_owned: false,
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "not the system's");
}

#[test]
fn a_request_not_from_loopback_or_of_unknown_address_asks() {
    let probe = FakeProbe::own_phone();
    let lan: SocketAddr = "192.168.1.20:5000".parse().unwrap();
    assert_asks(
        check_with(&probe, Some(lan), SERVER_PORT, GXSERVER_PID),
        "SSH forward",
    );
    assert_asks(
        check_with(&probe, None, SERVER_PORT, GXSERVER_PID),
        "unknown",
    );
}

#[test]
fn a_connection_to_another_port_or_held_only_by_gxserver_asks() {
    let probe = FakeProbe::own_phone();
    assert_asks(
        check_with(&probe, Some(client()), SERVER_PORT + 1, GXSERVER_PID),
        "no other process",
    );
    assert_asks(
        check_with(&probe, Some(client()), SERVER_PORT, SSHD_PID),
        "no other process",
    );
}

#[test]
fn an_ssh_login_with_several_tailnet_peers_asks() {
    let probe = FakeProbe {
        files: Ok(format!(
            "{}f9\nn{OWN_IP}:22->100.70.1.1:40000\n",
            session_files(PHONE_IP)
        )),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "several tailnet addresses");
}

#[test]
fn listing_failures_ask() {
    let probe = FakeProbe {
        on_port: Err("lsof: not found".to_string()),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "could not list sockets");
    let probe = FakeProbe {
        parent_uid: Err("the parent process ended".to_string()),
        ..FakeProbe::own_phone()
    };
    assert_asks(run(&probe), "parent");
}

#[test]
fn tailnet_addresses_are_the_cgnat_range_and_tailscales_ipv6_prefix() {
    for ip in ["100.64.0.1", "100.127.255.254", "fd7a:115c:a1e0::1"] {
        assert!(is_tailnet_ip(ip.parse().unwrap()), "{ip}");
    }
    for ip in [
        "100.63.255.255",
        "100.128.0.1",
        "10.0.0.1",
        "127.0.0.1",
        "fd7a:115c:a1e1::1",
        "::1",
    ] {
        assert!(!is_tailnet_ip(ip.parse().unwrap()), "{ip}");
    }
}

#[test]
fn lsof_output_parses_ipv6_and_the_executable() {
    let processes = parse_lsof(
        "p12\ncsshd-session\nftxt\nn/usr/libexec/sshd-session\nftxt\nn/usr/lib/dyld\n\
         f7\nn[fd7a:115c:a1e0::a601:51ab]:22->[fd7a:115c:a1e0::1]:52090\nf8\nn*:22\n",
    );
    assert_eq!(processes.len(), 1);
    assert_eq!(processes[0].command, "sshd-session");
    assert_eq!(processes[0].files[0], "/usr/libexec/sshd-session");
    assert_eq!(processes[0].connections.len(), 1);
    assert_eq!(
        processes[0].connections[0].1,
        "[fd7a:115c:a1e0::1]:52090".parse::<SocketAddr>().unwrap()
    );
}

/// The whole check against this computer's real sockets and Tailscale, read-only: lsof, ps and a
/// local whois. Run by hand with `GX_TRACE_CLIENT=127.0.0.1:<port>` (a live SSH forward's client
/// end), `GX_TRACE_SERVER_PORT` and `GX_TRACE_SERVER_PID`, and `--ignored --nocapture`.
#[test]
#[ignore]
fn live_trace_of_an_existing_ssh_forward() {
    let client: SocketAddr = std::env::var("GX_TRACE_CLIENT").unwrap().parse().unwrap();
    let port: u16 = std::env::var("GX_TRACE_SERVER_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let pid: u32 = std::env::var("GX_TRACE_SERVER_PID")
        .unwrap()
        .parse()
        .unwrap();
    let check = check_with(&SystemProbe, Some(client), port, pid);
    println!("live check: {check:?}");
}
