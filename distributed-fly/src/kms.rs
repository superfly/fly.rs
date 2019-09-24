extern crate rusoto_kms;
use self::rusoto_kms::{DecryptRequest, Kms, KmsClient};

use rusoto_core::Region;

use super::REDIS_POOL;
use r2d2_redis::redis;
use sha2::{Digest, Sha256};

lazy_static! {
    static ref KMS_CLIENT: KmsClient = KmsClient::new(Region::UsEast1);
}

static CACHE_PREFIX: &str = "local:kms:";

pub fn decrypt(blob: Vec<u8>) -> Result<Option<Vec<u8>>, rusoto_kms::DecryptError> {
    debug!("decrypting value via KMS");
    if let Some(plaintext) = plaintext_cache(&blob) {
        debug!("found in cache, aborting decryption");
        return Ok(Some(plaintext));
    }
    let req = DecryptRequest {
        ciphertext_blob: blob.clone(),
        encryption_context: None,
        grant_tokens: None,
    };
    let res = KMS_CLIENT.decrypt(req).sync()?;
    if let Some(plaintext) = res.plaintext {
        set_plaintext_cache(&blob, &plaintext);
        Ok(Some(plaintext))
    } else {
        Ok(None)
    }
}

fn format_cache_key(blob: &Vec<u8>) -> String {
    format!("{}{:x}", CACHE_PREFIX, Sha256::digest(blob.as_slice()))
}

fn set_plaintext_cache(blob: &Vec<u8>, plaintext: &Vec<u8>) {
    if let Ok(conn) = REDIS_POOL.get() {
        if let Err(e) = redis::cmd("SET")
            .arg(format_cache_key(blob))
            .arg(plaintext.as_slice())
            .query::<()>(&*conn)
        {
            error!("error setting kms cache key: {}", e);
        }
    }
}

fn plaintext_cache(blob: &Vec<u8>) -> Option<Vec<u8>> {
    match REDIS_POOL.get() {
        Err(e) => {
            error!("error getting redis conn for kms: {}", e);
            None
        }
        Ok(conn) => {
            match redis::cmd("GET")
                .arg(format_cache_key(blob))
                .query::<Option<Vec<u8>>>(&*conn)
            {
                Err(e) => {
                    error!("error querying redis for kms: {}", e);
                    None
                }
                Ok(maybe_vec) => match maybe_vec {
                    Some(vec) => Some(vec),
                    None => None,
                },
            }
        }
    }
}
