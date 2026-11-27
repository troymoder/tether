use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Address {
    Port {
        host: String,
        port: String,
    },
    /// The mDNS name a phone announces itself under, which stands for host
    /// and port.
    Name(String),
}

impl Address {
    pub(super) fn typed(host: &str, port: &str) -> Option<Self> {
        let host = host.trim();
        let port = port.trim();

        match (host.is_empty(), port.is_empty()) {
            (true, _) => None,
            (false, true) => Some(Address::Name(host.to_owned())),
            (false, false) => Some(Address::Port {
                host: host.to_owned(),
                port: port.to_owned(),
            }),
        }
    }

    /// The address of a route as tetherd lists it: `host:port`, or a name.
    pub(super) fn listed(address: &str) -> Self {
        match address.rsplit_once(':') {
            Some((host, port)) => Address::Port {
                host: host.to_owned(),
                port: port.to_owned(),
            },
            None => Address::Name(address.to_owned()),
        }
    }

    pub(super) fn fields(self) -> (String, String) {
        match self {
            Address::Port { host, port } => (host, port),
            Address::Name(name) => (name, String::new()),
        }
    }
}

/// The address as adb takes it.
impl fmt::Display for Address {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Address::Port { host, port } => write!(out, "{host}:{port}"),
            Address::Name(name) => out.write_str(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_typed_as_a_host_and_a_port_or_as_a_name() {
        let name = "adb-R58M._adb-tls-connect._tcp";

        assert_eq!(
            Address::typed(" 192.168.1.31 ", " 40213 ").map(|address| address.to_string()),
            Some("192.168.1.31:40213".to_owned())
        );
        assert_eq!(
            Address::typed(name, ""),
            Some(Address::Name(name.to_owned()))
        );
        assert_eq!(Address::Name(name.to_owned()).to_string(), name);
        assert_eq!(Address::typed("  ", "40213"), None);
    }

    #[test]
    fn a_listed_address_fills_the_fields_it_would_be_typed_in() {
        let name = "adb-R58M._adb-tls-pairing._tcp";

        assert_eq!(
            Address::listed("192.168.1.31:37119").fields(),
            ("192.168.1.31".to_owned(), "37119".to_owned())
        );
        assert_eq!(
            Address::listed(name).fields(),
            (name.to_owned(), String::new())
        );
    }
}
