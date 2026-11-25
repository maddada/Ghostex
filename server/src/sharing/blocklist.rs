//! Strict port blocklist for localhost port sharing.
//!
//! CDXC:Sharing 2026-11-24 DECISION:
//! Hard security rule: blocklist gxserver's port, Ghostex internal ports, SSH, databases, and
//! other sensitive system services so they can never be exposed over a public tunnel.

use std::collections::HashSet;

use crate::constants::{GXSERVER_LOCAL_API_PORT, GXSERVER_REMOTE_API_PORT};

/// Known sensitive service / database / system ports that can never be shared publicly.
const BLOCKED_PORTS: &[(u16, &str)] = &[
    // SSH & remote administration
    (22, "SSH"),
    (23, "Telnet"),
    (25, "SMTP"),
    (53, "DNS"),
    (111, "RPC"),
    (135, "Windows RPC"),
    (139, "NetBIOS"),
    (445, "SMB"),
    (548, "AFP"),
    (631, "CUPS printing"),
    (2222, "SSH"),
    (3389, "Remote Desktop"),
    (5900, "VNC"),
    (5901, "VNC"),
    // Databases, caches, message brokers & search engines
    (1433, "SQL Server"),
    (1521, "Oracle DB"),
    (3306, "MySQL"),
    (33060, "MySQL"),
    (5432, "PostgreSQL"),
    (5433, "PostgreSQL"),
    (5672, "RabbitMQ"),
    (5984, "CouchDB"),
    (6379, "Redis"),
    (6380, "Redis"),
    (7000, "Cassandra"),
    (7001, "Cassandra"),
    (7474, "Neo4j"),
    (7687, "Neo4j"),
    (8086, "InfluxDB"),
    (8123, "ClickHouse"),
    (9000, "ClickHouse / internal service"),
    (9042, "Cassandra"),
    (9092, "Kafka"),
    (9200, "Elasticsearch"),
    (9300, "Elasticsearch"),
    (11211, "Memcached"),
    (15672, "RabbitMQ"),
    (27017, "MongoDB"),
    (27018, "MongoDB"),
    (27019, "MongoDB"),
    (28015, "RethinkDB"),
];

/// Ghostex internal port range (gxserver local/remote APIs, peer listener, collab shim, etc.).
const GHOSTEX_INTERNAL_PORT_MIN: u16 = 58744;
const GHOSTEX_INTERNAL_PORT_MAX: u16 = 58760;

/// Process name substrings that are never allowed to be shared.
const BLOCKED_PROCESS_PATTERNS: &[(&str, &str)] = &[
    ("gxserver", "Ghostex internal service"),
    ("ghostex", "Ghostex internal service"),
    ("sshd", "SSH"),
    ("postgres", "PostgreSQL"),
    ("mysqld", "MySQL"),
    ("mariadbd", "MariaDB"),
    ("redis-server", "Redis"),
    ("mongod", "MongoDB"),
    ("memcached", "Memcached"),
];

/// Returns `Err(human_reason)` if `port` must never be shared through a public tunnel.
pub(crate) fn check_port_allowed(
    port: u16,
    gxserver_port: u16,
    active_proxy_ports: &HashSet<u16>,
    process_name: Option<&str>,
) -> Result<(), String> {
    if port == 0 {
        return Err("Port 0 is not valid.".to_string());
    }
    if port == gxserver_port
        || port == GXSERVER_LOCAL_API_PORT
        || port == GXSERVER_REMOTE_API_PORT
        || (GHOSTEX_INTERNAL_PORT_MIN..=GHOSTEX_INTERNAL_PORT_MAX).contains(&port)
    {
        return Err(format!(
            "Port {port} is a Ghostex internal service port and cannot be shared."
        ));
    }
    if active_proxy_ports.contains(&port) {
        return Err(format!(
            "Port {port} is an internal Ghostex share proxy and cannot be shared."
        ));
    }
    if let Some((_, label)) = BLOCKED_PORTS.iter().find(|(blocked, _)| *blocked == port) {
        return Err(format!(
            "Port {port} ({label}) is protected and cannot be shared."
        ));
    }
    if let Some(name) = process_name {
        let lower = name.to_ascii_lowercase();
        if let Some((_, label)) = BLOCKED_PROCESS_PATTERNS
            .iter()
            .find(|(pattern, _)| lower.contains(pattern))
        {
            return Err(format!(
                "Port {port} belongs to {label} and cannot be shared."
            ));
        }
    }
    Ok(())
}
