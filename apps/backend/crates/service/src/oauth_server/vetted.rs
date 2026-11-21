//! 接続先のアドレスの検査（特殊用途の範囲の拒否）と、検査を通ったことを表す型。
//!
//! 型を `client` と別のモジュールに置くのは、[`VettedAddrs`] を作れる場所を [`vet_addrs`] の
//! 1 箇所に限るためである（同じモジュールに在ると、フィールドを直に書いて検査を飛ばせる）。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use super::client::ClientError;

// IANA の特殊用途のアドレスの登録簿（2026-11-19 に取得した版）の全部と multicast。
// https://www.iana.org/assignments/iana-ipv4-special-registry/
// https://www.iana.org/assignments/iana-ipv6-special-registry/
// 到達できると書かれている項目（AS112、AMT など）も含めて全部拒否する。
// 親の範囲に含まれる細かい項目（192.0.0.0/29、2001::/32 など）は親だけを置く。
// 登録簿が変わったら、ここを突き合わせて直す。
const SPECIAL_USE_V4: [(Ipv4Addr, u32); 18] = [
    (Ipv4Addr::new(0, 0, 0, 0), 8),
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    (Ipv4Addr::new(192, 0, 2, 0), 24),
    (Ipv4Addr::new(192, 31, 196, 0), 24),
    (Ipv4Addr::new(192, 52, 193, 0), 24),
    (Ipv4Addr::new(192, 88, 99, 0), 24),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    (Ipv4Addr::new(192, 175, 48, 0), 24),
    (Ipv4Addr::new(198, 18, 0, 0), 15),
    (Ipv4Addr::new(198, 51, 100, 0), 24),
    (Ipv4Addr::new(203, 0, 113, 0), 24),
    // multicast
    (Ipv4Addr::new(224, 0, 0, 0), 4),
    // 予約済み。255.255.255.255/32 を含む
    (Ipv4Addr::new(240, 0, 0, 0), 4),
];

const SPECIAL_USE_V6: [(Ipv6Addr, u32); 15] = [
    (Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0), 128),
    (Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1), 128),
    (Ipv6Addr::new(0x64, 0xff9b, 0, 0, 0, 0, 0, 0), 96),
    (Ipv6Addr::new(0x64, 0xff9b, 1, 0, 0, 0, 0, 0), 48),
    (Ipv6Addr::new(0x100, 0, 0, 0, 0, 0, 0, 0), 64),
    (Ipv6Addr::new(0x100, 0, 0, 1, 0, 0, 0, 0), 64),
    (Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2002, 0, 0, 0, 0, 0, 0, 0), 16),
    (Ipv6Addr::new(0x2620, 0x4f, 0x8000, 0, 0, 0, 0, 0), 48),
    (Ipv6Addr::new(0x3fff, 0, 0, 0, 0, 0, 0, 0), 20),
    (Ipv6Addr::new(0x5f00, 0, 0, 0, 0, 0, 0, 0), 16),
    (Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 0), 7),
    (Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10),
    // multicast
    (Ipv6Addr::new(0xff00, 0, 0, 0, 0, 0, 0, 0), 8),
];

/// 特殊用途のアドレスか。CIMD は、特殊用途のアドレスへ解決する URL の取得を禁じている。
/// IPv4-mapped（`::ffff:0:0/96`）は IPv4 に戻してから判定する。
pub(super) fn is_special_use(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(ip) => {
            let bits = ip.to_bits();
            SPECIAL_USE_V4.iter().any(|(network, prefix)| {
                let mask = u32::MAX << (32 - prefix);
                bits & mask == network.to_bits() & mask
            })
        }
        IpAddr::V6(ip) => {
            let bits = ip.to_bits();
            SPECIAL_USE_V6.iter().any(|(network, prefix)| {
                let mask = u128::MAX << (128 - prefix);
                bits & mask == network.to_bits() & mask
            })
        }
    }
}

/// 接続してよいと確かめたアドレス。[`vet_addrs`] だけが作る。
///
/// 取得の関数（`client`）はこの型だけを受ける。フィールドはこのモジュールの外から触れないので、
/// 特殊用途の検査を通さずに取りに行く書き方は、型が合わない。
pub(super) struct VettedAddrs(Vec<SocketAddr>);

impl VettedAddrs {
    pub(super) fn as_slice(&self) -> &[SocketAddr] {
        &self.0
    }

    /// 手元の TCP サーバー（loopback）へ向ける単体試験のための入口。
    #[cfg(test)]
    pub(super) fn unchecked_for_test(addrs: Vec<SocketAddr>) -> Self {
        Self(addrs)
    }
}

/// 名前解決の結果を確かめる。1 つでも特殊用途なら全体を拒否する。
pub(super) fn vet_addrs(addrs: Vec<SocketAddr>) -> Result<VettedAddrs, ClientError> {
    if addrs.is_empty() || addrs.iter().any(|addr| is_special_use(addr.ip())) {
        return Err(ClientError::Unavailable);
    }
    Ok(VettedAddrs(addrs))
}
