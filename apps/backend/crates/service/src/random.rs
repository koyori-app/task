//! 秘密に使う乱数。OS の乱数を直に取る（`rand::rngs::SysRng`。ThreadRng のように溜めず、毎回 OS に
//! 聞く）。取れなければ panic する（以前の `OsRng.fill_bytes` と同じ）。
//!
//! 乱数を argon2 の再輸出（`argon2::password_hash::rand_core`）から取っていた頃は、argon2 と
//! password-hash を上げるたびに乱数の型が変わった。乱数はここ一箇所で取り、依存の更新から切り離す。

use rand::{TryRng, rngs::SysRng};

/// `buf` を OS の乱数で埋める。
pub fn fill(buf: &mut [u8]) {
    SysRng.try_fill_bytes(buf).expect("OS の乱数が取れぬ");
}

/// OS の乱数で埋めた `N` バイトを返す。
pub fn bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    fill(&mut buf);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_fresh_each_time() {
        assert_ne!(bytes::<32>(), bytes::<32>());
        assert_ne!(bytes::<32>(), [0u8; 32]);
    }

    #[test]
    fn fill_overwrites_the_whole_buffer() {
        // 32 バイトがすべて 0 のままである確率は 2^-256。
        let mut buf = [0u8; 32];
        fill(&mut buf);
        assert_ne!(buf, [0u8; 32]);
    }
}
