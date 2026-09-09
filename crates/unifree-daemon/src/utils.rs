use std::net::{IpAddr, UdpSocket};

/// Get the local IP address used to reach a given target
pub fn get_local_ip_for(target: IpAddr) -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // Connecting resolves the route without sending anything
    socket.connect((target, 80)).ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

/// Pick the inform URL for a device, deriving it from the route when not overridden
pub fn choose_inform_url(override_url: Option<String>, ip: IpAddr, port: u16) -> Option<String> {
    match override_url {
        Some(url) => Some(url),
        None => get_local_ip_for(ip).map(|local| format!("http://{}:{}/inform", local, port)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
    // Forces connect() to fail with EAFNOSUPPORT against the v4 socket, which is
    // the only portable way to reach the None branch; no v6 route is involved
    const WRONG_FAMILY: IpAddr = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));

    #[test]
    fn test_get_local_ip_for_loopback() {
        let src = get_local_ip_for(LOOPBACK);
        assert_eq!(src.as_deref(), Some("127.0.0.1"));
    }

    #[test]
    fn test_get_local_ip_for_wrong_family() {
        assert_eq!(get_local_ip_for(WRONG_FAMILY), None);
    }

    #[test]
    fn test_choose_inform_url_prefers_override() {
        let url = choose_inform_url(Some("http://example:9/inform".to_string()), WRONG_FAMILY, 80);
        assert_eq!(url.as_deref(), Some("http://example:9/inform"));
    }

    #[test]
    fn test_choose_inform_url_derives_from_route() {
        let url = choose_inform_url(None, LOOPBACK, 8080);
        assert_eq!(url.as_deref(), Some("http://127.0.0.1:8080/inform"));
    }

    #[test]
    fn test_choose_inform_url_none_when_unroutable() {
        assert_eq!(choose_inform_url(None, WRONG_FAMILY, 8080), None);
    }
}
