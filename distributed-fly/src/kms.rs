extern crate rusoto_kms;
use self::rusoto_kms::{DecryptRequest, Kms, KmsClient};

use rusoto_core::Region;

lazy_static! {
  static ref KMS_CLIENT: KmsClient = KmsClient::new(Region::UsEast1);
}

pub fn decrypt(blob: Vec<u8>) -> Vec<u8> {
  let req = DecryptRequest {
    ciphertext_blob: blob,
    encryption_context: None,
    grant_tokens: None,
  };
  let res = KMS_CLIENT.decrypt(req).sync().unwrap();
  res.plaintext.unwrap()
}
