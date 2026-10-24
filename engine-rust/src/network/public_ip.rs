//! Global-address semantics of the retained CPython 3.13 source contract.
use std::net::IpAddr;

fn in_prefix(address: u128, prefix: u128, bits: u32, width: u32) -> bool {
    address >> (width - bits) == prefix >> (width - bits)
}

pub fn is_global(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let address = u128::from(ip.to_bits());
            if matches!(ip.octets(), [192, 0, 0, 9 | 10]) {
                return true;
            }
            ![
                (0x00000000, 8),
                (0x0a000000, 8),
                (0x64400000, 10),
                (0x7f000000, 8),
                (0xa9fe0000, 16),
                (0xac100000, 12),
                (0xc0000000, 24),
                (0xc0000200, 24),
                (0xc0a80000, 16),
                (0xc6120000, 15),
                (0xc6336400, 24),
                (0xcb007100, 24),
                (0xf0000000, 4),
            ]
            .into_iter()
            .any(|(prefix, bits)| in_prefix(address, prefix, bits, 32))
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_global(IpAddr::V4(mapped));
            }
            let address = ip.to_bits();
            if [
                (0x20010001000000000000000000000001, 128),
                (0x20010001000000000000000000000002, 128),
                (0x20010003000000000000000000000000, 32),
                (0x20010004011200000000000000000000, 48),
                (0x20010020000000000000000000000000, 28),
                (0x20010030000000000000000000000000, 28),
            ]
            .into_iter()
            .any(|(prefix, bits)| in_prefix(address, prefix, bits, 128))
            {
                return true;
            }
            ![
                (0, 128),
                (1, 128),
                (0x0064ff9b000100000000000000000000, 48),
                (0x01000000000000000000000000000000, 64),
                (0x20010000000000000000000000000000, 23),
                (0x20010db8000000000000000000000000, 32),
                (0x20020000000000000000000000000000, 16),
                (0x3fff0000000000000000000000000000, 20),
                (0xfc000000000000000000000000000000, 7),
                (0xfe800000000000000000000000000000, 10),
            ]
            .into_iter()
            .any(|(prefix, bits)| in_prefix(address, prefix, bits, 128))
        }
    }
}
