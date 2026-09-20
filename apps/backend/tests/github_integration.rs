//! GitHub App Wave 0 統合テスト（Webhook 署名）。

use backend::handlers::github::verify_webhook_signature;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn sign_payload(secret: &str, body: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(body);
    let digest = mac.finalize().into_bytes();
    format!("sha256={}", hex::encode(digest))
}

#[test]
fn test_webhook_signature_validation() {
    let secret = "webhook-secret";
    let body = br#"{"installation":{"id":42}}"#;
    let signature = sign_payload(secret, body);
    assert!(verify_webhook_signature(secret, &signature, body));
    assert!(!verify_webhook_signature(secret, "sha256=deadbeef", body));
    assert!(!verify_webhook_signature("wrong-secret", &signature, body));
}
